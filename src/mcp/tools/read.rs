//! Read tool implementation
//!
//! Reads file contents from the filesystem.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::base::{Tool, ToolKind};
use crate::mcp::registry::{ToolContext, ToolResult};

/// Read tool for reading file contents
#[derive(Debug, Default)]
pub struct ReadTool;

/// Read tool input parameters
#[derive(Debug, Deserialize)]
struct ReadInput {
    /// Path to the file to read
    file_path: String,
    /// Optional line offset to start reading from (1-indexed)
    #[serde(default)]
    offset: Option<usize>,
    /// Optional maximum number of lines to read
    #[serde(default)]
    limit: Option<usize>,
}

impl ReadTool {
    /// Create a new Read tool instance
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "Read"
    }

    fn description(&self) -> &str {
        "Read the contents of a file from the filesystem. Supports reading specific line ranges with offset and limit parameters."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["file_path"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to read"
                },
                "offset": {
                    "type": "integer",
                    "description": "Line number to start reading from (1-indexed). Defaults to 1."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to read. Defaults to reading entire file."
                }
            }
        })
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Read
    }

    fn requires_permission(&self) -> bool {
        false // Reading doesn't require explicit permission
    }

    async fn execute(&self, input: serde_json::Value, context: &ToolContext) -> ToolResult {
        // Parse input
        let params: ReadInput = match serde_json::from_value(input) {
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

        // Check if it's a file
        if !path.is_file() {
            return ToolResult::error(format!("Not a file: {}", path.display()));
        }

        // Read file content with timing
        let read_start = std::time::Instant::now();
        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => {
                let read_duration = read_start.elapsed();
                return ToolResult::error(format!(
                    "Failed to read file: {} (elapsed: {}ms)",
                    e,
                    read_duration.as_millis()
                ));
            }
        };
        let read_duration = read_start.elapsed();

        tracing::debug!(
            file_path = %path.display(),
            file_size_bytes = content.len(),
            read_duration_ms = read_duration.as_millis(),
            "File read completed"
        );

        // Apply offset and limit
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let offset = params.offset.unwrap_or(1).saturating_sub(1); // Convert to 0-indexed
        let limit = params.limit.unwrap_or(lines.len());

        if offset >= lines.len() {
            return ToolResult::success("").with_metadata(json!({
                "total_lines": total_lines,
                "returned_lines": 0
            }));
        }

        let selected_lines: Vec<String> = lines
            .iter()
            .skip(offset)
            .take(limit)
            .enumerate()
            .map(|(i, line)| format!("{:6}→{}", offset + i + 1, line))
            .collect();

        let returned_lines = selected_lines.len();

        // Calculate display path:
        // - If file is under cwd, show relative path with ./ prefix for cwd files
        // - If file is outside cwd, show absolute path
        let display_path = if let Ok(rel) = path.strip_prefix(&context.cwd) {
            let rel_str = rel.to_string_lossy();
            if rel_str.is_empty() {
                // File is the cwd directory itself (unlikely)
                path.display().to_string()
            } else if rel_str.contains('/') {
                // File in subdirectory: crates/rcoder/Cargo.toml
                rel_str.to_string()
            } else {
                // File directly in cwd: add ./ prefix
                format!("./{}", rel_str)
            }
        } else {
            // File outside cwd: show absolute path
            path.display().to_string()
        };

        // Add file header with path and line range information
        let header = format!(
            "File: {} (lines {}-{} of {}, total {} lines)\n{}\n",
            display_path,
            offset + 1,
            offset + returned_lines.min(total_lines),
            total_lines,
            total_lines,
            "-".repeat(60)
        );

        let result = format!("{}\n{}", header, selected_lines.join("\n"));

        tracing::info!(
            file_path = %path.display(),
            total_lines = total_lines,
            returned_lines = returned_lines,
            offset = offset + 1,
            "File read successfully"
        );

        ToolResult::success(result).with_metadata(json!({
            "total_lines": total_lines,
            "returned_lines": returned_lines,
            "offset": offset + 1,
            "path": path.display().to_string(),
            "read_duration_ms": read_duration.as_millis(),
            "file_size_bytes": content.len()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_read_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let mut file = std::fs::File::create(&file_path).unwrap();
        writeln!(file, "Line 1").unwrap();
        writeln!(file, "Line 2").unwrap();
        writeln!(file, "Line 3").unwrap();

        let tool = ReadTool::new();
        let context = ToolContext::new("test", temp_dir.path());

        let result = tool
            .execute(json!({"file_path": file_path.to_str().unwrap()}), &context)
            .await;

        assert!(!result.is_error);
        assert!(result.content.contains("Line 1"));
        assert!(result.content.contains("Line 2"));
        assert!(result.content.contains("Line 3"));
    }

    #[tokio::test]
    async fn test_read_with_offset_and_limit() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let mut file = std::fs::File::create(&file_path).unwrap();
        for i in 1..=10 {
            writeln!(file, "Line {}", i).unwrap();
        }

        let tool = ReadTool::new();
        let context = ToolContext::new("test", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "file_path": file_path.to_str().unwrap(),
                    "offset": 3,
                    "limit": 2
                }),
                &context,
            )
            .await;

        assert!(!result.is_error);
        assert!(result.content.contains("Line 3"));
        assert!(result.content.contains("Line 4"));
        assert!(!result.content.contains("Line 5"));
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ReadTool::new();
        let context = ToolContext::new("test", temp_dir.path());

        let result = tool
            .execute(json!({"file_path": "/nonexistent/file.txt"}), &context)
            .await;

        assert!(result.is_error);
        assert!(result.content.contains("not found"));
    }

    #[test]
    fn test_read_tool_properties() {
        let tool = ReadTool::new();
        assert_eq!(tool.name(), "Read");
        assert_eq!(tool.kind(), ToolKind::Read);
        assert!(!tool.requires_permission());
    }
}
