//! ACP MCP Server implementation
//!
//! This module provides an MCP server that integrates with the ACP protocol,
//! allowing tools to send notifications during execution (e.g., terminal output).

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use claude_code_agent_sdk::{
    SdkMcpServer, SdkMcpTool, ToolDefinition, ToolHandler, ToolResult as SdkToolResult,
};
use futures::future::BoxFuture;
use futures::FutureExt;
use sacp::schema::{
    SessionId, SessionNotification, SessionUpdate, Terminal, ToolCallContent, ToolCallId,
    ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields,
};
use sacp::JrConnectionCx;
use serde_json::Value;
use tokio::sync::RwLock;

use super::registry::{ToolContext, ToolResult};
use super::server::McpServer;
use crate::session::BackgroundProcessManager;
use crate::terminal::TerminalClient;

/// ACP-integrated MCP server
///
/// This server implements the SDK's `SdkMcpServer` trait, allowing it to be
/// injected into the Claude Agent SDK. It provides access to:
/// - ACP connection for sending notifications
/// - Terminal API for command execution
/// - Tool registry for built-in tools
pub struct AcpMcpServer {
    /// Server name
    name: String,
    /// Server version
    version: String,
    /// Tool registry
    mcp_server: Arc<McpServer>,
    /// Tool definitions for SDK
    tools: HashMap<String, SdkMcpTool>,
    /// Session ID
    session_id: Arc<RwLock<Option<String>>>,
    /// ACP connection for sending notifications
    connection_cx: Arc<RwLock<Option<JrConnectionCx>>>,
    /// Terminal client
    terminal_client: Arc<RwLock<Option<Arc<TerminalClient>>>>,
    /// Background process manager
    background_processes: Arc<RwLock<Option<Arc<BackgroundProcessManager>>>>,
    /// Working directory
    cwd: Arc<RwLock<std::path::PathBuf>>,
    /// Cancel callback - called when MCP cancellation notification is received
    cancel_callback: Arc<RwLock<Option<Box<dyn Fn() + Send + Sync>>>>,
}

impl std::fmt::Debug for AcpMcpServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcpMcpServer")
            .field("name", &self.name)
            .field("version", &self.version)
            .finish()
    }
}

impl AcpMcpServer {
    /// Create a new ACP MCP server
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        let mcp_server = Arc::new(McpServer::new());
        let tools = Self::build_tool_definitions(&mcp_server);

        Self {
            name: name.into(),
            version: version.into(),
            mcp_server,
            tools,
            session_id: Arc::new(RwLock::new(None)),
            connection_cx: Arc::new(RwLock::new(None)),
            terminal_client: Arc::new(RwLock::new(None)),
            background_processes: Arc::new(RwLock::new(None)),
            cwd: Arc::new(RwLock::new(std::path::PathBuf::from("/tmp"))),
            cancel_callback: Arc::new(RwLock::new(None)),
        }
    }

    /// Set the session ID
    pub async fn set_session_id(&self, session_id: impl Into<String>) {
        let mut guard = self.session_id.write().await;
        *guard = Some(session_id.into());
    }

    /// Set the ACP connection
    pub async fn set_connection(&self, cx: JrConnectionCx) {
        let mut guard = self.connection_cx.write().await;
        *guard = Some(cx);
    }

    /// Set the terminal client
    pub async fn set_terminal_client(&self, client: Arc<TerminalClient>) {
        let mut guard = self.terminal_client.write().await;
        *guard = Some(client);
    }

    /// Set the background process manager
    pub async fn set_background_processes(&self, manager: Arc<BackgroundProcessManager>) {
        let mut guard = self.background_processes.write().await;
        *guard = Some(manager);
    }

    /// Set the working directory
    pub async fn set_cwd(&self, cwd: impl Into<std::path::PathBuf>) {
        let mut guard = self.cwd.write().await;
        *guard = cwd.into();
    }

    /// Set the cancel callback
    ///
    /// This callback is invoked when a MCP cancellation notification is received.
    /// Use this to interrupt the Claude CLI when the user cancels an operation.
    pub async fn set_cancel_callback<F>(&self, callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        let mut guard = self.cancel_callback.write().await;
        *guard = Some(Box::new(callback));
    }

    /// Get the MCP server
    pub fn mcp_server(&self) -> &Arc<McpServer> {
        &self.mcp_server
    }

    /// Build tool definitions from the MCP server
    ///
    /// Tools are registered with the `mcp__acp__` prefix so that CLI routes
    /// tool calls to our SDK MCP server via `mcp_message` requests.
    fn build_tool_definitions(mcp_server: &Arc<McpServer>) -> HashMap<String, SdkMcpTool> {
        let mut tools = HashMap::new();

        for schema in mcp_server.tool_schemas() {
            // Use original tool name without prefix
            // The MCP protocol will automatically add mcp__acp__ prefix
            let tool_name = schema.name.clone();
            let description = schema.description.clone();
            let input_schema = schema.input_schema.clone();

            // Create a placeholder handler - actual execution goes through handle_message
            let tool = SdkMcpTool {
                name: tool_name.clone(),
                description,
                input_schema,
                handler: Arc::new(PlaceholderHandler),
            };

            tools.insert(tool_name, tool);
        }

        tools
    }

    /// Send a terminal update notification
    async fn send_terminal_update(
        &self,
        tool_use_id: &str,
        terminal_id: &str,
        status: ToolCallStatus,
        title: Option<&str>,
    ) -> Result<(), String> {
        let connection_cx = self.connection_cx.read().await;
        let session_id = self.session_id.read().await;

        let Some(cx) = connection_cx.as_ref() else {
            return Err("No connection context available".to_string());
        };

        let Some(session_id) = session_id.as_ref() else {
            return Err("No session ID available".to_string());
        };

        // Build terminal content
        let terminal = Terminal::new(terminal_id.to_string());
        let content = vec![ToolCallContent::Terminal(terminal)];

        // Build update fields
        let mut update_fields = ToolCallUpdateFields::new()
            .status(status)
            .content(content);

        if let Some(title) = title {
            update_fields = update_fields.title(title);
        }

        // Build and send notification
        let tool_call_id = ToolCallId::new(tool_use_id.to_string());
        let update = ToolCallUpdate::new(tool_call_id, update_fields);
        let notification = SessionNotification::new(
            SessionId::new(session_id.as_str()),
            SessionUpdate::ToolCallUpdate(update),
        );

        cx.send_notification(notification)
            .map_err(|e| format!("Failed to send notification: {}", e))
    }

    /// Create a tool context for tool execution
    async fn create_tool_context(&self, tool_use_id: Option<&str>) -> ToolContext {
        let session_id = self.session_id.read().await;
        let cwd = self.cwd.read().await;
        let terminal_client = self.terminal_client.read().await;
        let background_processes = self.background_processes.read().await;
        let connection_cx = self.connection_cx.read().await;

        let mut context = ToolContext::new(
            session_id.as_deref().unwrap_or("unknown"),
            cwd.clone(),
        );

        if let Some(client) = terminal_client.as_ref() {
            context = context.with_terminal_client(client.clone());
        }

        if let Some(manager) = background_processes.as_ref() {
            context = context.with_background_processes(manager.clone());
        }

        if let Some(id) = tool_use_id {
            context = context.with_tool_use_id(id);
        }

        if let Some(cx) = connection_cx.as_ref() {
            context = context.with_connection_cx(cx.clone());
        }

        context
    }

    /// Execute a tool with ACP integration
    #[tracing::instrument(skip(self, arguments), fields(tool_use_id = ?tool_use_id))]
    async fn execute_tool(
        &self,
        tool_name: &str,
        arguments: Value,
        tool_use_id: Option<&str>,
    ) -> Result<ToolResult, String> {
        tracing::info!("Executing tool: {}", tool_name);
        let context = self.create_tool_context(tool_use_id).await;

        // Special handling for Bash tool - send terminal update after terminal creation
        if tool_name == "Bash" {
            return self
                .execute_bash_tool(arguments, tool_use_id, &context)
                .await;
        }

        // Execute other tools normally
        let result = self.mcp_server.execute(tool_name, arguments, &context).await;
        tracing::info!("Tool {} completed, is_error: {}", tool_name, result.is_error);
        Ok(result)
    }

    /// Execute Bash tool with Terminal API integration
    #[tracing::instrument(skip(self, arguments, context), fields(tool_use_id = ?tool_use_id))]
    async fn execute_bash_tool(
        &self,
        arguments: Value,
        tool_use_id: Option<&str>,
        context: &ToolContext,
    ) -> Result<ToolResult, String> {
        let terminal_client = self.terminal_client.read().await;
        let has_terminal_client = terminal_client.is_some();
        tracing::debug!("Terminal client available: {}", has_terminal_client);

        // If terminal client is available and we have a tool_use_id, use Terminal API
        if let (Some(client), Some(tool_use_id)) = (terminal_client.as_ref(), tool_use_id) {
            let command = arguments
                .get("command")
                .and_then(|v| v.as_str())
                .ok_or("Missing command")?;
            let description = arguments.get("description").and_then(|v| v.as_str());
            let run_in_background = arguments
                .get("run_in_background")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let timeout_ms = arguments
                .get("timeout")
                .and_then(|v| v.as_u64())
                .unwrap_or(120_000);

            tracing::info!(command = %command, "Creating terminal for Bash command");

            // Create terminal
            let terminal_id = client
                .create(
                    "bash",
                    vec!["-c".to_string(), command.to_string()],
                    Some(context.cwd.clone()),
                    Some(32_000),
                )
                .await
                .map_err(|e| format!("Failed to create terminal: {}", e))?;

            tracing::info!(terminal_id = %terminal_id.0, "Terminal created, sending update");

            // Send terminal update immediately
            if let Err(e) = self
                .send_terminal_update(
                    tool_use_id,
                    terminal_id.0.as_ref(),
                    ToolCallStatus::InProgress,
                    description,
                )
                .await
            {
                tracing::warn!("Failed to send terminal update: {}", e);
            }

            // Handle background vs foreground execution
            if run_in_background {
                tracing::info!("Running command in background");
                let shell_id = format!("term-{}", terminal_id.0.as_ref());
                return Ok(ToolResult::success(format!(
                    "Command started in background.\n\nShell ID: {}\n\nUse BashOutput to check status.",
                    shell_id
                )));
            }

            tracing::debug!("Waiting for command to complete (timeout: {}ms)", timeout_ms);

            // Wait for command to complete
            let timeout_duration = std::time::Duration::from_millis(timeout_ms);
            let exit_result = tokio::time::timeout(
                timeout_duration,
                client.wait_for_exit(terminal_id.clone()),
            )
            .await;

            // Get output
            let output = match client.output(terminal_id.clone()).await {
                Ok(resp) => resp.output,
                Err(e) => format!("(failed to get output: {})", e),
            };

            // Release terminal
            drop(client.release(terminal_id).await);

            // Process result
            match exit_result {
                Ok(Ok(exit_response)) => {
                    let exit_status = exit_response.exit_status;
                    #[allow(clippy::cast_possible_wrap)]
                    let exit_code = exit_status.exit_code.map(|c| c as i32).unwrap_or(-1);
                    tracing::info!(exit_code = exit_code, "Command completed");

                    if exit_code == 0 {
                        Ok(ToolResult::success(output))
                    } else {
                        Ok(ToolResult::error(format!(
                            "Command failed with exit code {}\n{}",
                            exit_code, output
                        )))
                    }
                }
                Ok(Err(e)) => {
                    tracing::error!("Terminal execution failed: {}", e);
                    Ok(ToolResult::error(format!("Terminal execution failed: {}", e)))
                }
                Err(_) => {
                    tracing::warn!("Command timed out after {}ms", timeout_ms);
                    Ok(ToolResult::error(format!(
                        "Command timed out after {}ms\n{}",
                        timeout_ms, output
                    )))
                }
            }
        } else {
            // Fall back to direct execution
            tracing::info!("Falling back to direct execution (no terminal client or tool_use_id)");
            let result = self
                .mcp_server
                .execute("Bash", arguments, context)
                .await;
            tracing::info!("Direct execution completed, is_error: {}", result.is_error);
            Ok(result)
        }
    }
}

/// Placeholder handler for SDK tool definitions
struct PlaceholderHandler;

impl ToolHandler for PlaceholderHandler {
    fn handle(&self, args: Value) -> BoxFuture<'static, claude_code_agent_sdk::errors::Result<SdkToolResult>> {
        tracing::warn!("PlaceholderHandler called with args: {:?}", args);
        async move {
            // This should never be called - execution goes through AcpMcpServer::handle_message
            tracing::error!("PlaceholderHandler was called! SDK is not using handle_message!");
            Ok(SdkToolResult {
                content: vec![claude_code_agent_sdk::McpToolResultContent::Text {
                    text: "Tool execution error: placeholder handler called".to_string(),
                }],
                is_error: true,
            })
        }
        .boxed()
    }
}

#[async_trait]
impl SdkMcpServer for AcpMcpServer {
    async fn handle_message(&self, message: Value) -> claude_code_agent_sdk::errors::Result<Value> {
        let method = message["method"]
            .as_str()
            .ok_or_else(|| claude_code_agent_sdk::errors::ClaudeError::Transport("Missing method".to_string()))?;

        tracing::debug!(method = %method, "AcpMcpServer handling message");

        match method {
            "initialize" => {
                tracing::info!("MCP server initializing");
                Ok(serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": self.name,
                        "version": self.version
                    }
                }))
            }
            "tools/list" => {
                tracing::info!("MCP server received tools/list request");
                let tools: Vec<_> = self
                    .tools
                    .values()
                    .map(|t| {
                        serde_json::json!({
                            "name": t.name,
                            "description": t.description,
                            "inputSchema": t.input_schema
                        })
                    })
                    .collect();

                tracing::info!("MCP server returning {} tools: {:?}",
                    tools.len(),
                    tools.iter().map(|t| t["name"].as_str().unwrap_or("")).collect::<Vec<_>>()
                );
                Ok(serde_json::json!({
                    "tools": tools
                }))
            }
            "tools/call" => {
                let params = &message["params"];
                let tool_name = params["name"].as_str().ok_or_else(|| {
                    claude_code_agent_sdk::errors::ClaudeError::Transport("Missing tool name".to_string())
                })?;
                let arguments = params["arguments"].clone();

                // Get tool_use_id from _meta if available
                let tool_use_id = params
                    .get("_meta")
                    .and_then(|m| m.get("claudecode/toolUseId"))
                    .and_then(|v| v.as_str());

                tracing::info!(
                    tool_name = %tool_name,
                    tool_use_id = ?tool_use_id,
                    has_meta = params.get("_meta").is_some(),
                    "Received tools/call request"
                );

                let result = self
                    .execute_tool(tool_name, arguments, tool_use_id)
                    .await
                    .map_err(|e| {
                        tracing::error!(error = %e, "Tool execution failed");
                        claude_code_agent_sdk::errors::ClaudeError::Transport(e)
                    })?;

                tracing::info!(
                    tool_name = %tool_name,
                    is_error = result.is_error,
                    content_len = result.content.len(),
                    "Tool execution completed"
                );

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": result.content
                    }],
                    "isError": result.is_error
                }))
            }
            // MCP notifications - these don't expect a response but we return empty success
            "notifications/cancelled" => {
                // Handle cancellation notification
                // The request_id is in params.requestId
                let request_id = message["params"]["requestId"].as_str();
                tracing::info!(request_id = ?request_id, "Received MCP cancellation notification");

                // Call the cancel callback to interrupt Claude CLI
                let callback = self.cancel_callback.read().await;
                if let Some(ref cb) = *callback {
                    tracing::info!("Invoking cancel callback to interrupt Claude CLI");
                    cb();
                } else {
                    tracing::warn!("No cancel callback registered, cancellation may not take effect");
                }

                Ok(serde_json::json!({}))
            }
            "notifications/initialized" => {
                tracing::info!("Received initialized notification");
                Ok(serde_json::json!({}))
            }
            "notifications/progress" => {
                tracing::debug!("Received progress notification");
                Ok(serde_json::json!({}))
            }
            _ => {
                // Check if it's a notification (starts with "notifications/")
                if method.starts_with("notifications/") {
                    tracing::debug!(method = %method, "Received unknown notification, ignoring");
                    Ok(serde_json::json!({}))
                } else {
                    Err(claude_code_agent_sdk::errors::ClaudeError::Transport(format!(
                        "Unknown method: {}",
                        method
                    )))
                }
            }
        }
    }

    fn list_tools(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|t| ToolDefinition {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.input_schema.clone(),
            })
            .collect()
    }
}

/// Get the list of built-in tools that should be disabled in the SDK
///
/// When using AcpMcpServer, these tools should be disabled in the SDK
/// so that our MCP server handles them instead.
pub fn get_disallowed_tools() -> Vec<String> {
    vec![
        "Bash".to_string(),
        "BashOutput".to_string(),
        "KillShell".to_string(),
        "Read".to_string(),
        "Write".to_string(),
        "Edit".to_string(),
        "Glob".to_string(),
        "Grep".to_string(),
        "LS".to_string(),
        "Task".to_string(),
        "TaskOutput".to_string(),
        "TodoWrite".to_string(),
        "ExitPlanMode".to_string(),
        "WebFetch".to_string(),
        "WebSearch".to_string(),
        "NotebookRead".to_string(),
        "NotebookEdit".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use claude_code_agent_sdk::ACP_TOOL_PREFIX;

    #[test]
    fn test_acp_mcp_server_creation() {
        let server = AcpMcpServer::new("test", "1.0.0");
        assert_eq!(server.name, "test");
        assert_eq!(server.version, "1.0.0");
    }

    #[test]
    fn test_tool_definitions_use_original_names() {
        let server = AcpMcpServer::new("acp", "1.0.0");
        // Tools should use original names WITHOUT prefix
        // MCP protocol automatically adds mcp__acp__ prefix when exposing to Claude
        assert!(server.tools.contains_key("Bash"));
        assert!(server.tools.contains_key("Read"));
        assert!(server.tools.contains_key("Write"));
        // Prefixed names should NOT be in the map
        assert!(!server.tools.contains_key("mcp__acp__Bash"));
        assert!(!server.tools.contains_key("mcp__acp__Read"));
    }

    #[test]
    fn test_get_disallowed_tools() {
        let tools = get_disallowed_tools();
        assert!(tools.contains(&"Bash".to_string()));
        assert!(tools.contains(&"Read".to_string()));
    }

    #[test]
    fn test_acp_tool_prefix_constant() {
        // Verify the SDK constant is correct
        assert_eq!(ACP_TOOL_PREFIX, "mcp__acp__");
    }
}
