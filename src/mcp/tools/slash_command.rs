//! SlashCommand tool implementation
//!
//! Allows the agent to execute user-invocable slash commands.
//!
//! This tool is primarily for permission control. The actual command
//! execution is handled by the Claude agent SDK through its own mechanisms.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::mcp::{ToolContext, ToolResult, Tool};

/// SlashCommand tool
#[derive(Debug, Default)]
pub struct SlashCommandTool;

impl SlashCommandTool {
    pub fn new() -> Self {
        Self
    }
}

/// Input parameters for SlashCommand
#[derive(Debug, Deserialize)]
struct SlashCommandInput {
    /// The slash command to execute (e.g., "commit", "review-pr")
    command: String,
    /// Optional arguments for the command
    #[serde(default)]
    args: String,
}

#[async_trait]
impl Tool for SlashCommandTool {
    fn name(&self) -> &str {
        "SlashCommand"
    }

    fn description(&self) -> &str {
        "Execute a user-invocable slash command"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "title": "slash_command",
            "description": "Execute a user-invocable slash command",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The slash command to execute (e.g., 'commit', 'review-pr')"
                },
                "args": {
                    "type": "string",
                    "description": "Optional arguments for the command"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> ToolResult {
        // Parse input
        let params: SlashCommandInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid input: {}", e)),
        };

        // Build the full command string
        let full_command = if params.args.is_empty() {
            format!("/{}", params.command)
        } else {
            format!("/{} {}", params.command, params.args)
        };

        // Return success - the actual command execution is handled by the SDK
        ToolResult::success(format!("Slash command: '{}'", full_command))
            .with_metadata(json!({
                "command": params.command,
                "args": params.args,
                "full_command": full_command
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slash_command_tool_properties() {
        let tool = SlashCommandTool;
        assert_eq!(tool.name(), "SlashCommand");
        assert!(tool.description().contains("slash command"));
    }

    #[test]
    fn test_slash_command_input_schema() {
        let tool = SlashCommandTool;
        let schema = tool.input_schema();

        assert_eq!(schema["type"], "object");
        assert_eq!(schema["title"], "slash_command");
        assert!(schema["properties"]["command"].is_object());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("command"))
        );
    }
}
