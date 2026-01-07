//! Tool information extraction from tool calls
//!
//! Extracts user-friendly information from tool calls for UI display.

use crate::types::{ToolInfo, ToolKind};

/// ACP tool name prefix for SDK MCP server tools
const ACP_TOOL_PREFIX: &str = "mcp__acp__";

/// Strip the ACP prefix from a tool name if present
fn strip_acp_prefix(name: &str) -> &str {
    name.strip_prefix(ACP_TOOL_PREFIX).unwrap_or(name)
}

/// Extract tool information from a tool name and input
///
/// This creates a `ToolInfo` struct with a human-readable title,
/// tool kind classification, and relevant locations (e.g., file paths).
///
/// # Arguments
///
/// * `name` - The tool name (e.g., "Read", "Bash", "Edit", "mcp__acp__Bash")
/// * `input` - The tool input parameters as JSON
///
/// # Returns
///
/// A `ToolInfo` with populated fields for UI display
pub fn extract_tool_info(name: &str, input: &serde_json::Value) -> ToolInfo {
    // Strip mcp__acp__ prefix for ACP tools to use the same display logic
    let effective_name = strip_acp_prefix(name);

    match effective_name {
        "Read" => {
            let path = input
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("file");
            let title = format!("Read {}", truncate_path(path));
            ToolInfo::new(title, ToolKind::Read).with_location(path)
        }

        "Edit" => {
            let path = input
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("file");
            let title = format!("Edit {}", truncate_path(path));
            ToolInfo::new(title, ToolKind::Edit).with_location(path)
        }

        "Write" => {
            let path = input
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("file");
            let title = format!("Write {}", truncate_path(path));
            ToolInfo::new(title, ToolKind::Edit).with_location(path)
        }

        "Bash" => {
            let cmd = input.get("command").and_then(|v| v.as_str()).unwrap_or("");
            let desc = input.get("description").and_then(|v| v.as_str());

            let title = desc
                .map(String::from)
                .unwrap_or_else(|| format!("Run: {}", truncate_string(cmd, 50)));

            ToolInfo::new(title, ToolKind::Execute)
        }

        "Grep" => {
            let pattern = input.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
            let title = format!("Search: {}", truncate_string(pattern, 40));
            ToolInfo::new(title, ToolKind::Search)
        }

        "Glob" => {
            let pattern = input.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
            let title = format!("Find: {}", truncate_string(pattern, 40));
            ToolInfo::new(title, ToolKind::Search)
        }

        "WebFetch" => {
            let url = input.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let title = format!("Fetch {}", truncate_string(url, 50));
            ToolInfo::new(title, ToolKind::Fetch)
        }

        "WebSearch" => {
            let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let title = format!("Search: {}", truncate_string(query, 40));
            ToolInfo::new(title, ToolKind::Fetch)
        }

        "Task" => {
            let desc = input
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("Task");
            ToolInfo::new(desc, ToolKind::Think)
        }

        "TodoWrite" => ToolInfo::new("Update task list", ToolKind::Think),

        "EnterPlanMode" | "ExitPlanMode" => {
            ToolInfo::new(name.to_string(), ToolKind::SwitchMode)
        }

        "AskUserQuestion" => ToolInfo::new("Ask question", ToolKind::Other),

        "NotebookRead" | "NotebookEdit" => {
            let path = input
                .get("notebook_path")
                .and_then(|v| v.as_str())
                .unwrap_or("notebook");
            let title = format!("{} {}", name, truncate_path(path));
            let kind = if name == "NotebookRead" {
                ToolKind::Read
            } else {
                ToolKind::Edit
            };
            ToolInfo::new(title, kind).with_location(path)
        }

        // MCP tools (format: mcp__server__tool)
        // Note: mcp__acp__* tools are already handled above via strip_acp_prefix
        name if name.starts_with("mcp__") && !name.starts_with(ACP_TOOL_PREFIX) => {
            let parts: Vec<&str> = name.split("__").collect();
            let tool_name = parts.get(2).unwrap_or(&name);
            ToolInfo::new(format!("MCP: {tool_name}"), ToolKind::Other)
        }

        // Default case
        _ => ToolInfo::new(effective_name.to_string(), ToolKind::Other),
    }
}

/// Truncate a file path for display
fn truncate_path(path: &str) -> String {
    // Get just the filename if path is long
    if path.len() > 60 {
        std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .map(String::from)
            .unwrap_or_else(|| truncate_string(path, 60))
    } else {
        path.to_string()
    }
}

/// Truncate a string to a maximum length
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_read_tool_info() {
        let input = json!({"file_path": "/path/to/file.rs"});
        let info = extract_tool_info("Read", &input);

        assert_eq!(info.kind, ToolKind::Read);
        assert!(info.title.contains("file.rs"));
        assert!(info.locations.is_some());
    }

    #[test]
    fn test_extract_bash_tool_info() {
        let input = json!({
            "command": "cargo build",
            "description": "Build the project"
        });
        let info = extract_tool_info("Bash", &input);

        assert_eq!(info.kind, ToolKind::Execute);
        assert_eq!(info.title, "Build the project");
    }

    #[test]
    fn test_extract_bash_tool_info_no_description() {
        let input = json!({"command": "cargo test --release"});
        let info = extract_tool_info("Bash", &input);

        assert_eq!(info.kind, ToolKind::Execute);
        assert!(info.title.starts_with("Run:"));
    }

    #[test]
    fn test_extract_grep_tool_info() {
        let input = json!({"pattern": "fn main"});
        let info = extract_tool_info("Grep", &input);

        assert_eq!(info.kind, ToolKind::Search);
        assert!(info.title.contains("fn main"));
    }

    #[test]
    fn test_extract_mcp_tool_info() {
        let input = json!({});
        let info = extract_tool_info("mcp__server__custom_tool", &input);

        assert_eq!(info.kind, ToolKind::Other);
        assert!(info.title.contains("custom_tool"));
    }

    #[test]
    fn test_extract_acp_bash_tool_info() {
        // mcp__acp__Bash should display like Bash
        let input = json!({"command": "tree -L 2 -d"});
        let info = extract_tool_info("mcp__acp__Bash", &input);

        assert_eq!(info.kind, ToolKind::Execute);
        assert!(info.title.contains("tree"));
        assert!(!info.title.contains("MCP")); // Should NOT show "MCP: Bash"
    }

    #[test]
    fn test_extract_acp_read_tool_info() {
        // mcp__acp__Read should display like Read
        let input = json!({"file_path": "/path/to/file.rs"});
        let info = extract_tool_info("mcp__acp__Read", &input);

        assert_eq!(info.kind, ToolKind::Read);
        assert!(info.title.contains("Read"));
        assert!(info.title.contains("file.rs"));
        assert!(!info.title.contains("MCP"));
    }

    #[test]
    fn test_truncate_long_path() {
        let long_path = "/very/long/path/to/some/deeply/nested/directory/structure/file.rs";
        let truncated = truncate_path(long_path);
        assert!(truncated.len() <= 60 || truncated == "file.rs");
    }

    #[test]
    fn test_truncate_string() {
        assert_eq!(truncate_string("short", 10), "short");
        assert_eq!(truncate_string("this is a longer string", 10), "this is...");
    }
}
