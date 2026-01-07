//! MCP (Model Context Protocol) Server integration
//!
//! This module provides built-in tools that can be used by Claude Code.
//! The tools follow the MCP pattern but are implemented natively for efficiency.
//!
//! ## Tool Categories
//!
//! - **File Tools**: Read, Write, Edit - File system operations
//! - **Terminal Tools**: Bash, KillShell - Command execution
//! - **Search Tools**: Grep, Glob - Code search
//!
//! ## External MCP Servers
//!
//! The `external` module provides support for connecting to external MCP servers
//! to extend tool capabilities.
//!
//! ## ACP Integration
//!
//! The `acp_server` module provides an MCP server that integrates with the ACP
//! protocol, allowing tools to send notifications during execution.

mod acp_server;
mod external;
mod registry;
mod server;
pub mod tools;

pub use acp_server::{AcpMcpServer, get_disallowed_tools};
pub use external::{ExternalMcpError, ExternalMcpManager, ExternalMcpServer};
pub use registry::{ToolContext, ToolRegistry, ToolResult, ToolStatus, ACP_TOOL_PREFIX};
pub use server::McpServer;
pub use tools::Tool;
