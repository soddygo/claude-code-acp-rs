//! Skill tool implementation
//!
//! Allows the agent to execute user-invocable skills.
//!
//! This tool is primarily for permission control. The actual skill
//! execution is handled by the Claude agent SDK through its own mechanisms.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::mcp::{ToolContext, ToolResult, Tool};

/// Skill tool
#[derive(Debug, Default)]
pub struct SkillTool;

impl SkillTool {
    pub fn new() -> Self {
        Self
    }
}

/// Input parameters for Skill
#[derive(Debug, Deserialize)]
struct SkillInput {
    /// The skill name to invoke (e.g., "commit", "pdf")
    skill: String,
    /// Optional arguments for the skill
    #[serde(default)]
    args: String,
}

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &str {
        "Skill"
    }

    fn description(&self) -> &str {
        "Execute a user-invocable skill"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "title": "skill",
            "description": "Execute a user-invocable skill",
            "properties": {
                "skill": {
                    "type": "string",
                    "description": "The skill name to invoke (e.g., 'commit', 'pdf')"
                },
                "args": {
                    "type": "string",
                    "description": "Optional arguments for the skill"
                }
            },
            "required": ["skill"]
        })
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> ToolResult {
        // Parse input
        let params: SkillInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid input: {}", e)),
        };

        // Return success - the actual skill execution is handled by the SDK
        ToolResult::success(format!("Skill: '{}'", params.skill))
            .with_metadata(json!({
                "skill": params.skill,
                "args": params.args
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_tool_properties() {
        let tool = SkillTool;
        assert_eq!(tool.name(), "Skill");
        assert!(tool.description().contains("skill"));
    }

    #[test]
    fn test_skill_input_schema() {
        let tool = SkillTool;
        let schema = tool.input_schema();

        assert_eq!(schema["type"], "object");
        assert_eq!(schema["title"], "skill");
        assert!(schema["properties"]["skill"].is_object());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("skill"))
        );
    }
}
