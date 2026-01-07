//! KillShell tool for terminating background shell processes
//!
//! This tool kills a running background shell process started with
//! `run_in_background=true`.
//!
//! Supports two execution modes:
//! - Direct process execution: shell IDs starting with "shell-"
//! - Terminal API: shell IDs starting with "term-" (Client-side PTY)

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use super::base::Tool;
use crate::mcp::registry::{ToolContext, ToolResult};
use crate::session::{BackgroundTerminal, TerminalExitStatus};
use crate::terminal::TerminalId;

/// Prefix for Terminal API shell IDs
const TERMINAL_API_PREFIX: &str = "term-";

/// KillShell tool implementation
#[derive(Debug, Default)]
pub struct KillShellTool;

/// Input parameters for KillShell
#[derive(Debug, Deserialize)]
struct KillShellInput {
    /// The ID of the background shell to kill
    shell_id: String,
}

#[async_trait]
impl Tool for KillShellTool {
    fn name(&self) -> &str {
        "KillShell"
    }

    fn description(&self) -> &str {
        "Kills a running background bash shell. Use this to terminate long-running \
         commands that were started with run_in_background=true."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "shell_id": {
                    "type": "string",
                    "description": "The ID of the background shell to kill"
                }
            },
            "required": ["shell_id"]
        })
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> ToolResult {
        // Parse input
        let params: KillShellInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid input: {}", e)),
        };

        // Check if this is a Terminal API shell
        if let Some(terminal_id) = params.shell_id.strip_prefix(TERMINAL_API_PREFIX) {
            return Self::kill_terminal(terminal_id, context).await;
        }

        // Fall back to background process manager
        Self::kill_background_process(&params.shell_id, context).await
    }
}

impl KillShellTool {
    /// Kill a terminal via Terminal API
    async fn kill_terminal(terminal_id: &str, context: &ToolContext) -> ToolResult {
        let Some(terminal_client) = context.terminal_client() else {
            return ToolResult::error("Terminal API not available");
        };

        let tid = TerminalId::new(terminal_id.to_string());

        // Kill the terminal
        match terminal_client.kill(tid.clone()).await {
            Ok(_) => {
                // Get final output
                let output = match terminal_client.output(tid.clone()).await {
                    Ok(resp) => resp.output,
                    Err(_) => String::new(),
                };

                // Release the terminal
                drop(terminal_client.release(tid).await);

                ToolResult::success(format!(
                    "Terminal command killed successfully.\n\nFinal output:\n{}",
                    if output.is_empty() {
                        "(No output)".to_string()
                    } else {
                        output
                    }
                )).with_metadata(json!({
                    "terminal_id": terminal_id,
                    "terminal_api": true
                }))
            }
            Err(e) => ToolResult::error(format!("Failed to kill terminal: {}", e)),
        }
    }

    /// Kill a background process via process manager
    async fn kill_background_process(shell_id: &str, context: &ToolContext) -> ToolResult {
        // Get the background process manager from context
        let Some(manager) = context.background_processes() else {
            return ToolResult::error("Background process manager not available");
        };

        // Get the terminal
        let Some(terminal) = manager.get_mut(shell_id) else {
            return ToolResult::error(format!("Unknown shell ID: {}", shell_id));
        };

        // Check the terminal state and kill if running
        match &*terminal {
            BackgroundTerminal::Running { child, output_buffer, .. } => {
                // Try to kill the process
                let mut child_guard = child.lock().await;
                match child_guard.kill().await {
                    Ok(()) => {
                        // Get final output before transitioning
                        let final_output = output_buffer.lock().await.clone();
                        drop(child_guard);
                        drop(terminal);

                        // Update terminal to finished state
                        manager.finish_terminal(shell_id, TerminalExitStatus::Killed).await;

                        ToolResult::success(format!(
                            "Command killed successfully.\n\nFinal output:\n{}",
                            if final_output.is_empty() {
                                "(No output)".to_string()
                            } else {
                                final_output
                            }
                        ))
                    }
                    Err(e) => {
                        ToolResult::error(format!("Failed to kill process: {}", e))
                    }
                }
            }
            BackgroundTerminal::Finished { status, final_output } => {
                let message = match status {
                    TerminalExitStatus::Exited(code) => {
                        format!("Command had already exited with code {}.", code)
                    }
                    TerminalExitStatus::Killed => "Command was already killed.".to_string(),
                    TerminalExitStatus::TimedOut => "Command was killed by timeout.".to_string(),
                    TerminalExitStatus::Aborted => "Command was aborted by user.".to_string(),
                };

                ToolResult::success(format!(
                    "{}\n\nFinal output:\n{}",
                    message,
                    if final_output.is_empty() {
                        "(No output)".to_string()
                    } else {
                        final_output.clone()
                    }
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kill_shell_tool_properties() {
        let tool = KillShellTool;
        assert_eq!(tool.name(), "KillShell");
        assert!(tool.description().contains("Kill"));
    }

    #[test]
    fn test_kill_shell_input_schema() {
        let tool = KillShellTool;
        let schema = tool.input_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["shell_id"].is_object());
        assert!(schema["required"].as_array().unwrap().contains(&json!("shell_id")));
    }
}
