//! Edit tool implementation
//!
//! Performs string replacement edits in files.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::base::{Tool, ToolKind};
use crate::mcp::registry::{ToolContext, ToolResult};
// TODO: Uncomment when implementing permission checks
// use crate::settings::{PermissionCheckResult, PermissionDecision};

/// Edit tool for performing string replacements in files
#[derive(Debug, Default)]
pub struct EditTool;

/// Edit tool input parameters
#[derive(Debug, Deserialize)]
struct EditInput {
    /// Path to the file to edit
    file_path: String,
    /// String to search for
    old_string: String,
    /// String to replace with
    new_string: String,
    /// Whether to replace all occurrences (default: false)
    #[serde(default)]
    replace_all: bool,
}

impl EditTool {
    /// Create a new Edit tool instance
    pub fn new() -> Self {
        Self
    }

    /// Check permission before executing the tool
    ///
    /// TODO: Implement interactive permission request flow
    ///
    /// Current implementation: Always allow execution (commented out permission checks)
    ///
    /// Future implementation should:
    /// 1. Check for explicit deny rules - block if matched
    /// 2. Check for explicit allow rules - allow if matched
    /// 3. For "Ask" decisions - send permission request to client via PermissionManager
    /// 4. Wait for user response - allow or deny based on user choice
    ///
    /// Architecture note: SDK does NOT call can_use_tool for MCP tools, so we need
    /// to implement the permission request flow within the tool execution path.
    async fn check_permission(
        &self,
        _input: &serde_json::Value,
        _context: &ToolContext,
    ) -> Option<ToolResult> {
        // TODO: Implement permission checking
        // let Some(checker) = context.permission_checker.as_ref() else {
        //     return None;
        // };
        // let checker = checker.read().await;
        // let result: PermissionCheckResult = checker.check_permission("Edit", input);
        // match result.decision {
        //     PermissionDecision::Allow => None,
        //     PermissionDecision::Deny => Some(ToolResult::error(...)),
        //     PermissionDecision::Ask => {
        //         // Send permission request via PermissionManager
        //         // Wait for user response
        //         // Return result based on user choice
        //     }
        // }

        // Currently: Always allow execution
        None
    }
}

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "Edit"
    }

    fn description(&self) -> &str {
        "Perform a string replacement edit in a file. The old_string must match exactly and uniquely in the file (unless replace_all is true). Use this for precise, surgical edits."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["file_path", "old_string", "new_string"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to edit"
                },
                "old_string": {
                    "type": "string",
                    "description": "The exact string to find and replace"
                },
                "new_string": {
                    "type": "string",
                    "description": "The string to replace old_string with"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Whether to replace all occurrences. Default: false (requires unique match)"
                }
            }
        })
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Edit
    }

    fn requires_permission(&self) -> bool {
        true // Editing requires permission
    }

    async fn execute(&self, input: serde_json::Value, context: &ToolContext) -> ToolResult {
        // Check permission before executing
        if let Some(result) = self.check_permission(&input, context).await {
            return result;
        }

        // Parse input
        let params: EditInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid input: {}", e)),
        };

        // Resolve path relative to working directory if not absolute
        let path = if std::path::Path::new(&params.file_path).is_absolute() {
            std::path::PathBuf::from(&params.file_path)
        } else {
            context.cwd.join(&params.file_path)
        };

        // Check if file exists
        if !path.exists() {
            return ToolResult::error(format!("File not found: {}", path.display()));
        }

        // Read current content
        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => return ToolResult::error(format!("Failed to read file: {}", e)),
        };

        // Check if old_string exists
        let match_count = content.matches(&params.old_string).count();

        if match_count == 0 {
            return ToolResult::error(
                "String not found in file. The old_string must match exactly.",
            );
        }

        if match_count > 1 && !params.replace_all {
            return ToolResult::error(format!(
                "Found {} occurrences of the search string. Use replace_all: true to replace all, or provide a more unique string.",
                match_count
            ));
        }

        // Perform replacement
        let new_content = if params.replace_all {
            content.replace(&params.old_string, &params.new_string)
        } else {
            content.replacen(&params.old_string, &params.new_string, 1)
        };

        // Write updated content
        match tokio::fs::write(&path, &new_content).await {
            Ok(()) => {
                let replacements = if params.replace_all { match_count } else { 1 };

                // Generate a simple diff preview
                let diff_preview = generate_diff_preview(&params.old_string, &params.new_string);

                ToolResult::success(format!(
                    "Edited {} ({} replacement{})\n{}",
                    path.display(),
                    replacements,
                    if replacements > 1 { "s" } else { "" },
                    diff_preview
                ))
                .with_metadata(json!({
                    "path": path.display().to_string(),
                    "replacements": replacements,
                    "old_length": params.old_string.len(),
                    "new_length": params.new_string.len()
                }))
            }
            Err(e) => ToolResult::error(format!("Failed to write file: {}", e)),
        }
    }
}

/// Generate a simple diff preview
fn generate_diff_preview(old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let mut preview = String::new();

    // Show removed lines (truncated if too many)
    let max_lines = 5;
    for (i, line) in old_lines.iter().take(max_lines).enumerate() {
        preview.push_str(&format!("- {}\n", line));
        if i == max_lines - 1 && old_lines.len() > max_lines {
            preview.push_str(&format!(
                "  ... ({} more lines)\n",
                old_lines.len() - max_lines
            ));
        }
    }

    // Show added lines (truncated if too many)
    for (i, line) in new_lines.iter().take(max_lines).enumerate() {
        preview.push_str(&format!("+ {}\n", line));
        if i == max_lines - 1 && new_lines.len() > max_lines {
            preview.push_str(&format!(
                "  ... ({} more lines)\n",
                new_lines.len() - max_lines
            ));
        }
    }

    preview
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as IoWrite;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_edit_single_replacement() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let mut file = std::fs::File::create(&file_path).unwrap();
        writeln!(file, "Hello, World!").unwrap();
        writeln!(file, "Goodbye, World!").unwrap();

        let tool = EditTool::new();
        let context = ToolContext::new("test", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "file_path": file_path.to_str().unwrap(),
                    "old_string": "Hello",
                    "new_string": "Hi"
                }),
                &context,
            )
            .await;

        assert!(!result.is_error);

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("Hi, World!"));
        assert!(content.contains("Goodbye, World!"));
    }

    #[tokio::test]
    async fn test_edit_replace_all() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let mut file = std::fs::File::create(&file_path).unwrap();
        writeln!(file, "foo bar foo baz foo").unwrap();

        let tool = EditTool::new();
        let context = ToolContext::new("test", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "file_path": file_path.to_str().unwrap(),
                    "old_string": "foo",
                    "new_string": "qux",
                    "replace_all": true
                }),
                &context,
            )
            .await;

        assert!(!result.is_error);

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(!content.contains("foo"));
        assert_eq!(content.matches("qux").count(), 3);
    }

    #[tokio::test]
    async fn test_edit_multiple_matches_error() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let mut file = std::fs::File::create(&file_path).unwrap();
        writeln!(file, "foo bar foo").unwrap();

        let tool = EditTool::new();
        let context = ToolContext::new("test", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "file_path": file_path.to_str().unwrap(),
                    "old_string": "foo",
                    "new_string": "baz"
                }),
                &context,
            )
            .await;

        assert!(result.is_error);
        assert!(result.content.contains("2 occurrences"));
    }

    #[tokio::test]
    async fn test_edit_string_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        std::fs::write(&file_path, "Hello, World!").unwrap();

        let tool = EditTool::new();
        let context = ToolContext::new("test", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "file_path": file_path.to_str().unwrap(),
                    "old_string": "Goodbye",
                    "new_string": "Hi"
                }),
                &context,
            )
            .await;

        assert!(result.is_error);
        assert!(result.content.contains("not found"));
    }

    #[test]
    fn test_edit_tool_properties() {
        let tool = EditTool::new();
        assert_eq!(tool.name(), "Edit");
        assert_eq!(tool.kind(), ToolKind::Edit);
        assert!(tool.requires_permission());
    }

    #[test]
    fn test_diff_preview() {
        let preview = generate_diff_preview("old line", "new line");
        assert!(preview.contains("- old line"));
        assert!(preview.contains("+ new line"));
    }
}
