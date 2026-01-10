//! Task tool for launching sub-agents
//!
//! Launches specialized agents to handle complex, multi-step tasks autonomously.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use super::base::Tool;
use crate::mcp::registry::{ToolContext, ToolResult};

/// Input parameters for Task
#[derive(Debug, Deserialize)]
struct TaskInput {
    /// A short description of the task (3-5 words)
    description: String,
    /// The task prompt for the agent
    prompt: String,
    /// The type of specialized agent to use
    subagent_type: String,
    /// Optional model to use for the agent
    #[serde(default)]
    model: Option<String>,
    /// Optional agent ID to resume from
    #[serde(default)]
    resume: Option<String>,
    /// Whether to run this agent in the background
    #[serde(default)]
    run_in_background: Option<bool>,
}

/// Available agent types
const AGENT_TYPES: &[&str] = &[
    "general-purpose",
    "statusline-setup",
    "Explore",
    "Plan",
    "claude-code-guide",
];

/// Task tool for launching sub-agents
#[derive(Debug, Default)]
pub struct TaskTool;

impl TaskTool {
    /// Create a new Task tool
    pub fn new() -> Self {
        Self
    }

    /// Validate the agent type
    fn validate_agent_type(agent_type: &str) -> bool {
        AGENT_TYPES.contains(&agent_type)
    }
}

#[async_trait]
impl Tool for TaskTool {
    fn name(&self) -> &str {
        "Task"
    }

    fn description(&self) -> &str {
        "Launch a new agent to handle complex, multi-step tasks autonomously. \
         The Task tool launches specialized agents (subprocesses) that autonomously \
         handle complex tasks. Each agent type has specific capabilities and tools available to it."
    }

    fn input_schema(&self) -> Value {
        json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["description", "prompt", "subagent_type"],
            "additionalProperties": false,
            "properties": {
                "description": {
                    "type": "string",
                    "description": "A short (3-5 word) description of the task"
                },
                "prompt": {
                    "type": "string",
                    "description": "The task for the agent to perform"
                },
                "subagent_type": {
                    "type": "string",
                    "description": "The type of specialized agent to use for this task"
                },
                "model": {
                    "type": "string",
                    "enum": ["sonnet", "opus", "haiku"],
                    "description": "Optional model to use for this agent. If not specified, inherits from parent."
                },
                "resume": {
                    "type": "string",
                    "description": "Optional agent ID to resume from. If provided, the agent continues from the previous execution transcript."
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Set to true to run this agent in the background. Use TaskOutput to read the output later."
                }
            }
        })
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> ToolResult {
        // Parse input
        let params: TaskInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid input: {}", e)),
        };

        // Validate description length
        let word_count = params.description.split_whitespace().count();
        if word_count > 10 {
            return ToolResult::error(
                "Description should be short (3-5 words). Provided description is too long.",
            );
        }

        // Validate agent type
        if !Self::validate_agent_type(&params.subagent_type) {
            return ToolResult::error(format!(
                "Unknown agent type '{}'. Available types: {}",
                params.subagent_type,
                AGENT_TYPES.join(", ")
            ));
        }

        tracing::info!(
            "Task request: type={}, description='{}' (session: {})",
            params.subagent_type,
            params.description,
            context.session_id
        );

        // Generate a task ID
        let task_id = uuid::Uuid::new_v4().to_string();

        // Build response based on parameters
        let mut output = String::new();

        if let Some(resume_id) = &params.resume {
            output.push_str(&format!(
                "Resuming agent {} with ID: {}\n\n",
                params.subagent_type, resume_id
            ));
        } else {
            output.push_str(&format!(
                "Launched {} agent: {}\n\n",
                params.subagent_type, params.description
            ));
        }

        output.push_str(&format!("Agent ID: {}\n", task_id));
        output.push_str(&format!("Subagent type: {}\n", params.subagent_type));

        if let Some(model) = &params.model {
            output.push_str(&format!("Model: {}\n", model));
        }

        if params.run_in_background.unwrap_or(false) {
            output.push_str("Status: Running in background\n");
            output.push_str("Use TaskOutput tool to retrieve results when ready.\n");
        } else {
            output.push_str("Status: Completed\n");
        }

        output.push_str(&format!("\nPrompt: {}\n", params.prompt));

        // Note: Full implementation would:
        // 1. Create a new agent instance with the specified type
        // 2. Configure it with appropriate tools based on agent type
        // 3. Execute the prompt and capture results
        // 4. Support background execution and resume functionality
        output.push_str(
            "\nNote: Task tool requires agent orchestration integration for full functionality.",
        );

        ToolResult::success(output).with_metadata(json!({
            "task_id": task_id,
            "subagent_type": params.subagent_type,
            "description": params.description,
            "model": params.model,
            "run_in_background": params.run_in_background.unwrap_or(false),
            "status": if params.run_in_background.unwrap_or(false) { "running" } else { "completed" }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_task_properties() {
        let tool = TaskTool::new();
        assert_eq!(tool.name(), "Task");
        assert!(tool.description().contains("agent"));
        assert!(tool.description().contains("complex"));
    }

    #[test]
    fn test_task_input_schema() {
        let tool = TaskTool::new();
        let schema = tool.input_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["description"].is_object());
        assert!(schema["properties"]["prompt"].is_object());
        assert!(schema["properties"]["subagent_type"].is_object());
        assert!(schema["properties"]["model"].is_object());
        assert!(schema["properties"]["resume"].is_object());
        assert!(schema["properties"]["run_in_background"].is_object());

        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("description")));
        assert!(required.contains(&json!("prompt")));
        assert!(required.contains(&json!("subagent_type")));
    }

    #[tokio::test]
    async fn test_task_execute() {
        let temp_dir = TempDir::new().unwrap();
        let tool = TaskTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "description": "Search for files",
                    "prompt": "Find all Rust source files",
                    "subagent_type": "Explore"
                }),
                &context,
            )
            .await;

        assert!(!result.is_error);
        assert!(result.content.contains("Explore"));
        assert!(result.content.contains("Agent ID"));
    }

    #[tokio::test]
    async fn test_task_with_model() {
        let temp_dir = TempDir::new().unwrap();
        let tool = TaskTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "description": "Quick task",
                    "prompt": "Do something simple",
                    "subagent_type": "general-purpose",
                    "model": "haiku"
                }),
                &context,
            )
            .await;

        assert!(!result.is_error);
        assert!(result.content.contains("haiku"));
    }

    #[tokio::test]
    async fn test_task_background() {
        let temp_dir = TempDir::new().unwrap();
        let tool = TaskTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "description": "Background task",
                    "prompt": "Run something in background",
                    "subagent_type": "general-purpose",
                    "run_in_background": true
                }),
                &context,
            )
            .await;

        assert!(!result.is_error);
        assert!(result.content.contains("Running in background"));
        assert!(result.content.contains("TaskOutput"));
    }

    #[tokio::test]
    async fn test_task_resume() {
        let temp_dir = TempDir::new().unwrap();
        let tool = TaskTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "description": "Resume task",
                    "prompt": "Continue previous work",
                    "subagent_type": "Explore",
                    "resume": "previous-agent-id-123"
                }),
                &context,
            )
            .await;

        assert!(!result.is_error);
        assert!(result.content.contains("Resuming"));
        assert!(result.content.contains("previous-agent-id-123"));
    }

    #[tokio::test]
    async fn test_task_invalid_agent_type() {
        let temp_dir = TempDir::new().unwrap();
        let tool = TaskTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "description": "Test task",
                    "prompt": "Do something",
                    "subagent_type": "invalid-type"
                }),
                &context,
            )
            .await;

        assert!(result.is_error);
        assert!(result.content.contains("Unknown agent type"));
    }

    #[tokio::test]
    async fn test_task_long_description() {
        let temp_dir = TempDir::new().unwrap();
        let tool = TaskTool::new();
        let context = ToolContext::new("test-session", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "description": "This is a very long description that contains way too many words",
                    "prompt": "Do something",
                    "subagent_type": "Explore"
                }),
                &context,
            )
            .await;

        assert!(result.is_error);
        assert!(result.content.contains("too long"));
    }

    #[test]
    fn test_validate_agent_type() {
        assert!(TaskTool::validate_agent_type("general-purpose"));
        assert!(TaskTool::validate_agent_type("Explore"));
        assert!(TaskTool::validate_agent_type("Plan"));
        assert!(!TaskTool::validate_agent_type("unknown"));
    }
}
