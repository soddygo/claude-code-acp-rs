//! Session state and management
//!
//! Each session represents an active Claude conversation with its own
//! ClaudeClient instance, usage tracking, and permission state.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use claude_code_agent_sdk::{
    ClaudeAgentOptions, ClaudeClient, HookEvent, HookMatcher, McpServerConfig, McpServers,
    SystemPrompt, SystemPromptPreset,
};
use claude_code_agent_sdk::types::mcp::McpSdkServerConfig;
use sacp::JrConnectionCx;
use tokio::sync::RwLock;

use crate::converter::NotificationConverter;
use crate::hooks::{
    create_post_tool_use_hook, create_pre_tool_use_hook, HookCallbackRegistry,
};
use crate::mcp::AcpMcpServer;
use crate::settings::PermissionChecker;
use crate::terminal::TerminalClient;
use crate::types::{AgentConfig, AgentError, NewSessionMeta, Result};

use super::BackgroundProcessManager;
use super::permission::{PermissionHandler, PermissionMode};
use super::usage::UsageTracker;

/// Get the list of tools that should be replaced by ACP MCP server tools.
///
/// Only tools that interact with the terminal or filesystem should be replaced:
/// - Terminal tools: Bash, BashOutput, KillShell
/// - File tools: Read, Write, Edit
///
/// Other tools like Glob, Grep, Task, etc. should remain as CLI built-in tools.
fn get_acp_replacement_tools() -> Vec<&'static str> {
    vec![
        // Terminal tools - must be replaced to use ACP Terminal API
        "Bash",
        "BashOutput",
        "KillShell",
        // File tools - must be replaced for ACP file synchronization
        "Read",
        "Write",
        "Edit",
    ]
}

/// An active Claude session
///
/// Each session holds its own ClaudeClient instance and maintains
/// independent state for usage tracking, permissions, and message conversion.
pub struct Session {
    /// Unique session identifier
    pub session_id: String,
    /// Working directory for this session
    pub cwd: PathBuf,
    /// The Claude client for this session
    client: RwLock<ClaudeClient>,
    /// Whether this session has been cancelled
    cancelled: Arc<AtomicBool>,
    /// Permission handler for tool execution
    permission: RwLock<PermissionHandler>,
    /// Token usage tracker
    usage_tracker: UsageTracker,
    /// Notification converter with tool use cache
    converter: NotificationConverter,
    /// Whether the client is connected
    connected: AtomicBool,
    /// Hook callback registry for PostToolUse callbacks
    hook_callback_registry: Arc<HookCallbackRegistry>,
    /// Permission checker for hooks
    permission_checker: Arc<RwLock<PermissionChecker>>,
    /// Current model ID for this session
    current_model: RwLock<Option<String>>,
    /// ACP MCP server for tool execution with notifications
    acp_mcp_server: Arc<AcpMcpServer>,
    /// Background process manager
    background_processes: Arc<BackgroundProcessManager>,
}

impl Session {
    /// Create a new session
    ///
    /// # Arguments
    ///
    /// * `session_id` - Unique identifier for this session
    /// * `cwd` - Working directory
    /// * `config` - Agent configuration from environment
    /// * `meta` - Session metadata from the new session request
    pub fn new(
        session_id: String,
        cwd: PathBuf,
        config: &AgentConfig,
        meta: Option<&NewSessionMeta>,
    ) -> Result<Self> {
        // Create hook callback registry
        let hook_callback_registry = Arc::new(HookCallbackRegistry::new());

        // Create permission checker for hooks
        let settings = crate::settings::Settings::default();
        let permission_checker = Arc::new(RwLock::new(PermissionChecker::new(settings, &cwd)));

        // Create hooks
        let pre_tool_use_hook = create_pre_tool_use_hook(permission_checker.clone());
        let post_tool_use_hook = create_post_tool_use_hook(hook_callback_registry.clone());

        // Build hooks map
        let mut hooks_map: HashMap<HookEvent, Vec<HookMatcher>> = HashMap::new();
        hooks_map.insert(
            HookEvent::PreToolUse,
            vec![HookMatcher::builder().hooks(vec![pre_tool_use_hook]).build()],
        );
        hooks_map.insert(
            HookEvent::PostToolUse,
            vec![HookMatcher::builder().hooks(vec![post_tool_use_hook]).build()],
        );

        // Create ACP MCP server
        let acp_mcp_server = Arc::new(AcpMcpServer::new("acp", env!("CARGO_PKG_VERSION")));

        // Create background process manager
        let background_processes = Arc::new(BackgroundProcessManager::new());

        // Build MCP servers with our ACP server
        let mut mcp_servers_dict = HashMap::new();
        mcp_servers_dict.insert(
            "acp".to_string(),
            McpServerConfig::Sdk(McpSdkServerConfig {
                name: "acp".to_string(),
                instance: acp_mcp_server.clone(),
            }),
        );

        tracing::info!(
            "Registering ACP MCP server with {} entries",
            mcp_servers_dict.len()
        );

        // Build ClaudeAgentOptions
        let mut options = ClaudeAgentOptions::builder()
            .cwd(cwd.clone())
            .hooks(hooks_map)
            .mcp_servers(McpServers::Dict(mcp_servers_dict))
            .build();

        // Verify mcp_servers is set correctly
        match &options.mcp_servers {
            McpServers::Dict(dict) => {
                tracing::info!("MCP servers configured: {:?}", dict.keys().collect::<Vec<_>>());
            }
            McpServers::Empty => {
                tracing::warn!("MCP servers is Empty - this is unexpected!");
            }
            McpServers::Path(p) => {
                tracing::warn!("MCP servers is Path({:?}) - this is unexpected!", p);
            }
        }

        // Configure ACP tools to replace CLI built-in tools
        // This disables CLI's built-in tools and enables our MCP tools with mcp__acp__ prefix
        options.use_acp_tools(&get_acp_replacement_tools());

        tracing::info!(
            "Configured ACP tools - disallowed: {:?}, allowed: {:?}",
            options.disallowed_tools,
            options.allowed_tools
        );

        // Apply config from environment
        config.apply_to_options(&mut options);

        // Apply meta options if provided
        if let Some(meta) = meta {
            // Set system prompt append if provided
            if let Some(append) = meta.get_system_prompt_append() {
                let preset = SystemPromptPreset::with_append("claude_code", append);
                options.system_prompt = Some(SystemPrompt::Preset(preset));
            }

            // Set resume session if provided
            if let Some(resume_id) = meta.get_resume_session_id() {
                options.resume = Some(resume_id.to_string());
            }
        }

        // Create the client
        let client = ClaudeClient::new(options);

        Ok(Self {
            session_id,
            cwd,
            client: RwLock::new(client),
            cancelled: Arc::new(AtomicBool::new(false)),
            permission: RwLock::new(PermissionHandler::new()),
            usage_tracker: UsageTracker::new(),
            converter: NotificationConverter::new(),
            connected: AtomicBool::new(false),
            hook_callback_registry,
            permission_checker,
            current_model: RwLock::new(None),
            acp_mcp_server,
            background_processes,
        })
    }

    /// Connect to Claude
    pub async fn connect(&self) -> Result<()> {
        if self.connected.load(Ordering::SeqCst) {
            return Ok(());
        }

        let mut client = self.client.write().await;
        client.connect().await.map_err(AgentError::from)?;
        self.connected.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Disconnect from Claude
    pub async fn disconnect(&self) -> Result<()> {
        if !self.connected.load(Ordering::SeqCst) {
            return Ok(());
        }

        let mut client = self.client.write().await;
        client.disconnect().await.map_err(AgentError::from)?;
        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Check if the session is connected
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// Get read access to the client
    pub async fn client(&self) -> tokio::sync::RwLockReadGuard<'_, ClaudeClient> {
        self.client.read().await
    }

    /// Get write access to the client
    pub async fn client_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, ClaudeClient> {
        self.client.write().await
    }

    /// Check if the session has been cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Cancel this session and interrupt the Claude CLI
    pub async fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);

        // Send interrupt signal to Claude CLI to stop current operation
        if let Ok(client) = self.client.try_read() {
            if let Err(e) = client.interrupt().await {
                tracing::warn!("Failed to send interrupt signal: {}", e);
            } else {
                tracing::info!("Sent interrupt signal to Claude CLI");
            }
        } else {
            tracing::warn!("Could not acquire client lock for interrupt");
        }
    }

    /// Reset the cancelled flag
    pub fn reset_cancelled(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
    }

    /// Get the permission handler
    pub async fn permission(&self) -> tokio::sync::RwLockReadGuard<'_, PermissionHandler> {
        self.permission.read().await
    }

    /// Get the current permission mode
    pub async fn permission_mode(&self) -> PermissionMode {
        self.permission.read().await.mode()
    }

    /// Set the permission mode
    pub async fn set_permission_mode(&self, mode: PermissionMode) {
        self.permission.write().await.set_mode(mode);
    }

    /// Get the current model ID
    ///
    /// Note: Not yet used because sacp SDK does not support SetSessionModel.
    #[allow(dead_code)]
    pub async fn current_model(&self) -> Option<String> {
        self.current_model.read().await.clone()
    }

    /// Set the model for this session
    ///
    /// Note: Not yet used because sacp SDK does not support SetSessionModel.
    #[allow(dead_code)]
    pub async fn set_model(&self, model_id: String) {
        *self.current_model.write().await = Some(model_id);
    }

    /// Get the usage tracker
    pub fn usage_tracker(&self) -> &UsageTracker {
        &self.usage_tracker
    }

    /// Get the notification converter
    pub fn converter(&self) -> &NotificationConverter {
        &self.converter
    }

    /// Get the hook callback registry
    pub fn hook_callback_registry(&self) -> &Arc<HookCallbackRegistry> {
        &self.hook_callback_registry
    }

    /// Get the permission checker
    pub fn permission_checker(&self) -> &Arc<RwLock<PermissionChecker>> {
        &self.permission_checker
    }

    /// Register a PostToolUse callback for a tool use
    pub fn register_post_tool_use_callback(
        &self,
        tool_use_id: String,
        callback: crate::hooks::PostToolUseCallback,
    ) {
        self.hook_callback_registry
            .register_post_tool_use(tool_use_id, callback);
    }

    /// Get the ACP MCP server
    pub fn acp_mcp_server(&self) -> &Arc<AcpMcpServer> {
        &self.acp_mcp_server
    }

    /// Get the background process manager
    pub fn background_processes(&self) -> &Arc<BackgroundProcessManager> {
        &self.background_processes
    }

    /// Configure the ACP MCP server with connection and terminal client
    ///
    /// This should be called after creating the session to enable Terminal API
    /// integration for Bash commands.
    pub async fn configure_acp_server(
        &self,
        connection_cx: JrConnectionCx,
        terminal_client: Option<Arc<TerminalClient>>,
    ) {
        self.acp_mcp_server.set_session_id(&self.session_id).await;
        self.acp_mcp_server.set_connection(connection_cx).await;
        self.acp_mcp_server.set_cwd(self.cwd.clone()).await;
        self.acp_mcp_server
            .set_background_processes(self.background_processes.clone())
            .await;

        if let Some(client) = terminal_client {
            self.acp_mcp_server.set_terminal_client(client).await;
        }

        // Set up cancel callback to interrupt Claude CLI when MCP cancellation is received
        let cancelled_flag = self.cancelled.clone();
        let session_id = self.session_id.clone();

        self.acp_mcp_server
            .set_cancel_callback(move || {
                tracing::info!(
                    session_id = %session_id,
                    "MCP cancel callback invoked, setting session cancelled flag"
                );
                cancelled_flag.store(true, Ordering::SeqCst);
            })
            .await;
    }
}

#[allow(clippy::missing_fields_in_debug)]
impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("session_id", &self.session_id)
            .field("cwd", &self.cwd)
            .field("cancelled", &self.cancelled.load(Ordering::Relaxed))
            .field("connected", &self.connected.load(Ordering::Relaxed))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AgentConfig {
        AgentConfig {
            base_url: None,
            auth_token: None,
            model: None,
            small_fast_model: None,
        }
    }

    #[test]
    fn test_session_new() {
        let session = Session::new(
            "test-session-1".to_string(),
            PathBuf::from("/tmp"),
            &test_config(),
            None,
        )
        .unwrap();

        assert_eq!(session.session_id, "test-session-1");
        assert_eq!(session.cwd, PathBuf::from("/tmp"));
        assert!(!session.is_cancelled());
        assert!(!session.is_connected());
    }

    #[tokio::test]
    async fn test_session_cancel() {
        let session = Session::new(
            "test-session-2".to_string(),
            PathBuf::from("/tmp"),
            &test_config(),
            None,
        )
        .unwrap();

        assert!(!session.is_cancelled());
        session.cancel().await;
        assert!(session.is_cancelled());
        session.reset_cancelled();
        assert!(!session.is_cancelled());
    }

    #[tokio::test]
    async fn test_session_permission_mode() {
        let session = Session::new(
            "test-session-3".to_string(),
            PathBuf::from("/tmp"),
            &test_config(),
            None,
        )
        .unwrap();

        assert_eq!(session.permission_mode().await, PermissionMode::Default);
        session.set_permission_mode(PermissionMode::AcceptEdits).await;
        assert_eq!(session.permission_mode().await, PermissionMode::AcceptEdits);
    }
}
