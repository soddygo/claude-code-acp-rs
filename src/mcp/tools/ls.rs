//! LS tool for directory listing
//!
//! Lists directory contents with support for ignore patterns.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

use super::base::Tool;
use crate::mcp::registry::{ToolContext, ToolResult};

/// Maximum entries to return
const MAX_ENTRIES: usize = 1000;

/// LS tool for directory listing
#[derive(Debug, Default)]
pub struct LsTool;

/// Input parameters for LS
#[derive(Debug, Deserialize)]
struct LsInput {
    /// The path to list
    path: String,
    /// Patterns to ignore
    #[serde(default)]
    ignore: Option<Vec<String>>,
}

impl LsTool {
    /// Create a new LS tool instance
    pub fn new() -> Self {
        Self
    }

    /// Check if a name matches any ignore pattern
    fn should_ignore(name: &str, ignore_patterns: &[String]) -> bool {
        for pattern in ignore_patterns {
            // Simple glob matching for common patterns
            if pattern.starts_with('*') && pattern.len() > 1 {
                // *.ext pattern
                let suffix = &pattern[1..];
                if name.ends_with(suffix) {
                    return true;
                }
            } else if pattern.ends_with('*') && pattern.len() > 1 {
                // prefix* pattern
                let prefix = &pattern[..pattern.len() - 1];
                if name.starts_with(prefix) {
                    return true;
                }
            } else if name == pattern {
                // Exact match
                return true;
            }
        }
        false
    }
}

#[async_trait]
impl Tool for LsTool {
    fn name(&self) -> &str {
        "LS"
    }

    fn description(&self) -> &str {
        "Lists directory contents. Returns files and subdirectories with their types. \
         Supports ignore patterns to filter results."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The path to the directory to list"
                },
                "ignore": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Patterns to ignore (e.g., ['node_modules', '*.log', '.git'])"
                }
            }
        })
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> ToolResult {
        // Parse input
        let params: LsInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid input: {}", e)),
        };

        // Resolve path
        let target_path = {
            let path = Path::new(&params.path);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                context.cwd.join(path)
            }
        };

        // Validate path exists
        if !target_path.exists() {
            return ToolResult::error(format!("Path not found: {}", target_path.display()));
        }

        if !target_path.is_dir() {
            return ToolResult::error(format!(
                "Path is not a directory: {}",
                target_path.display()
            ));
        }

        // Get ignore patterns
        let ignore_patterns = params.ignore.unwrap_or_default();

        // Read directory entries
        let entries = match fs::read_dir(&target_path) {
            Ok(e) => e,
            Err(e) => {
                return ToolResult::error(format!(
                    "Failed to read directory {}: {}",
                    target_path.display(),
                    e
                ))
            }
        };

        // Collect and format entries
        let mut dirs: Vec<String> = Vec::new();
        let mut files: Vec<String> = Vec::new();
        let mut total_count = 0;

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();

            // Check ignore patterns
            if Self::should_ignore(&name, &ignore_patterns) {
                continue;
            }

            total_count += 1;
            if total_count > MAX_ENTRIES {
                break;
            }

            // Categorize as file or directory
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_dir() {
                    dirs.push(format!("{}/", name));
                } else if file_type.is_symlink() {
                    files.push(format!("{} -> (symlink)", name));
                } else {
                    files.push(name);
                }
            }
        }

        // Sort entries
        dirs.sort();
        files.sort();

        // Format output
        let truncated = total_count > MAX_ENTRIES;
        let mut output = String::new();

        // Add directories first
        if !dirs.is_empty() {
            output.push_str("Directories:\n");
            for dir in &dirs {
                output.push_str("  ");
                output.push_str(dir);
                output.push('\n');
            }
        }

        // Add files
        if !files.is_empty() {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str("Files:\n");
            for file in &files {
                output.push_str("  ");
                output.push_str(file);
                output.push('\n');
            }
        }

        // Empty directory case
        if output.is_empty() {
            output = format!("Directory {} is empty", target_path.display());
        }

        // Add truncation notice
        if truncated {
            output.push_str(&format!(
                "\n... (showing {} entries, more exist)",
                MAX_ENTRIES
            ));
        }

        // Add summary
        output.push_str(&format!(
            "\n\nTotal: {} directories, {} files",
            dirs.len(),
            files.len()
        ));

        ToolResult::success(output).with_metadata(json!({
            "path": target_path.display().to_string(),
            "directories": dirs.len(),
            "files": files.len(),
            "truncated": truncated
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_ls_tool_properties() {
        let tool = LsTool::new();
        assert_eq!(tool.name(), "LS");
        assert!(tool.description().contains("directory"));
    }

    #[test]
    fn test_ls_input_schema() {
        let tool = LsTool::new();
        let schema = tool.input_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["required"].as_array().unwrap().contains(&json!("path")));
    }

    #[tokio::test]
    async fn test_ls_directory() {
        let temp_dir = TempDir::new().unwrap();

        // Create test structure
        fs::create_dir(temp_dir.path().join("src")).unwrap();
        fs::create_dir(temp_dir.path().join("tests")).unwrap();
        File::create(temp_dir.path().join("README.md"))
            .unwrap()
            .write_all(b"# README")
            .unwrap();
        File::create(temp_dir.path().join("Cargo.toml"))
            .unwrap()
            .write_all(b"[package]")
            .unwrap();

        let tool = LsTool::new();
        let context = ToolContext::new("test", temp_dir.path());

        let result = tool
            .execute(json!({"path": "."}), &context)
            .await;

        assert!(!result.is_error);
        assert!(result.content.contains("src/"));
        assert!(result.content.contains("tests/"));
        assert!(result.content.contains("README.md"));
        assert!(result.content.contains("Cargo.toml"));
    }

    #[tokio::test]
    async fn test_ls_with_ignore() {
        let temp_dir = TempDir::new().unwrap();

        // Create test structure
        fs::create_dir(temp_dir.path().join("node_modules")).unwrap();
        fs::create_dir(temp_dir.path().join("src")).unwrap();
        File::create(temp_dir.path().join("app.log"))
            .unwrap()
            .write_all(b"log")
            .unwrap();
        File::create(temp_dir.path().join("main.rs"))
            .unwrap()
            .write_all(b"fn main() {}")
            .unwrap();

        let tool = LsTool::new();
        let context = ToolContext::new("test", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "path": ".",
                    "ignore": ["node_modules", "*.log"]
                }),
                &context,
            )
            .await;

        assert!(!result.is_error);
        assert!(!result.content.contains("node_modules"));
        assert!(!result.content.contains("app.log"));
        assert!(result.content.contains("src/"));
        assert!(result.content.contains("main.rs"));
    }

    #[tokio::test]
    async fn test_ls_empty_directory() {
        let temp_dir = TempDir::new().unwrap();

        let tool = LsTool::new();
        let context = ToolContext::new("test", temp_dir.path());

        let result = tool
            .execute(json!({"path": "."}), &context)
            .await;

        assert!(!result.is_error);
        assert!(result.content.contains("empty") || result.content.contains("Total: 0"));
    }

    #[tokio::test]
    async fn test_ls_nonexistent_path() {
        let temp_dir = TempDir::new().unwrap();

        let tool = LsTool::new();
        let context = ToolContext::new("test", temp_dir.path());

        let result = tool
            .execute(json!({"path": "nonexistent"}), &context)
            .await;

        assert!(result.is_error);
        assert!(result.content.contains("not found"));
    }

    #[test]
    fn test_should_ignore() {
        // Exact match
        assert!(LsTool::should_ignore("node_modules", &["node_modules".to_string()]));
        assert!(!LsTool::should_ignore("src", &["node_modules".to_string()]));

        // Suffix pattern (*.ext)
        assert!(LsTool::should_ignore("app.log", &["*.log".to_string()]));
        assert!(!LsTool::should_ignore("app.txt", &["*.log".to_string()]));

        // Prefix pattern (prefix*)
        assert!(LsTool::should_ignore(".gitignore", &[".*".to_string()]));
        assert!(!LsTool::should_ignore("src", &[".*".to_string()]));
    }
}
