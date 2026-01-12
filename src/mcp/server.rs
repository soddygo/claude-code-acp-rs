//! MCP Server implementation
//!
//! Manages tool registration and provides the server interface.
//! Supports both built-in tools and external MCP servers.

use std::path::Path;
use std::sync::Arc;

use crate::mcp::external::{ExternalMcpError, ExternalMcpManager};
use crate::mcp::registry::{ToolContext, ToolRegistry, ToolResult, ToolSchema};
use crate::mcp::tools::{
    AskUserQuestionTool, BashOutputTool, BashTool, EditTool, ExitPlanModeTool, GlobTool,
    GrepTool, KillShellTool, LsTool, NotebookEditTool, NotebookReadTool, ReadTool,
    SkillTool, SlashCommandTool, TaskOutputTool, TaskTool, TodoWriteTool, Tool,
    WebFetchTool, WebSearchTool, WriteTool,
};
use crate::settings::McpServerConfig;

/// MCP Server for managing and executing tools
pub struct McpServer {
    /// Tool registry
    registry: ToolRegistry,
    /// Server name
    name: String,
    /// Server version
    version: String,
    /// External MCP server manager
    external: Arc<ExternalMcpManager>,
}

impl std::fmt::Debug for McpServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpServer")
            .field("registry", &self.registry)
            .field("name", &self.name)
            .field("version", &self.version)
            .field("external", &"<ExternalMcpManager>")
            .finish()
    }
}

impl McpServer {
    /// Create a new MCP server with default tools
    pub fn new() -> Self {
        let mut server = Self {
            registry: ToolRegistry::new(),
            name: "claude-code-acp-rs".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            external: Arc::new(ExternalMcpManager::new()),
        };

        // Register built-in tools
        server.register_builtin_tools();

        server
    }

    /// Create a new MCP server with custom name and version
    pub fn with_info(name: impl Into<String>, version: impl Into<String>) -> Self {
        let mut server = Self {
            registry: ToolRegistry::new(),
            name: name.into(),
            version: version.into(),
            external: Arc::new(ExternalMcpManager::new()),
        };

        server.register_builtin_tools();

        server
    }

    /// Create an empty MCP server without built-in tools
    pub fn empty() -> Self {
        Self {
            registry: ToolRegistry::new(),
            name: "claude-code-acp-rs".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            external: Arc::new(ExternalMcpManager::new()),
        }
    }

    /// Register all built-in tools
    fn register_builtin_tools(&mut self) {
        self.registry.register(ReadTool::new());
        self.registry.register(WriteTool::new());
        self.registry.register(EditTool::new());
        self.registry.register(BashTool::new());
        self.registry.register(BashOutputTool);
        self.registry.register(KillShellTool);
        self.registry.register(GlobTool::new());
        self.registry.register(GrepTool::new());
        self.registry.register(LsTool::new());
        self.registry.register(TodoWriteTool::new());
        self.registry.register(ExitPlanModeTool::new());
        self.registry.register(WebFetchTool::new());
        self.registry.register(WebSearchTool::new());
        self.registry.register(NotebookReadTool::new());
        self.registry.register(NotebookEditTool::new());
        self.registry.register(TaskTool::new());
        self.registry.register(TaskOutputTool::new());
        self.registry.register(AskUserQuestionTool::new());
        self.registry.register(SlashCommandTool::new());
        self.registry.register(SkillTool::new());
    }

    /// Get the server name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the server version
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Register a custom tool
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.registry.register(tool);
    }

    /// Register a custom tool as Arc
    pub fn register_arc(&mut self, tool: Arc<dyn Tool>) {
        self.registry.register_arc(tool);
    }

    /// Get a tool by name
    pub fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.registry.get(name)
    }

    /// Check if a tool exists
    pub fn has_tool(&self, name: &str) -> bool {
        self.registry.contains(name)
    }

    /// Get all tool names
    pub fn tool_names(&self) -> Vec<&str> {
        self.registry.names()
    }

    /// Get the number of registered tools
    pub fn tool_count(&self) -> usize {
        self.registry.len()
    }

    /// Get all tool schemas (including external MCP tools)
    pub fn tool_schemas(&self) -> Vec<ToolSchema> {
        self.registry.schemas()
    }

    /// Get all tool schemas including external MCP tools (async)
    pub async fn all_tool_schemas(&self) -> Vec<ToolSchema> {
        let mut schemas = self.registry.schemas();
        schemas.extend(self.external.all_tools());
        schemas
    }

    /// Execute a tool by name
    ///
    /// Routes to external MCP servers for tools with format `mcp__<server>__<tool>`
    pub async fn execute(
        &self,
        name: &str,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> ToolResult {
        // Check if this is an external MCP tool
        if ExternalMcpManager::is_external_tool(name) {
            return match self.external.call_tool(name, input).await {
                Ok(result) => result,
                Err(e) => ToolResult::error(format!("External MCP error: {}", e)),
            };
        }

        // Execute built-in tool
        self.registry.execute(name, input, context).await
    }

    /// Connect to external MCP servers from configuration
    ///
    /// # Arguments
    ///
    /// * `servers` - MCP server configurations from settings
    /// * `cwd` - Working directory for relative paths
    #[tracing::instrument(
        name = "connect_external_mcp_servers",
        skip(self, servers, cwd),
        fields(
            server_count = servers.len(),
        )
    )]
    pub async fn connect_external_servers(
        &self,
        servers: &std::collections::HashMap<String, McpServerConfig>,
        cwd: Option<&Path>,
    ) -> Vec<ExternalMcpError> {
        let start_time = std::time::Instant::now();
        let mut errors = Vec::new();
        let mut success_count = 0;
        let mut skip_count = 0;
        let total_count = servers.len();

        tracing::info!(
            total_servers = total_count,
            cwd = ?cwd,
            "Starting to connect external MCP servers"
        );

        for (name, config) in servers {
            // Skip disabled servers
            if config.disabled {
                skip_count += 1;
                tracing::debug!(
                    server_name = %name,
                    "Skipping disabled MCP server"
                );
                continue;
            }

            tracing::info!(
                server_name = %name,
                command = %config.command,
                args = ?config.args,
                "Connecting to external MCP server"
            );

            let server_start = std::time::Instant::now();
            if let Err(e) = self
                .external
                .connect(
                    name.clone(),
                    &config.command,
                    &config.args,
                    config.env.as_ref(),
                    cwd,
                )
                .await
            {
                let elapsed = server_start.elapsed();
                tracing::error!(
                    server_name = %name,
                    command = %config.command,
                    error = %e,
                    elapsed_ms = elapsed.as_millis(),
                    "Failed to connect to MCP server"
                );
                errors.push(e);
            } else {
                success_count += 1;
                let elapsed = server_start.elapsed();
                tracing::info!(
                    server_name = %name,
                    elapsed_ms = elapsed.as_millis(),
                    "Successfully connected to MCP server"
                );
            }
        }

        let total_elapsed = start_time.elapsed();
        tracing::info!(
            total_servers = total_count,
            success_count = success_count,
            error_count = errors.len(),
            skip_count = skip_count,
            total_elapsed_ms = total_elapsed.as_millis(),
            "Finished connecting external MCP servers"
        );

        errors
    }

    /// Get the external MCP manager
    pub fn external_manager(&self) -> &Arc<ExternalMcpManager> {
        &self.external
    }

    /// Get the tool registry
    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write as IoWrite;
    use tempfile::TempDir;

    #[test]
    fn test_server_new() {
        let server = McpServer::new();
        assert_eq!(server.name(), "claude-code-acp-rs");
        assert!(!server.version().is_empty());

        // Should have built-in tools
        assert!(server.has_tool("Read"));
        assert!(server.has_tool("Write"));
        assert!(server.has_tool("Edit"));
        assert!(server.has_tool("Bash"));
        assert!(server.has_tool("BashOutput"));
        assert!(server.has_tool("KillShell"));
        assert!(server.has_tool("Glob"));
        assert!(server.has_tool("Grep"));
        assert!(server.has_tool("LS"));
        assert!(server.has_tool("TodoWrite"));
        assert!(server.has_tool("ExitPlanMode"));
        assert!(server.has_tool("WebFetch"));
        assert!(server.has_tool("WebSearch"));
        assert!(server.has_tool("NotebookRead"));
        assert!(server.has_tool("NotebookEdit"));
        assert!(server.has_tool("Task"));
        assert!(server.has_tool("TaskOutput"));
        assert_eq!(server.tool_count(), 20);
    }

    #[test]
    fn test_server_empty() {
        let server = McpServer::empty();
        assert_eq!(server.tool_count(), 0);
        assert!(!server.has_tool("Read"));
    }

    #[test]
    fn test_server_with_info() {
        let server = McpServer::with_info("custom-server", "1.0.0");
        assert_eq!(server.name(), "custom-server");
        assert_eq!(server.version(), "1.0.0");
    }

    #[test]
    fn test_tool_names() {
        let server = McpServer::new();
        let names = server.tool_names();

        assert!(names.contains(&"Read"));
        assert!(names.contains(&"Write"));
        assert!(names.contains(&"Edit"));
        assert!(names.contains(&"Bash"));
        assert!(names.contains(&"BashOutput"));
        assert!(names.contains(&"KillShell"));
        assert!(names.contains(&"Glob"));
        assert!(names.contains(&"Grep"));
        assert!(names.contains(&"LS"));
        assert!(names.contains(&"TodoWrite"));
        assert!(names.contains(&"ExitPlanMode"));
        assert!(names.contains(&"WebFetch"));
        assert!(names.contains(&"WebSearch"));
        assert!(names.contains(&"Task"));
        assert!(names.contains(&"TaskOutput"));
    }

    #[test]
    fn test_tool_schemas() {
        let server = McpServer::new();
        let schemas = server.tool_schemas();

        assert_eq!(schemas.len(), 20);

        // Check that each schema has required fields
        for schema in &schemas {
            assert!(!schema.name.is_empty());
            assert!(!schema.description.is_empty());
            assert!(schema.input_schema.is_object());
        }
    }

    #[tokio::test]
    async fn test_execute_read_tool() {
        let server = McpServer::new();
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let mut file = std::fs::File::create(&file_path).unwrap();
        writeln!(file, "Test content").unwrap();

        let context = ToolContext::new("test-session", temp_dir.path());

        let result = server
            .execute(
                "Read",
                json!({"file_path": file_path.to_str().unwrap()}),
                &context,
            )
            .await;

        assert!(!result.is_error);
        assert!(result.content.contains("Test content"));
    }

    #[tokio::test]
    async fn test_execute_unknown_tool() {
        let server = McpServer::new();
        let temp_dir = TempDir::new().unwrap();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = server.execute("UnknownTool", json!({}), &context).await;

        assert!(result.is_error);
        assert!(result.content.contains("not found"));
    }

    #[tokio::test]
    async fn test_execute_write_tool() {
        let server = McpServer::new();
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("new_file.txt");

        let context = ToolContext::new("test-session", temp_dir.path());

        let result = server
            .execute(
                "Write",
                json!({
                    "file_path": file_path.to_str().unwrap(),
                    "content": "New content"
                }),
                &context,
            )
            .await;

        assert!(!result.is_error);
        assert!(file_path.exists());
    }

    #[tokio::test]
    async fn test_execute_bash_tool() {
        let server = McpServer::new();
        let temp_dir = TempDir::new().unwrap();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = server
            .execute(
                "Bash",
                json!({"command": "echo 'Hello from bash'"}),
                &context,
            )
            .await;

        assert!(!result.is_error);
        assert!(result.content.contains("Hello from bash"));
    }

    #[test]
    fn test_acp_prefix_has_tool() {
        let server = McpServer::new();

        // Direct names
        assert!(server.has_tool("Read"));
        assert!(server.has_tool("Bash"));
        assert!(server.has_tool("Glob"));

        // With ACP prefix
        assert!(server.has_tool("mcp__acp__Read"));
        assert!(server.has_tool("mcp__acp__Bash"));
        assert!(server.has_tool("mcp__acp__Glob"));

        // Unknown tool
        assert!(!server.has_tool("mcp__acp__Unknown"));
    }

    #[test]
    fn test_acp_prefix_get_tool() {
        let server = McpServer::new();

        // Get with direct name
        let read_tool = server.get_tool("Read");
        assert!(read_tool.is_some());
        assert_eq!(read_tool.unwrap().name(), "Read");

        // Get with ACP prefix
        let read_tool_prefixed = server.get_tool("mcp__acp__Read");
        assert!(read_tool_prefixed.is_some());
        assert_eq!(read_tool_prefixed.unwrap().name(), "Read");
    }

    #[tokio::test]
    async fn test_execute_with_acp_prefix() {
        let server = McpServer::new();
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let mut file = std::fs::File::create(&file_path).unwrap();
        writeln!(file, "Test content").unwrap();

        let context = ToolContext::new("test-session", temp_dir.path());

        // Execute with ACP prefix
        let result = server
            .execute(
                "mcp__acp__Read",
                json!({"file_path": file_path.to_str().unwrap()}),
                &context,
            )
            .await;

        assert!(!result.is_error);
        assert!(result.content.contains("Test content"));
    }

    #[tokio::test]
    async fn test_external_mcp_tool_routing() {
        let server = McpServer::new();
        let temp_dir = TempDir::new().unwrap();
        let context = ToolContext::new("test-session", temp_dir.path());

        // External MCP tool should be recognized and routed
        // (will fail because no server is connected, but tests routing logic)
        let result = server
            .execute(
                "mcp__filesystem__read_file",
                json!({"path": "/tmp/test.txt"}),
                &context,
            )
            .await;

        // Should return an error since server is not connected
        assert!(result.is_error);
        assert!(result.content.contains("External MCP error"));
    }

    #[tokio::test]
    async fn test_all_tool_schemas_includes_external() {
        let server = McpServer::new();
        let schemas = server.all_tool_schemas().await;

        // Should have built-in tools
        assert!(schemas.len() >= 17);

        // Schemas should include built-in tools
        assert!(schemas.iter().any(|s| s.name == "Read"));
        assert!(schemas.iter().any(|s| s.name == "Bash"));
    }

    #[test]
    fn test_external_manager_accessible() {
        let server = McpServer::new();
        let manager = server.external_manager();

        // Should be able to access the external manager
        assert!(!ExternalMcpManager::is_external_tool("Read"));
        assert!(ExternalMcpManager::is_external_tool("mcp__server__tool"));
        let _ = manager; // Use the manager
    }
}
