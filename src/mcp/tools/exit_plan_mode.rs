//! ExitPlanMode tool
//!
//! Signals that the plan mode is complete and ready for user approval.
//! This tool is used when planning implementation steps and the plan has been
//! written to a plan file.

use async_trait::async_trait;
use serde_json::{json, Value};

use super::base::Tool;
use crate::mcp::registry::{ToolContext, ToolResult};

/// ExitPlanMode tool for signaling completion of plan mode
#[derive(Debug, Default)]
pub struct ExitPlanModeTool;

impl ExitPlanModeTool {
    /// Create a new ExitPlanMode tool
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ExitPlanModeTool {
    fn name(&self) -> &str {
        "ExitPlanMode"
    }

    fn description(&self) -> &str {
        "Use this tool when you are in plan mode and have finished writing your plan to the plan file \
         and are ready for user approval. This tool signals that you're done planning and ready for \
         the user to review and approve. IMPORTANT: Only use this tool for tasks that require planning \
         the implementation steps of code changes, not for research tasks."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "additionalProperties": true,
            "properties": {},
            "required": []
        })
    }

    async fn execute(&self, _input: Value, context: &ToolContext) -> ToolResult {
        // Log that plan mode is being exited
        tracing::info!(
            "ExitPlanMode called for session {} in {}",
            context.session_id,
            context.cwd.display()
        );

        // Return success message
        // The actual plan file should have been written before calling this tool
        let output = "Plan mode exited. The plan is ready for user review and approval. \
                     Once approved, implementation can begin.";

        ToolResult::success(output).with_metadata(json!({
            "action": "exit_plan_mode",
            "status": "awaiting_approval"
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_exit_plan_mode_properties() {
        let tool = ExitPlanModeTool::new();
        assert_eq!(tool.name(), "ExitPlanMode");
        assert!(tool.description().contains("plan"));
        assert!(tool.description().contains("approval"));
    }

    #[test]
    fn test_exit_plan_mode_input_schema() {
        let tool = ExitPlanModeTool::new();
        let schema = tool.input_schema();

        assert_eq!(schema["type"], "object");
        // No required fields
        assert!(schema["required"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_exit_plan_mode_execute() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ExitPlanModeTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool.execute(json!({}), &context).await;

        assert!(!result.is_error);
        assert!(result.content.contains("Plan mode exited"));
        assert!(result.content.contains("approval"));
    }

    #[tokio::test]
    async fn test_exit_plan_mode_with_extra_params() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ExitPlanModeTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        // Should accept additional properties gracefully
        let result = tool
            .execute(json!({"extra_field": "ignored"}), &context)
            .await;

        assert!(!result.is_error);
    }
}
