//! Ask User Question tool implementation
//!
//! Allows the agent to ask the user a question during execution.
//!
//! This tool is primarily for permission control. The actual question
//! asking is handled by the Claude agent SDK through its own mechanisms.

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::mcp::{Tool, ToolContext, ToolResult};

/// Ask User Question tool
#[derive(Debug, Clone, Default)]
pub struct AskUserQuestionTool;

impl AskUserQuestionTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for AskUserQuestionTool {
    fn name(&self) -> &str {
        "AskUserQuestion"
    }

    fn description(&self) -> &str {
        "Ask the user a question during execution"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "title": "ask_user_question",
            "description": "Ask the user a question during execution",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "The question to ask the user"
                },
                "options": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Optional predefined options for the user to choose from"
                },
                "allow_freeform": {
                    "type": "boolean",
                    "description": "Whether to allow free-form text input",
                    "default": false
                }
            },
            "required": ["question"]
        })
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> ToolResult {
        // Extract parameters
        let question = input.get("question").and_then(|v| v.as_str()).unwrap_or("");

        let options = input.get("options").and_then(|v| v.as_array());
        let allow_freeform = input
            .get("allow_freeform")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Return success - the actual question asking is handled by the SDK
        ToolResult::success(format!("Question: '{}'", question)).with_metadata(json!({
            "question": question,
            "has_options": options.is_some(),
            "allow_freeform": allow_freeform
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ask_user_question_name() {
        let tool = AskUserQuestionTool;
        assert_eq!(tool.name(), "AskUserQuestion");
    }

    #[test]
    fn test_ask_user_question_input_schema() {
        let tool = AskUserQuestionTool;
        let schema = tool.input_schema();

        assert_eq!(schema["type"], "object");
        assert_eq!(schema["title"], "ask_user_question");
        assert!(schema["properties"]["question"].is_object());
    }

    #[tokio::test]
    async fn test_ask_user_question_execute() {
        let tool = AskUserQuestionTool;
        let context = ToolContext::new("test-session", std::path::Path::new("/tmp"));

        let input = json!({
            "question": "What is your favorite color?",
            "options": ["Red", "Green", "Blue"],
            "allow_freeform": true
        });

        let result = tool.execute(input, &context).await;

        // Should succeed
        assert!(!result.is_error);
    }
}
