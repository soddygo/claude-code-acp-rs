//! Session state and management
//!
//! Each session represents an active Claude conversation with its own
//! ClaudeClient instance, usage tracking, and permission state.

use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Instant;
use tokio::sync::broadcast;

use claude_code_agent_sdk::types::mcp::McpSdkServerConfig;
use claude_code_agent_sdk::{
    ClaudeAgentOptions, ClaudeClient, HookEvent, HookMatcher, McpServerConfig, McpServers,
    SystemPrompt, SystemPromptPreset,
};
use sacp::JrConnectionCx;
use sacp::link::AgentToClient;
use sacp::schema::McpServer;
use tokio::sync::RwLock;
use tracing::instrument;

use crate::converter::NotificationConverter;
use crate::hooks::{HookCallbackRegistry, create_post_tool_use_hook, create_pre_tool_use_hook};
use crate::mcp::AcpMcpServer;
use crate::settings::{PermissionChecker, SettingsManager};
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
    /// Current model ID for this session (set once during initialization)
    current_model: OnceLock<String>,
    /// ACP MCP server for tool execution with notifications
    acp_mcp_server: Arc<AcpMcpServer>,
    /// Background process manager
    background_processes: Arc<BackgroundProcessManager>,
    /// External MCP servers to connect (from client request)
    /// Set once during session initialization via set_external_mcp_servers()
    external_mcp_servers: OnceLock<Vec<McpServer>>,
    /// Whether external MCP servers have been connected
    external_mcp_connected: AtomicBool,
    /// Cancel signal sender - used to notify when MCP cancellation is received
    cancel_sender: broadcast::Sender<()>,
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
    #[instrument(
        name = "session_create",
        skip(config, meta),
        fields(
            session_id = %session_id,
            cwd = ?cwd,
            has_meta = meta.is_some(),
        )
    )]
    pub fn new(
        session_id: String,
        cwd: PathBuf,
        config: &AgentConfig,
        meta: Option<&NewSessionMeta>,
    ) -> Result<Self> {
        let start_time = Instant::now();

        tracing::info!(
            session_id = %session_id,
            cwd = ?cwd,
            "Creating new session"
        );

        // Create hook callback registry
        let hook_callback_registry = Arc::new(HookCallbackRegistry::new());

        // Create permission checker for hooks
        // Load settings from ~/.claude/settings.json, .claude/settings.json, etc.
        let settings_manager = SettingsManager::new(&cwd)
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to load settings manager from cwd: {}. Using default settings.", e);
                // Fallback: try to load settings from home directory
                match dirs::home_dir() {
                    Some(home) => {
                        tracing::info!("Attempting to load settings from home directory");
                        SettingsManager::new(&home).unwrap_or_else(|e2| {
                            tracing::error!("Failed to load settings from home directory: {}. Using minimal default settings.", e2);
                            // Last resort: create a manager with minimal settings
                            SettingsManager::new_with_settings(crate::settings::Settings::default(), "/")
                        })
                    }
                    None => {
                        tracing::error!("No home directory found. Using minimal default settings.");
                        SettingsManager::new_with_settings(crate::settings::Settings::default(), "/")
                    }
                }
            });
        let permission_checker = Arc::new(RwLock::new(PermissionChecker::new(
            settings_manager.settings().clone(),
            &cwd,
        )));

        // Create hooks
        let pre_tool_use_hook = create_pre_tool_use_hook(permission_checker.clone());
        let post_tool_use_hook = create_post_tool_use_hook(hook_callback_registry.clone());

        // Build hooks map
        let mut hooks_map: HashMap<HookEvent, Vec<HookMatcher>> = HashMap::new();
        hooks_map.insert(
            HookEvent::PreToolUse,
            vec![
                HookMatcher::builder()
                    .hooks(vec![pre_tool_use_hook])
                    .build(),
            ],
        );
        hooks_map.insert(
            HookEvent::PostToolUse,
            vec![
                HookMatcher::builder()
                    .hooks(vec![post_tool_use_hook])
                    .build(),
            ],
        );

        tracing::info!(
            session_id = %session_id,
            hooks_count = 2,
            "Hooks configured: PreToolUse, PostToolUse"
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
            session_id = %session_id,
            mcp_server_count = mcp_servers_dict.len(),
            "MCP servers configured"
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
                tracing::debug!(
                    session_id = %session_id,
                    servers = ?dict.keys().collect::<Vec<_>>(),
                    "MCP servers registered"
                );
            }
            McpServers::Empty => {
                tracing::warn!(
                    session_id = %session_id,
                    "MCP servers is Empty - this is unexpected!"
                );
            }
            McpServers::Path(p) => {
                tracing::warn!(
                    session_id = %session_id,
                    path = ?p,
                    "MCP servers is Path - this is unexpected!"
                );
            }
        }

        // Configure ACP tools to replace CLI built-in tools
        // This disables CLI's built-in tools and enables our MCP tools with mcp__acp__ prefix
        let acp_tools = get_acp_replacement_tools();
        options.use_acp_tools(&acp_tools);

        // Enable streaming to receive incremental content updates
        // This allows SDK to send StreamEvent messages with content_block_delta
        options.include_partial_messages = true;

        tracing::debug!(
            session_id = %session_id,
            acp_tools = ?acp_tools,
            disallowed_tools = ?options.disallowed_tools,
            allowed_tools = ?options.allowed_tools,
            "ACP tools configured"
        );

        // Apply config from environment
        config.apply_to_options(&mut options);

        tracing::debug!(
            session_id = %session_id,
            model = ?options.model,
            fallback_model = ?options.fallback_model,
            max_thinking_tokens = ?options.max_thinking_tokens,
            base_url = ?config.base_url,
            api_key = ?config.masked_api_key(),
            env_vars_count = options.env.len(),
            "Agent config applied"
        );

        // Apply meta options if provided
        if let Some(meta) = meta {
            // Set system prompt: replace takes priority over append
            if let Some(replace) = meta.get_system_prompt_replace() {
                // Complete replacement of system prompt
                options.system_prompt = Some(SystemPrompt::Text(replace.to_string()));
                tracing::info!(
                    session_id = %session_id,
                    prompt_len = replace.len(),
                    "Using custom system prompt from meta (replace)"
                );
            } else if let Some(append) = meta.get_system_prompt_append() {
                // Append to default claude_code preset
                let preset = SystemPromptPreset::with_append("claude_code", append);
                options.system_prompt = Some(SystemPrompt::Preset(preset));
                tracing::info!(
                    session_id = %session_id,
                    append_len = append.len(),
                    "Appending to system prompt from meta"
                );
            }

            // Set resume session if provided
            if let Some(resume_id) = meta.get_resume_session_id() {
                options.resume = Some(resume_id.to_string());
                tracing::info!(
                    session_id = %session_id,
                    resume_session_id = %resume_id,
                    "Resuming from previous session"
                );
            }

            // Set max thinking tokens if provided (enables extended thinking mode)
            if let Some(tokens) = meta.get_max_thinking_tokens() {
                options.max_thinking_tokens = Some(tokens);
                tracing::info!(
                    session_id = %session_id,
                    max_thinking_tokens = tokens,
                    "Extended thinking mode enabled via meta"
                );
            }
        }

        // Create the client
        let client = ClaudeClient::new(options);

        let elapsed = start_time.elapsed();
        tracing::info!(
            session_id = %session_id,
            elapsed_ms = elapsed.as_millis(),
            "Session created successfully"
        );

        // Clone cwd for converter before moving cwd into the struct
        let cwd_for_converter = cwd.clone();

        Ok(Self {
            session_id,
            cwd,
            client: RwLock::new(client),
            cancelled: Arc::new(AtomicBool::new(false)),
            permission: RwLock::new(PermissionHandler::new()),
            usage_tracker: UsageTracker::new(),
            converter: NotificationConverter::with_cwd(cwd_for_converter),
            connected: AtomicBool::new(false),
            hook_callback_registry,
            permission_checker,
            current_model: OnceLock::new(),
            acp_mcp_server,
            background_processes,
            external_mcp_servers: OnceLock::new(),
            external_mcp_connected: AtomicBool::new(false),
            cancel_sender: broadcast::channel(1).0,
        })
    }

    /// Set external MCP servers to connect
    ///
    /// # Arguments
    ///
    /// * `servers` - List of MCP servers from the client request
    pub fn set_external_mcp_servers(&self, servers: Vec<McpServer>) {
        if !servers.is_empty() {
            tracing::info!(
                session_id = %self.session_id,
                server_count = servers.len(),
                "Storing external MCP servers for later connection"
            );

            for server in &servers {
                match server {
                    McpServer::Stdio(s) => {
                        tracing::debug!(
                            session_id = %self.session_id,
                            server_name = %s.name,
                            command = ?s.command,
                            args = ?s.args,
                            "External MCP server (stdio)"
                        );
                    }
                    McpServer::Http(s) => {
                        tracing::debug!(
                            session_id = %self.session_id,
                            server_name = %s.name,
                            url = %s.url,
                            "External MCP server (http)"
                        );
                    }
                    McpServer::Sse(s) => {
                        tracing::debug!(
                            session_id = %self.session_id,
                            server_name = %s.name,
                            url = %s.url,
                            "External MCP server (sse)"
                        );
                    }
                    _ => {
                        tracing::debug!(
                            session_id = %self.session_id,
                            "External MCP server (unknown type)"
                        );
                    }
                }
            }
        }

        // Set the servers (can only be set once)
        drop(self.external_mcp_servers.set(servers));
    }

    /// Connect to external MCP servers
    ///
    /// This should be called before the first prompt to ensure all
    /// external MCP tools are available.
    #[instrument(
        name = "connect_external_mcp_servers",
        skip(self),
        fields(session_id = %self.session_id)
    )]
    pub async fn connect_external_mcp_servers(&self) -> Result<()> {
        // Only connect once
        if self.external_mcp_connected.load(Ordering::SeqCst) {
            tracing::debug!(
                session_id = %self.session_id,
                "External MCP servers already connected"
            );
            return Ok(());
        }

        // Get servers (no lock needed with OnceLock)
        let servers = match self.external_mcp_servers.get() {
            Some(s) => s,
            None => {
                tracing::debug!(
                    session_id = %self.session_id,
                    "No external MCP servers to connect"
                );
                self.external_mcp_connected.store(true, Ordering::SeqCst);
                return Ok(());
            }
        };

        // Clone server list to avoid holding reference
        let servers_vec: Vec<_> = servers.iter().cloned().collect();

        let server_count = servers_vec.len();
        let start_time = Instant::now();

        tracing::info!(
            session_id = %self.session_id,
            server_count = server_count,
            "Connecting to external MCP servers"
        );

        let external_manager = self.acp_mcp_server.mcp_server().external_manager();

        let mut success_count = 0;
        let mut error_count = 0;

        for server in servers_vec.iter() {
            match server {
                McpServer::Stdio(s) => {
                    let server_start = Instant::now();

                    tracing::info!(
                        session_id = %self.session_id,
                        server_name = %s.name,
                        command = ?s.command,
                        args = ?s.args,
                        "Connecting to external MCP server (stdio)"
                    );

                    // Convert env variables
                    let env: Option<HashMap<String, String>> = if s.env.is_empty() {
                        None
                    } else {
                        Some(
                            s.env
                                .iter()
                                .map(|e| (e.name.clone(), e.value.clone()))
                                .collect(),
                        )
                    };

                    match external_manager
                        .connect(
                            s.name.clone(),
                            s.command.to_string_lossy().as_ref(),
                            &s.args,
                            env.as_ref(),
                            Some(self.cwd.as_path()),
                        )
                        .await
                    {
                        Ok(_) => {
                            success_count += 1;
                            let elapsed = server_start.elapsed();
                            tracing::info!(
                                session_id = %self.session_id,
                                server_name = %s.name,
                                elapsed_ms = elapsed.as_millis(),
                                "Successfully connected to external MCP server"
                            );
                        }
                        Err(e) => {
                            error_count += 1;
                            let elapsed = server_start.elapsed();
                            tracing::error!(
                                session_id = %self.session_id,
                                server_name = %s.name,
                                error = %e,
                                elapsed_ms = elapsed.as_millis(),
                                "Failed to connect to external MCP server"
                            );
                        }
                    }
                }
                McpServer::Http(s) => {
                    tracing::warn!(
                        session_id = %self.session_id,
                        server_name = %s.name,
                        url = %s.url,
                        "HTTP MCP servers not yet supported"
                    );
                }
                McpServer::Sse(s) => {
                    tracing::warn!(
                        session_id = %self.session_id,
                        server_name = %s.name,
                        url = %s.url,
                        "SSE MCP servers not yet supported"
                    );
                }
                _ => {
                    tracing::warn!(
                        session_id = %self.session_id,
                        "Unknown MCP server type - not supported"
                    );
                }
            }
        }

        let total_elapsed = start_time.elapsed();
        tracing::info!(
            session_id = %self.session_id,
            total_servers = server_count,
            success_count = success_count,
            error_count = error_count,
            total_elapsed_ms = total_elapsed.as_millis(),
            "Finished connecting external MCP servers"
        );

        self.external_mcp_connected.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Connect to Claude CLI
    ///
    /// This spawns the Claude CLI process and establishes JSON-RPC communication.
    #[instrument(
        name = "session_connect",
        skip(self),
        fields(session_id = %self.session_id)
    )]
    pub async fn connect(&self) -> Result<()> {
        if self.connected.load(Ordering::SeqCst) {
            tracing::debug!(
                session_id = %self.session_id,
                "Already connected to Claude CLI"
            );
            return Ok(());
        }

        let start_time = Instant::now();
        tracing::info!(
            session_id = %self.session_id,
            cwd = ?self.cwd,
            "Connecting to Claude CLI..."
        );

        let mut client = self.client.write().await;
        client.connect().await.map_err(|e| {
            let agent_error = AgentError::from(e);
            tracing::error!(
                session_id = %self.session_id,
                error = %agent_error,
                error_code = ?agent_error.error_code(),
                is_retryable = %agent_error.is_retryable(),
                error_chain = ?agent_error.source(),
                "Failed to connect to Claude CLI"
            );
            agent_error
        })?;

        self.connected.store(true, Ordering::SeqCst);

        let elapsed = start_time.elapsed();
        tracing::info!(
            session_id = %self.session_id,
            elapsed_ms = elapsed.as_millis(),
            "Successfully connected to Claude CLI"
        );

        Ok(())
    }

    /// Disconnect from Claude CLI
    #[instrument(
        name = "session_disconnect",
        skip(self),
        fields(session_id = %self.session_id)
    )]
    pub async fn disconnect(&self) -> Result<()> {
        if !self.connected.load(Ordering::SeqCst) {
            tracing::debug!(
                session_id = %self.session_id,
                "Already disconnected from Claude CLI"
            );
            return Ok(());
        }

        let start_time = Instant::now();
        tracing::info!(
            session_id = %self.session_id,
            "Disconnecting from Claude CLI..."
        );

        let mut client = self.client.write().await;
        client.disconnect().await.map_err(|e| {
            let agent_error = AgentError::from(e);
            tracing::error!(
                session_id = %self.session_id,
                error = %agent_error,
                error_code = ?agent_error.error_code(),
                is_retryable = %agent_error.is_retryable(),
                error_chain = ?agent_error.source(),
                "Failed to disconnect from Claude CLI"
            );
            agent_error
        })?;

        self.connected.store(false, Ordering::SeqCst);

        let elapsed = start_time.elapsed();
        tracing::info!(
            session_id = %self.session_id,
            elapsed_ms = elapsed.as_millis(),
            "Disconnected from Claude CLI"
        );

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

    /// Get a receiver for cancel signals
    ///
    /// This can be used to listen for MCP cancellation notifications.
    /// When a cancel notification is received, a signal is sent through the channel.
    pub fn cancel_receiver(&self) -> broadcast::Receiver<()> {
        self.cancel_sender.subscribe()
    }

    /// Check if the session has been cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Cancel this session and interrupt the Claude CLI
    #[instrument(
        name = "session_cancel",
        skip(self),
        fields(session_id = %self.session_id)
    )]
    pub async fn cancel(&self) {
        tracing::info!(
            session_id = %self.session_id,
            "Cancelling session and sending interrupt signal"
        );

        self.cancelled.store(true, Ordering::SeqCst);

        // Send interrupt signal to Claude CLI to stop current operation
        if let Ok(client) = self.client.try_read() {
            if let Err(e) = client.interrupt().await {
                tracing::warn!(
                    session_id = %self.session_id,
                    error = %e,
                    "Failed to send interrupt signal to Claude CLI"
                );
            } else {
                tracing::info!(
                    session_id = %self.session_id,
                    "Interrupt signal sent to Claude CLI"
                );
            }
        } else {
            tracing::warn!(
                session_id = %self.session_id,
                "Could not acquire client lock for interrupt"
            );
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
    pub fn current_model(&self) -> Option<String> {
        self.current_model.get().cloned()
    }

    /// Set the model for this session
    ///
    /// Note: Not yet used because sacp SDK does not support SetSessionModel.
    #[allow(dead_code)]
    pub fn set_model(&self, model_id: String) {
        // Ignore error if model was already set (should not happen in normal use)
        drop(self.current_model.set(model_id));
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
        connection_cx: JrConnectionCx<AgentToClient>,
        terminal_client: Option<Arc<TerminalClient>>,
    ) {
        self.acp_mcp_server.set_session_id(&self.session_id);
        self.acp_mcp_server.set_connection(connection_cx);
        self.acp_mcp_server.set_cwd(self.cwd.clone()).await;
        self.acp_mcp_server
            .set_background_processes(self.background_processes.clone());

        if let Some(client) = terminal_client {
            self.acp_mcp_server.set_terminal_client(client);
        }

        // Set up cancel callback to interrupt Claude CLI when MCP cancellation is received
        let cancelled_flag = self.cancelled.clone();
        let session_id = self.session_id.clone();
        let cancel_sender = self.cancel_sender.clone();

        self.acp_mcp_server
            .set_cancel_callback(move || {
                tracing::info!(
                    session_id = %session_id,
                    "MCP cancel callback invoked, sending cancel signal"
                );
                cancelled_flag.store(true, Ordering::SeqCst);
                // Send cancel signal through the channel
                let _ = cancel_sender.send(());
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
            api_key: None,
            model: None,
            small_fast_model: None,
            max_thinking_tokens: None,
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
        session
            .set_permission_mode(PermissionMode::AcceptEdits)
            .await;
        assert_eq!(session.permission_mode().await, PermissionMode::AcceptEdits);
    }
}
