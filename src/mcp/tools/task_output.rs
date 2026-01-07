//! TaskOutput tool for retrieving output from background tasks
//!
//! Retrieves output from running or completed tasks (background shells, agents, or remote sessions).

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use super::base::Tool;
use crate::mcp::registry::{ToolContext, ToolResult};

/// Input parameters for TaskOutput
#[derive(Debug, Deserialize)]
struct TaskOutputInput {
    /// The task ID to get output from
    task_id: String,
    /// Whether to wait for completion
    #[serde(default = "default_block")]
    block: bool,
    /// Max wait time in milliseconds
    #[serde(default = "default_timeout")]
    timeout: u64,
}

fn default_block() -> bool {
    true
}

fn default_timeout() -> u64 {
    30000
}

/// TaskOutput tool for retrieving task results
#[derive(Debug, Default)]
pub struct TaskOutputTool;

impl TaskOutputTool {
    /// Create a new TaskOutput tool
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for TaskOutputTool {
    fn name(&self) -> &str {
        "TaskOutput"
    }

    fn description(&self) -> &str {
        "Retrieves output from a running or completed task (background shell, agent, or remote session). \
         Takes a task_id parameter identifying the task. Returns the task output along with status information. \
         Use block=true (default) to wait for task completion. Use block=false for non-blocking check of current status."
    }

    fn input_schema(&self) -> Value {
        json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["task_id"],
            "additionalProperties": false,
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The task ID to get output from"
                },
                "block": {
                    "type": "boolean",
                    "default": true,
                    "description": "Whether to wait for completion"
                },
                "timeout": {
                    "type": "number",
                    "default": 30000,
                    "minimum": 0,
                    "maximum": 600_000,
                    "description": "Max wait time in ms"
                }
            }
        })
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> ToolResult {
        // Parse input
        let params: TaskOutputInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid input: {}", e)),
        };

        // Validate timeout
        if params.timeout > 600_000 {
            return ToolResult::error("Timeout cannot exceed 600000ms (10 minutes)");
        }

        tracing::info!(
            "TaskOutput request for task: {} (session: {})",
            params.task_id,
            context.session_id
        );

        // Note: Full implementation would:
        // 1. Look up the task by ID from a task registry
        // 2. If blocking, wait for completion up to timeout
        // 3. Return task output and status
        // 4. Handle different task types (shell, agent, remote)

        let mut output = format!("Task output for: {}\n\n", params.task_id);
        output.push_str(&format!("Blocking: {}\n", params.block));
        output.push_str(&format!("Timeout: {}ms\n", params.timeout));
        output.push_str("\nStatus: Task not found\n");
        output.push_str(
            "\nNote: TaskOutput tool requires task registry integration for full functionality. \
             Use the /tasks command to see available task IDs.",
        );

        ToolResult::success(output).with_metadata(json!({
            "task_id": params.task_id,
            "status": "not_found",
            "block": params.block,
            "timeout": params.timeout
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_task_output_properties() {
        let tool = TaskOutputTool::new();
        assert_eq!(tool.name(), "TaskOutput");
        assert!(tool.description().contains("task"));
        assert!(tool.description().contains("output"));
    }

    #[test]
    fn test_task_output_input_schema() {
        let tool = TaskOutputTool::new();
        let schema = tool.input_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["task_id"].is_object());
        assert!(schema["properties"]["block"].is_object());
        assert!(schema["properties"]["timeout"].is_object());

        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("task_id")));
    }

    #[tokio::test]
    async fn test_task_output_execute() {
        let temp_dir = TempDir::new().unwrap();
        let tool = TaskOutputTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "task_id": "test-task-123"
                }),
                &context,
            )
            .await;

        assert!(!result.is_error);
        assert!(result.content.contains("test-task-123"));
        assert!(result.content.contains("not found"));
    }

    #[tokio::test]
    async fn test_task_output_non_blocking() {
        let temp_dir = TempDir::new().unwrap();
        let tool = TaskOutputTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "task_id": "test-task-456",
                    "block": false
                }),
                &context,
            )
            .await;

        assert!(!result.is_error);
        assert!(result.content.contains("Blocking: false"));
    }

    #[tokio::test]
    async fn test_task_output_with_timeout() {
        let temp_dir = TempDir::new().unwrap();
        let tool = TaskOutputTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "task_id": "test-task-789",
                    "timeout": 60000
                }),
                &context,
            )
            .await;

        assert!(!result.is_error);
        assert!(result.content.contains("Timeout: 60000ms"));
    }

    #[tokio::test]
    async fn test_task_output_timeout_too_large() {
        let temp_dir = TempDir::new().unwrap();
        let tool = TaskOutputTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "task_id": "test-task",
                    "timeout": 999_999
                }),
                &context,
            )
            .await;

        assert!(result.is_error);
        assert!(result.content.contains("600000ms"));
    }
}
