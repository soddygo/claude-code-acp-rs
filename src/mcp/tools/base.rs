//! Base tool trait definition

use async_trait::async_trait;

use crate::mcp::registry::{ToolContext, ToolResult};

/// Tool trait for MCP-compatible tools
///
/// Tools implement this trait to provide functionality that can be
/// invoked by Claude or other agents.
#[async_trait]
pub trait Tool: Send + Sync + std::fmt::Debug {
    /// Get the tool name
    fn name(&self) -> &str;

    /// Get the tool description
    fn description(&self) -> &str;

    /// Get the JSON Schema for the tool's input parameters
    fn input_schema(&self) -> serde_json::Value;

    /// Execute the tool with the given input
    async fn execute(&self, input: serde_json::Value, context: &ToolContext) -> ToolResult;

    /// Check if this tool requires permission before execution
    fn requires_permission(&self) -> bool {
        true
    }

    /// Get the tool category/kind
    fn kind(&self) -> ToolKind {
        ToolKind::Other
    }
}

/// Tool categories for UI display
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    /// File reading operations
    Read,
    /// File editing/writing operations
    Edit,
    /// Command execution
    Execute,
    /// Search operations
    Search,
    /// Network/fetch operations
    Fetch,
    /// Thinking/planning operations
    Think,
    /// Other operations
    Other,
}

impl ToolKind {
    /// Get a human-readable label for the kind
    pub fn label(&self) -> &'static str {
        match self {
            ToolKind::Read => "Read",
            ToolKind::Edit => "Edit",
            ToolKind::Execute => "Execute",
            ToolKind::Search => "Search",
            ToolKind::Fetch => "Fetch",
            ToolKind::Think => "Think",
            ToolKind::Other => "Tool",
        }
    }
}
