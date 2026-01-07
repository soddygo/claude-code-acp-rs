//! ACP request handlers
//!
//! Implements handlers for ACP protocol requests:
//! - initialize: Return agent capabilities
//! - session/new: Create a new session
//! - session/prompt: Execute a prompt (Phase 1: simplified)
//! - session/setMode: Set permission mode

use std::sync::Arc;

use futures::StreamExt;
use sacp::schema::{
    AgentCapabilities, ContentBlock, CurrentModeUpdate, Implementation, InitializeRequest,
    InitializeResponse, ModelId, ModelInfo, NewSessionRequest, NewSessionResponse,
    PromptCapabilities, PromptRequest, PromptResponse, SessionId, SessionMode, SessionModeId,
    SessionModeState, SessionModelState, SessionNotification, SessionUpdate,
    SetSessionModeRequest, SetSessionModeResponse, SetSessionModelRequest, SetSessionModelResponse,
    StopReason, TextContent,
};
use sacp::JrConnectionCx;

use crate::session::{PermissionMode, SessionManager};
use crate::terminal::TerminalClient;
use crate::types::{AgentConfig, AgentError, NewSessionMeta};

/// Handle initialize request
///
/// Returns the agent's capabilities and protocol version.
#[tracing::instrument(skip(request, _config), fields(protocol_version = ?request.protocol_version))]
pub fn handle_initialize(
    request: InitializeRequest,
    _config: &AgentConfig,
) -> InitializeResponse {
    // Build agent capabilities using builder pattern
    let prompt_caps = PromptCapabilities::new()
        .image(true)
        .embedded_context(true);

    let capabilities = AgentCapabilities::new()
        .prompt_capabilities(prompt_caps);

    // Build agent info
    let agent_info = Implementation::new("claude-code-acp-rs", env!("CARGO_PKG_VERSION"))
        .title("Claude Code");

    // Build response
    InitializeResponse::new(request.protocol_version)
        .agent_capabilities(capabilities)
        .agent_info(agent_info)
}

/// Handle session/new request
///
/// Creates a new session with the given working directory and metadata.
/// Returns available modes and models for the session.
#[tracing::instrument(skip(request, config, sessions), fields(cwd = ?request.cwd))]
pub fn handle_new_session(
    request: NewSessionRequest,
    config: &AgentConfig,
    sessions: &Arc<SessionManager>,
) -> Result<NewSessionResponse, AgentError> {
    // Parse metadata from request if present
    let meta = request.meta.as_ref().and_then(|m| {
        serde_json::to_value(m)
            .ok()
            .map(|v| NewSessionMeta::from_request_meta(Some(&v)))
    });

    // Get working directory from request
    let cwd = request.cwd;

    // Generate session ID
    let session_id = uuid::Uuid::new_v4().to_string();

    // Create the session
    sessions.create_session(session_id.clone(), cwd, config, meta.as_ref())?;

    // Build available modes
    let available_modes = build_available_modes();
    let mode_state = SessionModeState::new("default", available_modes);

    // Build available models
    let available_models = build_available_models();
    let default_model = config.model.clone().unwrap_or_else(|| "default".to_string());
    let model_state = SessionModelState::new(default_model, available_models);

    Ok(NewSessionResponse::new(session_id)
        .modes(mode_state)
        .models(model_state))
}

/// Build available permission modes
///
/// Returns the list of permission modes available in the agent.
fn build_available_modes() -> Vec<SessionMode> {
    vec![
        SessionMode::new("default", "Default")
            .description("Standard behavior, prompts for dangerous operations"),
        SessionMode::new("acceptEdits", "Accept Edits")
            .description("Auto-accept file edit operations"),
        SessionMode::new("plan", "Plan Mode")
            .description("Planning mode, no actual tool execution"),
        SessionMode::new("dontAsk", "Don't Ask")
            .description("Don't prompt for permissions, deny if not pre-approved"),
        SessionMode::new("bypassPermissions", "Bypass Permissions")
            .description("Bypass all permission checks"),
    ]
}

/// Build available models
///
/// Returns the list of models available in the agent.
/// By default, returns a single "default" model (like the official claude-code-acp).
///
/// Custom models can be configured via the `ACP_MODELS` environment variable,
/// with format: `model_id:display_name,model_id2:display_name2`
///
/// Example:
/// ```sh
/// ACP_MODELS="glm-4:GLM 4,glm-4.7:GLM 4.7"
/// ```
fn build_available_models() -> Vec<ModelInfo> {
    // Check for custom models from environment
    if let Ok(custom_models) = std::env::var("ACP_MODELS") {
        let models: Vec<ModelInfo> = custom_models
            .split(',')
            .filter_map(|s| {
                let parts: Vec<&str> = s.trim().splitn(2, ':').collect();
                if parts.len() == 2 {
                    Some(ModelInfo::new(
                        ModelId::new(parts[0].trim()),
                        parts[1].trim(),
                    ))
                } else if !parts.is_empty() && !parts[0].is_empty() {
                    // Just model ID, use it as display name too
                    Some(ModelInfo::new(
                        ModelId::new(parts[0].trim()),
                        parts[0].trim(),
                    ))
                } else {
                    None
                }
            })
            .collect();

        if !models.is_empty() {
            return models;
        }
    }

    // Default: single "default" model (matches official claude-code-acp behavior)
    vec![ModelInfo::new(ModelId::new("default"), "Default")]
}

/// Handle session/prompt request
///
/// Sends the prompt to Claude and streams responses back as notifications.
#[tracing::instrument(skip(request, _config, sessions, connection_cx), fields(session_id = %request.session_id.0))]
pub async fn handle_prompt(
    request: PromptRequest,
    _config: &AgentConfig,
    sessions: &Arc<SessionManager>,
    connection_cx: JrConnectionCx,
) -> Result<PromptResponse, AgentError> {
    let session_id = request.session_id.0.as_ref();
    let session = sessions.get_session_or_error(session_id)?;

    // Configure ACP MCP server with connection and terminal client
    // This enables tools like Bash to send terminal updates
    let terminal_client = Arc::new(TerminalClient::new(
        connection_cx.clone(),
        session_id.to_string(),
    ));
    session
        .configure_acp_server(connection_cx.clone(), Some(terminal_client))
        .await;

    // Connect if not already connected
    if !session.is_connected() {
        session.connect().await?;
    }

    // Reset cancelled flag for new prompt
    session.reset_cancelled();

    // Extract text from prompt content blocks
    let query_text = extract_text_from_content(&request.prompt);

    tracing::info!("Received prompt for session {}: {}", session_id, query_text);

    // Get mutable client access and send the query
    {
        let mut client = session.client_mut().await;

        // Send the query
        if !query_text.is_empty() {
            client
                .query(&query_text)
                .await
                .map_err(AgentError::from)?;
        }
    }

    // Get read access to client for streaming responses
    let client = session.client().await;
    let mut stream = client.receive_response();
    let converter = session.converter();

    // Process streaming responses
    while let Some(result) = stream.next().await {
        // Check if cancelled
        if session.is_cancelled() {
            tracing::info!("Prompt cancelled for session {}", session_id);
            return Ok(PromptResponse::new(StopReason::EndTurn));
        }

        match result {
            Ok(message) => {
                // Convert SDK message to ACP notifications
                let notifications = converter.convert_message(&message, session_id);

                // Send each notification
                for notification in notifications {
                    if let Err(e) = send_notification(&connection_cx, notification) {
                        tracing::warn!("Failed to send notification: {}", e);
                    }
                }
            }
            Err(e) => {
                tracing::error!("Error receiving message: {}", e);
                // Continue processing - don't fail on individual message errors
            }
        }
    }

    // Build response - ACP PromptResponse just has stop_reason
    Ok(PromptResponse::new(StopReason::EndTurn))
}

/// Send a notification via the connection context
fn send_notification(
    cx: &JrConnectionCx,
    notification: SessionNotification,
) -> Result<(), sacp::Error> {
    cx.send_notification(notification)
}

/// Handle session/setMode request
///
/// Sets the permission mode for the session and sends a CurrentModeUpdate notification.
#[tracing::instrument(skip(request, sessions, connection_cx), fields(session_id = %request.session_id.0, mode_id = %request.mode_id.0))]
pub async fn handle_set_mode(
    request: SetSessionModeRequest,
    sessions: &Arc<SessionManager>,
    connection_cx: JrConnectionCx,
) -> Result<SetSessionModeResponse, AgentError> {
    let session_id_str = request.session_id.0.as_ref();
    let session = sessions.get_session_or_error(session_id_str)?;

    // Parse the mode from mode_id
    let mode_id_str = request.mode_id.0.as_ref();
    let mode = PermissionMode::parse(mode_id_str)
        .ok_or_else(|| AgentError::InvalidMode(mode_id_str.to_string()))?;

    // Set the mode
    session.set_permission_mode(mode).await;

    // Send CurrentModeUpdate notification to inform the client
    let mode_update = CurrentModeUpdate::new(SessionModeId::new(mode_id_str));
    let notification = SessionNotification::new(
        SessionId::new(session_id_str),
        SessionUpdate::CurrentModeUpdate(mode_update),
    );

    if let Err(e) = connection_cx.send_notification(notification) {
        tracing::warn!("Failed to send CurrentModeUpdate notification: {}", e);
    }

    tracing::info!(
        "Mode changed for session {}: {}",
        session_id_str,
        mode_id_str
    );

    Ok(SetSessionModeResponse::new())
}

/// Handle session/setModel request
///
/// Sets the model for the session. Note that changing the model mid-session
/// may require reconnecting the client.
///
/// Note: This handler is not yet active because sacp SDK does not implement
/// JrRequest for SetSessionModelRequest. When sacp adds support, this can be
/// registered in the handler chain.
#[allow(dead_code)]
#[tracing::instrument(skip(request, sessions), fields(session_id = %request.session_id.0, model_id = %request.model_id.0))]
pub async fn handle_set_model(
    request: SetSessionModelRequest,
    sessions: &Arc<SessionManager>,
) -> Result<SetSessionModelResponse, AgentError> {
    let session_id = request.session_id.0.as_ref();
    let session = sessions.get_session_or_error(session_id)?;

    let model_id = request.model_id.0.as_ref();

    // Store the model selection in the session
    session.set_model(model_id.to_string()).await;

    tracing::info!(
        "Model changed for session {}: {}",
        session_id,
        model_id
    );

    Ok(SetSessionModelResponse::new())
}

/// Handle session cancellation
///
/// Called when a cancel notification is received.
/// Sends an interrupt signal to Claude CLI to stop the current operation.
#[tracing::instrument(skip(sessions))]
pub async fn handle_cancel(session_id: &str, sessions: &Arc<SessionManager>) -> Result<(), AgentError> {
    let session = sessions.get_session_or_error(session_id)?;
    session.cancel().await;
    Ok(())
}

/// Extract text from ACP content blocks
fn extract_text_from_content(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| {
            if let ContentBlock::Text(TextContent { text, .. }) = block {
                Some(text.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sacp::schema::ProtocolVersion;
    use std::path::PathBuf;

    #[test]
    fn test_handle_initialize() {
        let request = InitializeRequest::new(ProtocolVersion::LATEST);
        let config = AgentConfig::from_env();

        let response = handle_initialize(request, &config);

        assert_eq!(response.protocol_version, ProtocolVersion::LATEST);
    }

    #[test]
    fn test_handle_new_session() {
        let request = NewSessionRequest::new(PathBuf::from("/tmp"));
        let config = AgentConfig::from_env();
        let sessions = Arc::new(SessionManager::new());

        let response = handle_new_session(request, &config, &sessions).unwrap();

        assert!(!response.session_id.0.is_empty());
        assert!(sessions.has_session(&response.session_id.0));
    }

    #[test]
    fn test_extract_text_from_content() {
        let blocks = vec![
            ContentBlock::Text(TextContent::new("Hello")),
            ContentBlock::Text(TextContent::new("World")),
        ];

        let text = extract_text_from_content(&blocks);
        assert_eq!(text, "Hello\nWorld");
    }
}
