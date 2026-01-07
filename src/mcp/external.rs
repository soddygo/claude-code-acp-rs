//! External MCP server management
//!
//! Supports connecting to external MCP servers for extended tool capabilities.

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::RwLock;

use super::registry::{ToolResult, ToolSchema};

/// External MCP server connection type
pub enum McpConnection {
    /// Stdio-based connection (spawned process)
    Stdio {
        /// The spawned process
        #[allow(dead_code)]
        child: Child,
        /// Writer to send messages
        stdin: ChildStdin,
        /// Reader to receive messages
        stdout: BufReader<ChildStdout>,
    },
}

/// External MCP server state
#[allow(missing_debug_implementations)]
pub struct ExternalMcpServer {
    /// Server name
    pub name: String,
    /// Connection to the server
    connection: McpConnection,
    /// Available tools from this server
    tools: Vec<ToolSchema>,
    /// Whether the server is initialized
    initialized: bool,
}

/// JSON-RPC request structure
#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    fn new(id: u64, method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: method.into(),
            params,
        }
    }
}

/// JSON-RPC response structure
#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: u64,
    result: Option<serde_json::Value>,
    error: Option<JsonRpcError>,
}

/// JSON-RPC error
#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

impl ExternalMcpServer {
    /// Connect to an external MCP server via stdio
    #[allow(clippy::unused_async)]
    pub async fn connect_stdio(
        name: String,
        command: &str,
        args: &[String],
        env: Option<&HashMap<String, String>>,
        cwd: Option<&Path>,
    ) -> Result<Self, ExternalMcpError> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        if let Some(env) = env {
            cmd.envs(env);
        }

        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }

        let mut child = cmd.spawn().map_err(|e| ExternalMcpError::SpawnFailed {
            command: command.to_string(),
            error: e.to_string(),
        })?;

        let stdin = child.stdin.take().ok_or(ExternalMcpError::NoStdin)?;
        let stdout = child
            .stdout
            .take()
            .ok_or(ExternalMcpError::NoStdout)
            .map(BufReader::new)?;

        let connection = McpConnection::Stdio {
            child,
            stdin,
            stdout,
        };

        Ok(Self {
            name,
            connection,
            tools: Vec::new(),
            initialized: false,
        })
    }

    /// Initialize the MCP server
    pub async fn initialize(&mut self) -> Result<(), ExternalMcpError> {
        // Send initialize request
        let request = JsonRpcRequest::new(
            1,
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "claude-code-acp-rs",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
        );

        let _response = self.send_request(request).await?;

        // Send initialized notification
        self.send_notification("notifications/initialized", None)
            .await?;

        // List available tools
        let tools_request = JsonRpcRequest::new(2, "tools/list", None);
        let tools_response = self.send_request(tools_request).await?;

        // Parse tools from response
        if let Some(result) = tools_response.result {
            if let Some(tools) = result.get("tools").and_then(|t| t.as_array()) {
                self.tools = tools
                    .iter()
                    .filter_map(|t| {
                        let name = t.get("name")?.as_str()?;
                        let description = t
                            .get("description")
                            .and_then(|d| d.as_str())
                            .unwrap_or("");
                        let input_schema = t
                            .get("inputSchema")
                            .cloned()
                            .unwrap_or(serde_json::json!({"type": "object"}));

                        Some(ToolSchema {
                            name: name.to_string(),
                            description: description.to_string(),
                            input_schema,
                        })
                    })
                    .collect();
            }
        }

        self.initialized = true;
        Ok(())
    }

    /// Send a JSON-RPC request and wait for response
    async fn send_request(
        &mut self,
        request: JsonRpcRequest,
    ) -> Result<JsonRpcResponse, ExternalMcpError> {
        let McpConnection::Stdio { stdin, stdout, .. } = &mut self.connection;

        // Serialize and send request
        let request_json = serde_json::to_string(&request)
            .map_err(|e| ExternalMcpError::SerializationError(e.to_string()))?;

        stdin
            .write_all(request_json.as_bytes())
            .await
            .map_err(|e| ExternalMcpError::WriteError(e.to_string()))?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|e| ExternalMcpError::WriteError(e.to_string()))?;
        stdin
            .flush()
            .await
            .map_err(|e| ExternalMcpError::WriteError(e.to_string()))?;

        // Read response
        let mut line = String::new();
        stdout
            .read_line(&mut line)
            .await
            .map_err(|e| ExternalMcpError::ReadError(e.to_string()))?;

        let response: JsonRpcResponse = serde_json::from_str(&line)
            .map_err(|e| ExternalMcpError::DeserializationError(e.to_string()))?;

        if let Some(error) = response.error {
            return Err(ExternalMcpError::RpcError {
                code: error.code,
                message: error.message,
            });
        }

        Ok(response)
    }

    /// Send a JSON-RPC notification (no response expected)
    async fn send_notification(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<(), ExternalMcpError> {
        let McpConnection::Stdio { stdin, .. } = &mut self.connection;

        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        let notification_json = serde_json::to_string(&notification)
            .map_err(|e| ExternalMcpError::SerializationError(e.to_string()))?;

        stdin
            .write_all(notification_json.as_bytes())
            .await
            .map_err(|e| ExternalMcpError::WriteError(e.to_string()))?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|e| ExternalMcpError::WriteError(e.to_string()))?;
        stdin
            .flush()
            .await
            .map_err(|e| ExternalMcpError::WriteError(e.to_string()))?;

        Ok(())
    }

    /// Call a tool on this server
    pub async fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<ToolResult, ExternalMcpError> {
        if !self.initialized {
            return Err(ExternalMcpError::NotInitialized);
        }

        let request = JsonRpcRequest::new(
            3,
            "tools/call",
            Some(serde_json::json!({
                "name": tool_name,
                "arguments": arguments
            })),
        );

        let response = self.send_request(request).await?;

        // Parse tool result
        if let Some(result) = response.result {
            // Check if result has content array (MCP format)
            if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
                let text: Vec<String> = content
                    .iter()
                    .filter_map(|c| {
                        if c.get("type").and_then(|t| t.as_str()) == Some("text") {
                            c.get("text").and_then(|t| t.as_str()).map(String::from)
                        } else {
                            None
                        }
                    })
                    .collect();

                let is_error = result
                    .get("isError")
                    .and_then(|e| e.as_bool())
                    .unwrap_or(false);

                if is_error {
                    return Ok(ToolResult::error(text.join("\n")));
                }
                return Ok(ToolResult::success(text.join("\n")));
            }

            // Fallback: return raw JSON
            Ok(ToolResult::success(result.to_string()))
        } else {
            Ok(ToolResult::success(""))
        }
    }

    /// Get available tools from this server
    pub fn tools(&self) -> &[ToolSchema] {
        &self.tools
    }

    /// Check if the server is initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

/// Manager for multiple external MCP servers
#[allow(missing_debug_implementations)]
pub struct ExternalMcpManager {
    /// Connected servers by name
    servers: RwLock<HashMap<String, ExternalMcpServer>>,
}

impl ExternalMcpManager {
    /// Create a new external MCP manager
    pub fn new() -> Self {
        Self {
            servers: RwLock::new(HashMap::new()),
        }
    }

    /// Connect to an MCP server
    pub async fn connect(
        &self,
        name: String,
        command: &str,
        args: &[String],
        env: Option<&HashMap<String, String>>,
        cwd: Option<&Path>,
    ) -> Result<(), ExternalMcpError> {
        let mut server =
            ExternalMcpServer::connect_stdio(name.clone(), command, args, env, cwd).await?;
        server.initialize().await?;

        tracing::info!(
            "Connected to MCP server '{}' with {} tools",
            name,
            server.tools().len()
        );

        self.servers.write().await.insert(name, server);
        Ok(())
    }

    /// Disconnect from an MCP server
    pub async fn disconnect(&self, name: &str) {
        self.servers.write().await.remove(name);
    }

    /// Get all connected server names
    pub async fn server_names(&self) -> Vec<String> {
        self.servers.read().await.keys().cloned().collect()
    }

    /// Get all available tools from all servers
    ///
    /// Tool names are prefixed with `mcp__<server>__`
    pub async fn all_tools(&self) -> Vec<ToolSchema> {
        let servers = self.servers.read().await;
        let mut tools = Vec::new();

        for (server_name, server) in servers.iter() {
            for tool in server.tools() {
                tools.push(ToolSchema {
                    name: format!("mcp__{}_{}", server_name, tool.name),
                    description: format!("[{}] {}", server_name, tool.description),
                    input_schema: tool.input_schema.clone(),
                });
            }
        }

        tools
    }

    /// Call a tool on an external server
    ///
    /// Tool name should be prefixed with `mcp__<server>__`
    pub async fn call_tool(
        &self,
        full_tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<ToolResult, ExternalMcpError> {
        // Parse server name and tool name from `mcp__<server>__<tool>`
        let parts: Vec<&str> = full_tool_name.splitn(3, "__").collect();
        if parts.len() != 3 || parts[0] != "mcp" {
            return Err(ExternalMcpError::InvalidToolName(full_tool_name.to_string()));
        }

        let server_name = parts[1];
        let tool_name = parts[2];

        let mut servers = self.servers.write().await;
        let server = servers
            .get_mut(server_name)
            .ok_or_else(|| ExternalMcpError::ServerNotFound(server_name.to_string()))?;

        server.call_tool(tool_name, arguments).await
    }

    /// Check if a tool name refers to an external MCP tool
    ///
    /// External MCP tools have the format `mcp__<server>__<tool>` where
    /// `<server>` is not "acp" (which is reserved for the ACP prefix).
    pub fn is_external_tool(name: &str) -> bool {
        if !name.starts_with("mcp__") {
            return false;
        }

        // Split by __ and check structure
        let parts: Vec<&str> = name.splitn(3, "__").collect();
        if parts.len() != 3 || parts[0] != "mcp" {
            return false;
        }

        // "acp" is reserved for the ACP tool prefix, not external MCP
        parts[1] != "acp"
    }
}

impl Default for ExternalMcpManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors for external MCP operations
#[derive(Debug, thiserror::Error)]
pub enum ExternalMcpError {
    /// Failed to spawn MCP server process
    #[error("Failed to spawn MCP server '{command}': {error}")]
    SpawnFailed { command: String, error: String },

    /// No stdin available
    #[error("No stdin available for MCP server")]
    NoStdin,

    /// No stdout available
    #[error("No stdout available for MCP server")]
    NoStdout,

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Deserialization error
    #[error("Deserialization error: {0}")]
    DeserializationError(String),

    /// Write error
    #[error("Write error: {0}")]
    WriteError(String),

    /// Read error
    #[error("Read error: {0}")]
    ReadError(String),

    /// RPC error from server
    #[error("RPC error {code}: {message}")]
    RpcError { code: i64, message: String },

    /// Server not initialized
    #[error("Server not initialized")]
    NotInitialized,

    /// Invalid tool name format
    #[error("Invalid tool name format: {0}")]
    InvalidToolName(String),

    /// Server not found
    #[error("MCP server not found: {0}")]
    ServerNotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_external_mcp_manager_new() {
        let _manager = ExternalMcpManager::new();
        // Just verify creation works
        assert!(ExternalMcpManager::is_external_tool("mcp__server__tool"));
        assert!(!ExternalMcpManager::is_external_tool("Read"));
        assert!(!ExternalMcpManager::is_external_tool("mcp__acp__Read"));
    }

    #[test]
    fn test_is_external_tool() {
        // External MCP tools
        assert!(ExternalMcpManager::is_external_tool("mcp__myserver__mytool"));
        assert!(ExternalMcpManager::is_external_tool(
            "mcp__filesystem__read_file"
        ));

        // Not external tools
        assert!(!ExternalMcpManager::is_external_tool("Read"));
        assert!(!ExternalMcpManager::is_external_tool("Bash"));
        assert!(!ExternalMcpManager::is_external_tool("mcp__acp__Read")); // ACP prefix, not external
        assert!(!ExternalMcpManager::is_external_tool("mcp__single")); // Not enough parts
    }

    #[tokio::test]
    async fn test_manager_server_names_empty() {
        let manager = ExternalMcpManager::new();
        let names = manager.server_names().await;
        assert!(names.is_empty());
    }

    #[tokio::test]
    async fn test_manager_all_tools_empty() {
        let manager = ExternalMcpManager::new();
        let tools = manager.all_tools().await;
        assert!(tools.is_empty());
    }
}
