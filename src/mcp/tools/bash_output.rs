//! BashOutput tool for retrieving output from background shell processes
//!
//! This tool retrieves incremental output from a running or completed background
//! shell process started with `run_in_background=true`.
//!
//! Supports two execution modes:
//! - Direct process execution: shell IDs starting with "shell-"
//! - Terminal API: shell IDs starting with "term-" (Client-side PTY)

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use super::base::Tool;
use crate::mcp::registry::{ToolContext, ToolResult};
use crate::terminal::TerminalId;

/// Prefix for Terminal API shell IDs
const TERMINAL_API_PREFIX: &str = "term-";

/// BashOutput tool implementation
#[derive(Debug, Default)]
pub struct BashOutputTool;

/// Input parameters for BashOutput
#[derive(Debug, Deserialize)]
struct BashOutputInput {
    /// The ID of the background shell to get output from
    bash_id: String,
}

#[async_trait]
impl Tool for BashOutputTool {
    fn name(&self) -> &str {
        "BashOutput"
    }

    fn description(&self) -> &str {
        "Retrieves output from a running or completed background bash shell. \
         Use this to check on the progress of commands started with run_in_background=true."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "bash_id": {
                    "type": "string",
                    "description": "The ID of the background shell returned when the command was started"
                }
            },
            "required": ["bash_id"]
        })
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> ToolResult {
        // Parse input
        let params: BashOutputInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid input: {}", e)),
        };

        // Check if this is a Terminal API shell
        if let Some(terminal_id) = params.bash_id.strip_prefix(TERMINAL_API_PREFIX) {
            return self.get_terminal_output(terminal_id, context).await;
        }

        // Fall back to background process manager
        self.get_background_output(&params.bash_id, context).await
    }
}

impl BashOutputTool {
    /// Get output from Terminal API
    async fn get_terminal_output(&self, terminal_id: &str, context: &ToolContext) -> ToolResult {
        let Some(terminal_client) = context.terminal_client() else {
            return ToolResult::error("Terminal API not available");
        };

        let tid = TerminalId::new(terminal_id.to_string());

        // Get output from terminal
        match terminal_client.output(tid).await {
            Ok(response) => {
                let status = match &response.exit_status {
                    Some(exit_status) => {
                        if let Some(code) = exit_status.exit_code {
                            if code == 0 {
                                "completed (exit code 0)".to_string()
                            } else {
                                format!("completed (exit code {})", code)
                            }
                        } else if exit_status.signal.is_some() {
                            format!("killed (signal: {:?})", exit_status.signal)
                        } else {
                            "completed".to_string()
                        }
                    }
                    None => "running".to_string(),
                };

                let output = &response.output;
                let response_text = if output.is_empty() {
                    format!("Status: {}\n\n(No output yet)", status)
                } else {
                    format!("Status: {}\n\n{}", status, output)
                };

                ToolResult::success(response_text).with_metadata(json!({
                    "terminal_id": terminal_id,
                    "status": status,
                    "terminal_api": true
                }))
            }
            Err(e) => ToolResult::error(format!("Failed to get terminal output: {}", e)),
        }
    }

    /// Get output from background process manager
    async fn get_background_output(&self, bash_id: &str, context: &ToolContext) -> ToolResult {
        // Get the background process manager from context
        let Some(manager) = context.background_processes() else {
            return ToolResult::error("Background process manager not available");
        };

        // Get the terminal
        let Some(terminal) = manager.get(bash_id) else {
            return ToolResult::error(format!("Unknown shell ID: {}", bash_id));
        };

        // Get incremental output
        let output = terminal.get_incremental_output().await;
        let status = terminal.status_str();

        // Format response
        let response = if output.is_empty() {
            format!("Status: {}\n\n(No new output)", status)
        } else {
            format!("Status: {}\n\n{}", status, output)
        };

        ToolResult::success(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bash_output_tool_properties() {
        let tool = BashOutputTool;
        assert_eq!(tool.name(), "BashOutput");
        assert!(tool.description().contains("background"));
    }

    #[test]
    fn test_bash_output_input_schema() {
        let tool = BashOutputTool;
        let schema = tool.input_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["bash_id"].is_object());
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!("bash_id"))
        );
    }
}
