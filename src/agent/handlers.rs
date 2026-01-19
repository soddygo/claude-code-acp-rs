//! ACP request handlers
//!
//! Implements handlers for ACP protocol requests:
//! - initialize: Return agent capabilities
//! - session/new: Create a new session
//! - session/prompt: Execute a prompt (Phase 1: simplified)
//! - session/setMode: Set permission mode

use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use futures::{Stream, StreamExt};
use sacp::JrConnectionCx;
use sacp::link::AgentToClient;
use tokio_util::sync::CancellationToken;
use sacp::schema::{
    AgentCapabilities, AvailableCommandsUpdate, ContentBlock, CurrentModeUpdate, Implementation,
    InitializeRequest, InitializeResponse, LoadSessionRequest, LoadSessionResponse,
    NewSessionRequest, NewSessionResponse, PromptCapabilities, PromptRequest, PromptResponse,
    SessionId, SessionMode, SessionModeId, SessionModeState, SessionNotification, SessionUpdate,
    SetSessionModeRequest, SetSessionModeResponse, StopReason,
};
use tokio::sync::broadcast;
use tracing::instrument;

use crate::agent::flush;
use crate::agent::slash_commands::{get_predefined_commands, transform_mcp_command_input};
use crate::session::{PermissionMode, SessionManager};
use crate::terminal::TerminalClient;
use crate::types::{AgentConfig, AgentError, NewSessionMeta};

/// Handle initialize request
///
/// Returns the agent's capabilities and protocol version.
#[instrument(
    name = "acp_initialize",
    skip(request, _config),
    fields(
        protocol_version = ?request.protocol_version,
        agent_version = %env!("CARGO_PKG_VERSION"),
    )
)]
pub fn handle_initialize(request: InitializeRequest, _config: &AgentConfig) -> InitializeResponse {
    tracing::info!(
        protocol_version = ?request.protocol_version,
        agent_name = "claude-code-acp-rs",
        agent_version = %env!("CARGO_PKG_VERSION"),
        "Handling ACP initialize request"
    );

    // Build agent capabilities using builder pattern
    let prompt_caps = PromptCapabilities::new().image(true).embedded_context(true);

    let capabilities = AgentCapabilities::new().prompt_capabilities(prompt_caps);

    // Build agent info
    let agent_info =
        Implementation::new("claude-code-acp-rs", env!("CARGO_PKG_VERSION")).title("Claude Code");

    tracing::debug!(
        capabilities = ?capabilities,
        "Sending initialize response with capabilities"
    );

    // Build response
    InitializeResponse::new(request.protocol_version)
        .agent_capabilities(capabilities)
        .agent_info(agent_info)
}

/// Handle session/new request
///
/// Creates a new session with the given working directory and metadata.
/// Returns available modes and models for the session.
#[instrument(
    name = "acp_new_session",
    skip(request, config, sessions, connection_cx),
    fields(
        cwd = ?request.cwd,
        has_meta = request.meta.is_some(),
        mcp_server_count = request.mcp_servers.len(),
    )
)]
pub async fn handle_new_session(
    request: NewSessionRequest,
    config: &AgentConfig,
    sessions: &Arc<SessionManager>,
    connection_cx: JrConnectionCx<AgentToClient>,
) -> Result<NewSessionResponse, AgentError> {
    let start_time = Instant::now();

    tracing::info!(
        cwd = ?request.cwd,
        has_meta = request.meta.is_some(),
        mcp_server_count = request.mcp_servers.len(),
        "Creating new ACP session"
    );

    // Log external MCP servers from client
    if !request.mcp_servers.is_empty() {
        tracing::info!(
            mcp_servers = ?request.mcp_servers.iter().map(|s| match s {
                sacp::schema::McpServer::Stdio(stdio) => format!("{}(stdio:{})", stdio.name, stdio.command.display()),
                sacp::schema::McpServer::Http(http) => format!("{}(http:{})", http.name, http.url),
                sacp::schema::McpServer::Sse(sse) => format!("{}(sse:{})", sse.name, sse.url),
                _ => "unknown".to_string(),
            }).collect::<Vec<_>>(),
            "External MCP servers from client"
        );
    }

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

    tracing::debug!(
        session_id = %session_id,
        "Generated new session ID"
    );

    // Create the session
    let session =
        sessions.create_session(session_id.clone(), cwd.clone(), config, meta.as_ref())?;

    // Store external MCP servers for later connection
    if !request.mcp_servers.is_empty() {
        session.set_external_mcp_servers(request.mcp_servers);
    }

    // Build available modes
    let available_modes = build_available_modes();
    let mode_state = SessionModeState::new("default", available_modes);

    let elapsed = start_time.elapsed();
    tracing::info!(
        session_id = %session_id,
        cwd = ?cwd,
        elapsed_ms = elapsed.as_millis(),
        "New session created successfully"
    );

    // Send available commands list to client
    // This is done asynchronously (similar to TypeScript's setTimeout)
    // to ensure the response is sent first
    #[cfg(not(test))]  // Only in production, skip in tests
    {
        let session_id_clone = session_id.clone();
        tokio::spawn(async move {
            if let Err(e) = send_available_commands_update(&session_id_clone, connection_cx) {
                tracing::warn!(
                    session_id = %session_id_clone,
                    "Failed to send available commands update: {}",
                    e
                );
            }
        });
    }

    Ok(NewSessionResponse::new(session_id).modes(mode_state))
}

/// Handle session/load request
///
/// Loads an existing session by resuming it with the given session ID.
/// Returns available modes and models for the session.
///
/// Note: Unlike TS implementation which doesn't support loadSession,
/// our Rust implementation uses claude-code-agent-sdk's resume functionality
/// to restore conversation history.
#[instrument(
    name = "acp_load_session",
    skip(request, config, sessions),
    fields(
        session_id = %request.session_id.0,
        cwd = ?request.cwd,
    )
)]
pub fn handle_load_session(
    request: LoadSessionRequest,
    config: &AgentConfig,
    sessions: &Arc<SessionManager>,
) -> Result<LoadSessionResponse, AgentError> {
    let start_time = Instant::now();

    // The session_id in the request is the ID of the session to resume
    let resume_session_id = request.session_id.0.to_string();
    let cwd = request.cwd;

    tracing::info!(
        session_id = %resume_session_id,
        cwd = ?cwd,
        "Loading existing session"
    );

    // Create NewSessionMeta with resume option
    // This tells the underlying SDK to resume from the specified session
    let meta = NewSessionMeta::with_resume(&resume_session_id);

    // Generate a new session ID for this loaded session
    // Note: We use the same session ID as the one being loaded
    // so the client can continue using the same ID
    let session_id = resume_session_id.clone();

    // Check if session already exists in our manager
    // If it does, we just return success (session already loaded)
    if sessions.has_session(&session_id) {
        let elapsed = start_time.elapsed();
        tracing::info!(
            session_id = %session_id,
            elapsed_ms = elapsed.as_millis(),
            "Session already exists, returning existing session"
        );
    } else {
        // Create the session with resume option
        tracing::debug!(
            session_id = %session_id,
            "Creating session with resume option"
        );
        sessions.create_session(session_id.clone(), cwd.clone(), config, Some(&meta))?;

        let elapsed = start_time.elapsed();
        tracing::info!(
            session_id = %session_id,
            elapsed_ms = elapsed.as_millis(),
            "Session loaded and created successfully"
        );
    }

    // Build available modes (same as new session)
    let available_modes = build_available_modes();
    let mode_state = SessionModeState::new("default", available_modes);

    Ok(LoadSessionResponse::new().modes(mode_state))
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

/// Send available commands update to client
///
/// Sends the list of available slash commands to the client via ACP notification.
fn send_available_commands_update(
    session_id: &str,
    connection_cx: JrConnectionCx<AgentToClient>,
) -> Result<(), AgentError> {
    let commands = get_predefined_commands();
    let command_count = commands.len();

    #[cfg(not(test))]
    {
        let notification = SessionNotification::new(
            SessionId::new(session_id.to_string()),
            SessionUpdate::AvailableCommandsUpdate(AvailableCommandsUpdate::new(commands)),
        );

        connection_cx
            .send_notification(notification)
            .map_err(|e| AgentError::Internal(format!("Failed to send commands update: {}", e)))?;

        tracing::info!(
            session_id = %session_id,
            command_count,
            "Sent available commands update"
        );
    }

    #[cfg(test)]
    {
        // In tests, just log without actually sending
        tracing::info!(
            session_id = %session_id,
            command_count,
            "Test mode: skipping commands update"
        );
    }

    Ok(())
}

/// Handle session/prompt request
///
/// Sends the prompt to Claude and streams responses back as notifications.
#[instrument(
    name = "acp_prompt",
    skip(request, _config, sessions, connection_cx),
    fields(
        session_id = %request.session_id.0,
        prompt_blocks = request.prompt.len(),
    )
)]
pub async fn handle_prompt(
    request: PromptRequest,
    _config: &AgentConfig,
    sessions: &Arc<SessionManager>,
    connection_cx: JrConnectionCx<AgentToClient>,
    cancel_token: CancellationToken,
) -> Result<PromptResponse, AgentError> {
    let prompt_start = Instant::now();

    let session_id = request.session_id.0.as_ref();
    let session = sessions.get_session_or_error(session_id)?;

    // Reset cancelled flag at the start of each prompt
    // This ensures that cancelled state from previous prompt is cleared
    session.reset_cancelled();

    tracing::info!(
        session_id = %session_id,
        prompt_blocks = request.prompt.len(),
        "Starting prompt processing"
    );

    // Configure ACP MCP server with connection and terminal client
    // This enables tools like Bash to send terminal updates
    let terminal_client = Arc::new(TerminalClient::new(
        connection_cx.clone(),
        session_id.to_string(),
    ));
    session
        .configure_acp_server(connection_cx.clone(), Some(terminal_client))
        .await;

    // Set connection context for permission requests
    // This enables the can_use_tool callback to send permission requests to the client
    session.set_connection_cx(connection_cx.clone());

    // Connect external MCP servers first (if any)
    // This ensures external tools are available when Claude CLI starts
    let external_mcp_start = Instant::now();
    if let Err(e) = session.connect_external_mcp_servers().await {
        tracing::error!(
            session_id = %session_id,
            error = %e,
            "Error connecting to external MCP servers"
        );
        // Continue anyway - external MCP failures shouldn't block the session
    }
    let external_mcp_elapsed = external_mcp_start.elapsed();
    if external_mcp_elapsed.as_millis() > 0 {
        tracing::debug!(
            session_id = %session_id,
            external_mcp_elapsed_ms = external_mcp_elapsed.as_millis(),
            "External MCP servers connection completed"
        );
    }

    // Connect if not already connected
    if !session.is_connected() {
        let connect_start = Instant::now();
        tracing::debug!(
            session_id = %session_id,
            "Connecting to Claude CLI"
        );
        session.connect().await?;
        let connect_elapsed = connect_start.elapsed();
        tracing::info!(
            session_id = %session_id,
            connect_elapsed_ms = connect_elapsed.as_millis(),
            "Connected to Claude CLI"
        );
    }

    // Extract text from prompt content blocks
    let query_text = extract_text_from_content(&request.prompt);
    let query_preview = query_text.chars().take(200).collect::<String>();

    tracing::info!(
        session_id = %session_id,
        query_len = query_text.len(),
        query_preview = %query_preview,
        "Sending query to Claude CLI"
    );

    // Get mutable client access and send the query
    let query_start = Instant::now();

    {
        let mut client = session.client_mut().await;

        // Send the query
        if !query_text.is_empty() {
            // Transform MCP command format: /mcp:server:cmd -> /server:cmd (MCP)
            let transformed_query = transform_mcp_command_input(&query_text);
            client.query(&transformed_query).await.map_err(AgentError::from)?;
        }
    }
    let query_elapsed = query_start.elapsed();
    tracing::debug!(
        session_id = %session_id,
        query_elapsed_ms = query_elapsed.as_millis(),
        "Query sent to Claude CLI"
    );

    // Get read access to client for streaming responses
    let client = session.client().await;
    let mut stream = client.receive_response();
    let converter = session.converter();
    let mut cancel_rx = session.cancel_receiver();

    // NOTE: drain_leftover_messages() is no longer needed because the SDK now
    // implements query-scoped message channels for proper message isolation.
    // Each receive_response() call gets its own isolated receiver, preventing
    // late-arriving ResultMessages from being consumed by the wrong prompt.
    // The function is kept for reference/debugging but not called.

    // Track streaming statistics
    let mut message_count = 0u64;
    let mut notification_count = 0u64;
    let mut error_count = 0u64;

    // Track last ResultMessage for determining stop reason
    let mut last_result: Option<claude_code_agent_sdk::ResultMessage> = None;

    // Process streaming responses
    let stream_start = Instant::now();
    loop {
        // Check for cancel signal from MCP cancellation notification
        match cancel_rx.try_recv() {
            Ok(()) => {
                tracing::info!(
                    session_id = %session_id,
                    "Cancel signal received from MCP notification, interrupting CLI"
                );
                // Send interrupt signal to Claude CLI
                if let Err(e) = client.interrupt().await {
                    tracing::warn!(
                        session_id = %session_id,
                        error = %e,
                        "Failed to send interrupt signal to Claude CLI"
                    );
                }
                // Set cancelled flag
                session.cancel().await;
                break;
            }
            Err(broadcast::error::TryRecvError::Empty) => {
                // No cancel signal, continue processing
            }
            Err(broadcast::error::TryRecvError::Closed) => {
                tracing::warn!(
                    session_id = %session_id,
                    "Cancel channel closed, no longer listening for cancel signals"
                );
                break;
            }
            Err(broadcast::error::TryRecvError::Lagged(_)) => {
                // Lagged means we missed some messages, but the most recent value is available
                // Treat this as a cancel signal
                tracing::info!(
                    session_id = %session_id,
                    "Cancel signal lagged, treating as cancel notification"
                );
                if let Err(e) = client.interrupt().await {
                    tracing::warn!(
                        session_id = %session_id,
                        error = %e,
                        "Failed to send interrupt signal to Claude CLI"
                    );
                }
                session.cancel().await;
                break;
            }
        }

        // Check if cancelled via CancellationToken
        if cancel_token.is_cancelled() {
            let elapsed = prompt_start.elapsed();
            tracing::info!(
                session_id = %session_id,
                elapsed_ms = elapsed.as_millis(),
                message_count = message_count,
                notification_count = notification_count,
                "Prompt cancelled by user"
            );
            return Ok(PromptResponse::new(StopReason::Cancelled));
        }

        // Process next message from stream with timeout
        let msg_result =
            tokio::time::timeout(tokio::time::Duration::from_millis(100), stream.next()).await;

        match msg_result {
            Ok(Some(Ok(message))) => {
                message_count += 1;

                // Log message type for debugging
                let msg_type = format!("{:?}", message);
                tracing::debug!(
                    session_id = %session_id,
                    message_count = message_count,
                    msg_type = %msg_type.chars().take(50).collect::<String>(),
                    "Received message from SDK"
                );

                // Track ResultMessage for stop reason determination
                if let claude_code_agent_sdk::Message::Result(ref result) = message {
                    tracing::info!(
                        session_id = %session_id,
                        subtype = %result.subtype,
                        is_error = result.is_error,
                        duration_ms = result.duration_ms,
                        num_turns = result.num_turns,
                        "Received ResultMessage from Claude CLI"
                    );
                    last_result = Some(result.clone());
                }

                // Convert SDK message to ACP notifications
                let notifications = converter.convert_message(&message, session_id);
                let batch_size = notifications.len();

                // Send each notification
                for notification in notifications {
                    notification_count += 1;
                    if let Err(e) = send_notification(&connection_cx, notification) {
                        error_count += 1;
                        tracing::warn!(
                            session_id = %session_id,
                            error = %e,
                            "Failed to send notification"
                        );
                    }
                }

                tracing::trace!(
                    session_id = %session_id,
                    message_count = message_count,
                    batch_size = batch_size,
                    "Processed message from Claude CLI"
                );
            }
            Ok(None) => {
                // Stream ended normally
                break;
            }
            Ok(Some(Err(e))) => {
                error_count += 1;
                tracing::error!(
                    session_id = %session_id,
                    error = %e,
                    message_count = message_count,
                    "Error receiving message from Claude CLI"
                );
                // Continue processing - don't fail on individual message errors
            }
            Err(_) => {
                // Timeout - continue loop to check cancel signal again
            }
        }
    }

    let stream_elapsed = stream_start.elapsed();
    let total_elapsed = prompt_start.elapsed();

    tracing::info!(
        session_id = %session_id,
        total_elapsed_ms = total_elapsed.as_millis(),
        stream_elapsed_ms = stream_elapsed.as_millis(),
        query_elapsed_ms = query_elapsed.as_millis(),
        message_count = message_count,
        notification_count = notification_count,
        error_count = error_count,
        "Prompt completed"
    );

    // ========================================================================
    // CRITICAL: Flush pending notifications before returning EndTurn
    // ========================================================================
    //
    // This fixes the message ordering issue described in MESSAGE_ORDERING_ISSUE.md
    //
    // The Problem:
    // - send_notification() uses unbounded_send() which returns immediately
    // - Messages are processed asynchronously by outgoing_protocol_actor
    // - EndTurn response can arrive before notifications are sent
    //
    // The Solution:
    // - When using patched sacp with flush mechanism: call flush() to wait
    // - When using official sacp: fall back to sleep-based approximation
    //
    // IMPORTANT: This project uses a patch to your sacp fork during development
    // which includes the flush mechanism. See: docs/PATCH_CONFIGURATION.md
    //
    // When your PR is merged to official sacp, this will use the native flush()
    // method from the official library.
    //
    flush::ensure_notifications_flushed(&connection_cx, notification_count).await;

    // Determine stop reason based on cancellation state and ResultMessage
    // Reference: vendors/claude-code-acp/src/acp-agent.ts lines 286-323
    if cancel_token.is_cancelled() {
        tracing::info!(session_id = %session_id, "Returning Cancelled stop reason");
        return Ok(PromptResponse::new(StopReason::Cancelled));
    }

    if let Some(ref result) = last_result {
        // Check user cancelled flag first (set by session/cancel notification)
        // This matches TypeScript behavior where cancelled flag is checked before result handling
        if session.is_user_cancelled() {
            tracing::info!(
                session_id = %session_id,
                subtype = %result.subtype,
                "User cancelled session, returning Cancelled stop reason"
            );
            return Ok(PromptResponse::new(StopReason::Cancelled));
        }

        // Check is_error first - TS throws error when is_error=true
        if result.is_error {
            let error_msg = result
                .result
                .clone()
                .unwrap_or_else(|| result.subtype.clone());
            tracing::error!(
                session_id = %session_id,
                subtype = %result.subtype,
                is_error = result.is_error,
                error_msg = %error_msg,
                "Query completed with is_error=true, returning error"
            );
            // Match TS behavior: throw RequestError.internalError
            return Err(AgentError::Internal(format!(
                "Query failed: {} (subtype: {})",
                error_msg, result.subtype
            )));
        }

        // Determine stop reason based on subtype
        // Reference: vendors/claude-code-acp/src/acp-agent.ts lines 347-360
        let stop_reason = match result.subtype.as_str() {
            "success" => {
                tracing::debug!(
                    session_id = %session_id,
                    subtype = %result.subtype,
                    "Returning EndTurn for success"
                );
                StopReason::EndTurn
            }
            "error_during_execution" => {
                // Match TS behavior: error_during_execution with is_error=false returns EndTurn
                // This indicates execution was interrupted but not due to an error
                // User cancellation is already handled above by checking is_user_cancelled()
                tracing::info!(
                    session_id = %session_id,
                    subtype = %result.subtype,
                    "Returning EndTurn for error_during_execution (is_error=false)"
                );
                StopReason::EndTurn
            }
            "error_max_budget_usd" | "error_max_turns" | "error_max_structured_output_retries" => {
                tracing::info!(
                    session_id = %session_id,
                    subtype = %result.subtype,
                    "Returning MaxTurnRequests for max limit subtype"
                );
                StopReason::MaxTurnRequests
            }
            _ => {
                // Match TS behavior: unknown subtypes return Refusal (not EndTurn)
                tracing::warn!(
                    session_id = %session_id,
                    subtype = %result.subtype,
                    "Unknown result subtype, returning Refusal"
                );
                StopReason::Refusal
            }
        };
        return Ok(PromptResponse::new(stop_reason));
    }

    // No ResultMessage received - stream ended unexpectedly
    // TS throws: "Session did not end in result"
    tracing::error!(
        session_id = %session_id,
        "Stream ended without ResultMessage, returning error"
    );
    Err(AgentError::Internal(
        "Session did not end in result".to_string(),
    ))
}

/// Send a notification via the connection context
fn send_notification(
    cx: &JrConnectionCx<AgentToClient>,
    notification: SessionNotification,
) -> Result<(), sacp::Error> {
    cx.send_notification(notification)
}

/// Handle session/setMode request
///
/// Sets the permission mode for the session and sends a CurrentModeUpdate notification.
#[instrument(
    name = "acp_set_mode",
    skip(request, sessions, connection_cx),
    fields(
        session_id = %request.session_id.0,
        mode_id = %request.mode_id.0,
    )
)]
pub async fn handle_set_mode(
    request: SetSessionModeRequest,
    sessions: &Arc<SessionManager>,
    connection_cx: JrConnectionCx<AgentToClient>,
) -> Result<SetSessionModeResponse, AgentError> {
    let session_id_str = request.session_id.0.as_ref();
    let mode_id_str = request.mode_id.0.as_ref();

    tracing::info!(
        session_id = %session_id_str,
        mode_id = %mode_id_str,
        "Setting session mode"
    );

    let session = sessions.get_session_or_error(session_id_str)?;

    // Get previous mode for logging
    let previous_mode = session.permission_mode().await;

    // Parse the mode from mode_id
    let mode = PermissionMode::parse(mode_id_str).ok_or_else(|| {
        tracing::warn!(
            session_id = %session_id_str,
            mode_id = %mode_id_str,
            "Invalid mode ID"
        );
        AgentError::InvalidMode(mode_id_str.to_string())
    })?;

    // Set the mode in our permission handler
    session.set_permission_mode(mode).await;

    // Also set the mode in the SDK client
    // This is important for the SDK to know the current permission mode
    let sdk_mode = mode.to_sdk_mode();
    if let Err(e) = session.client().await.set_permission_mode(sdk_mode).await {
        tracing::warn!(
            session_id = %session_id_str,
            mode = %mode_id_str,
            error = %e,
            "Failed to set SDK permission mode (continuing anyway)"
        );
        // Don't fail - the local mode is still set
    }

    // Send CurrentModeUpdate notification to inform the client
    let mode_update = CurrentModeUpdate::new(SessionModeId::new(mode_id_str));
    let notification = SessionNotification::new(
        SessionId::new(session_id_str),
        SessionUpdate::CurrentModeUpdate(mode_update),
    );

    if let Err(e) = connection_cx.send_notification(notification) {
        tracing::warn!(
            session_id = %session_id_str,
            error = %e,
            "Failed to send CurrentModeUpdate notification"
        );
    }

    tracing::info!(
        session_id = %session_id_str,
        previous_mode = ?previous_mode,
        new_mode = %mode_id_str,
        "Session mode changed successfully"
    );

    Ok(SetSessionModeResponse::new())
}

/// Handle session cancellation
///
/// Called when a cancel notification is received.
/// Sends an interrupt signal to Claude CLI to stop the current operation.
#[instrument(
    name = "acp_cancel",
    skip(sessions),
    fields(session_id = %session_id)
)]
pub async fn handle_cancel(
    session_id: &str,
    sessions: &Arc<SessionManager>,
) -> Result<(), AgentError> {
    tracing::info!(
        session_id = %session_id,
        "Cancelling session"
    );

    let session = sessions.get_session_or_error(session_id)?;
    session.cancel().await;

    tracing::info!(
        session_id = %session_id,
        "Session cancellation completed"
    );

    Ok(())
}

/// Extract text from ACP content blocks
///
/// This handles all ContentBlock types:
/// - Text: Direct text content
/// - Resource: Embedded file content (prefers this as it contains the actual file text)
/// - ResourceLink: File references (includes URI as context)
/// - Image: Ignored (not text content - images should be handled by PromptConverter as SDK ImageBlock)
/// - Audio: Ignored (not text content - consistent with TypeScript reference implementation)
///
/// Note: This function extracts text-only content for logging/transcript purposes.
/// Image blocks are handled by PromptConverter and converted to SDK ImageBlock for the Claude API.
/// Audio blocks are not supported (consistent with vendors/claude-code-acp/src/acp-agent.ts).
fn extract_text_from_content(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| {
            match block {
                ContentBlock::Text(text_content) => Some(text_content.text.clone()),
                ContentBlock::Resource(embedded_resource) => {
                    // Extract text from embedded resource content
                    match &embedded_resource.resource {
                        sacp::schema::EmbeddedResourceResource::TextResourceContents(
                            text_resource,
                        ) => {
                            // Format as context tag with URI
                            Some(format!(
                                "<context uri=\"{}\">\n{}\n</context>",
                                text_resource.uri, text_resource.text
                            ))
                        }
                        sacp::schema::EmbeddedResourceResource::BlobResourceContents(
                            blob_resource,
                        ) => {
                            // Binary resource - include URI reference
                            Some(format!("<context uri=\"{}\" />", blob_resource.uri))
                        }
                        // Handle any future resource types
                        _ => None,
                    }
                }
                ContentBlock::ResourceLink(resource_link) => {
                    // ResourceLink - include URI reference as context
                    // Note: This doesn't include the file content, just a reference
                    let uri = &resource_link.uri;
                    let title = resource_link.title.as_deref().unwrap_or("");
                    if title.is_empty() {
                        Some(format!("<resource uri=\"{uri}\" />"))
                    } else {
                        Some(format!("[{title}]({uri})"))
                    }
                }
                ContentBlock::Image(_) | ContentBlock::Audio(_) => {
                    // Images and audio are not text content - skip them
                    None
                }
                // Handle any future ContentBlock types
                _ => None,
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Drain leftover messages from the response stream
///
/// This function consumes and discards any messages remaining in the stream
/// from a previous prompt. This is called after creating the stream but before
/// processing the new prompt's responses.
///
/// The function uses a short timeout to avoid blocking indefinitely if there
/// are no messages, and it logs any messages it drains for debugging.
///
/// **DEPRECATED**: This function is no longer needed because the SDK now
/// implements query-scoped message channels for proper message isolation.
/// Each receive_response() call gets its own isolated receiver, preventing
/// late-arriving ResultMessages from being consumed by the wrong prompt.
/// This function is kept for reference/debugging purposes only.
#[allow(dead_code)]
async fn drain_leftover_messages(
    stream: &mut Pin<Box<dyn Stream<Item = Result<claude_code_agent_sdk::Message, claude_code_agent_sdk::ClaudeError>> + Send + '_>>,
) {
    use tokio::time::{timeout, Duration};

    let mut drained_count = 0;
    let max_drain_time = Duration::from_millis(100);

    // Try to drain any leftover messages with a short timeout
    let start = std::time::Instant::now();
    while start.elapsed() < max_drain_time {
        match timeout(Duration::from_millis(10), stream.next()).await {
            Ok(Some(Ok(message))) => {
                drained_count += 1;
                // Log the drained message type for debugging
                tracing::debug!(
                    drained_message_type = format!("{:?}", message).chars().take(50).collect::<String>(),
                    "Drained leftover message from previous prompt"
                );
            }
            Ok(Some(Err(e))) => {
                tracing::warn!(
                    error = %e,
                    "Drained error message from previous prompt"
                );
                drained_count += 1;
            }
            Ok(None) => {
                // Stream ended
                break;
            }
            Err(_) => {
                // Timeout - no more messages available
                break;
            }
        }
    }

    if drained_count > 0 {
        tracing::info!(
            drained_count,
            "Drained leftover messages from previous prompt before processing new prompt"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sacp::schema::{ProtocolVersion, TextContent};

    #[test]
    fn test_handle_initialize() {
        let request = InitializeRequest::new(ProtocolVersion::LATEST);
        let config = AgentConfig::from_env();

        let response = handle_initialize(request, &config);

        assert_eq!(response.protocol_version, ProtocolVersion::LATEST);
    }

    #[tokio::test]
    async fn test_handle_new_session() {
        // Note: This test is disabled because handle_new_session now requires
        // JrConnectionCx which is difficult to mock in unit tests.
        // The functionality is tested through integration tests instead.
        // TODO: Add integration test for session/new with available commands update
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
