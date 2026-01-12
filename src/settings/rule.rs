//! Permission rule parsing and matching
//!
//! Implements rule parsing for allow/deny/ask permission rules with glob pattern support.

use std::path::Path;

use globset::{Glob, GlobMatcher};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::mcp::ExternalMcpManager;
use crate::mcp::tools::bash::contains_shell_operator;

/// Cached regex for parsing permission rules
/// Pattern: ToolName or ToolName(argument)
/// Compiled once and reused for better performance
static RULE_REGEX: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
    // This regex is statically known and will always compile correctly
    Regex::new(r"^(\w+)(?:\((.+)\))?$").expect("Invalid hardcoded regex pattern")
});

/// ACP tool name prefix
const ACP_TOOL_PREFIX: &str = "mcp__acp__";

/// Permission decision result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionDecision {
    /// Tool execution is allowed
    Allow,
    /// Tool execution is denied
    Deny,
    /// User should be asked for permission
    Ask,
}

/// Result of a permission check
#[derive(Debug, Clone)]
pub struct PermissionCheckResult {
    /// The decision
    pub decision: PermissionDecision,
    /// The rule that matched (if any)
    pub rule: Option<String>,
    /// The source of the rule (allow, deny, ask)
    pub source: Option<String>,
}

impl PermissionCheckResult {
    /// Create a new allow result
    pub fn allow(rule: impl Into<String>) -> Self {
        Self {
            decision: PermissionDecision::Allow,
            rule: Some(rule.into()),
            source: Some("allow".to_string()),
        }
    }

    /// Create a new deny result
    pub fn deny(rule: impl Into<String>) -> Self {
        Self {
            decision: PermissionDecision::Deny,
            rule: Some(rule.into()),
            source: Some("deny".to_string()),
        }
    }

    /// Create a new ask result with a rule
    pub fn ask_with_rule(rule: impl Into<String>) -> Self {
        Self {
            decision: PermissionDecision::Ask,
            rule: Some(rule.into()),
            source: Some("ask".to_string()),
        }
    }

    /// Create a default ask result (no matching rule)
    pub fn ask() -> Self {
        Self {
            decision: PermissionDecision::Ask,
            rule: None,
            source: None,
        }
    }
}

/// Permission settings from settings.json
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionSettings {
    /// Rules that allow tool execution
    #[serde(default)]
    pub allow: Option<Vec<String>>,

    /// Rules that deny tool execution
    #[serde(default)]
    pub deny: Option<Vec<String>>,

    /// Rules that require asking the user
    #[serde(default)]
    pub ask: Option<Vec<String>>,

    /// Additional directories that can be accessed
    #[serde(default)]
    pub additional_directories: Option<Vec<String>>,

    /// Default permission mode
    #[serde(default)]
    pub default_mode: Option<String>,
}

/// A parsed permission rule
#[derive(Debug, Clone)]
pub struct ParsedRule {
    /// The tool name (e.g., "Read", "Bash", "Edit")
    pub tool_name: String,
    /// The argument pattern (e.g., "./.env", "npm run:*")
    pub argument: Option<String>,
    /// Whether this is a wildcard rule (ends with :*)
    pub is_wildcard: bool,
    /// Compiled glob matcher for file paths
    glob_matcher: Option<GlobMatcher>,
}

impl ParsedRule {
    /// Parse a rule string like "Read", "Read(./.env)", "Bash(npm run:*)"
    pub fn parse(rule: &str) -> Self {
        // Use cached regex (compiled once at first use)
        // The regex is statically known and guaranteed to compile correctly
        if let Some(caps) = RULE_REGEX.captures(rule) {
            let tool_name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let argument = caps.get(2).map(|m| m.as_str().to_string());

            let is_wildcard = argument
                .as_ref()
                .map(|a| a.ends_with(":*"))
                .unwrap_or(false);

            // Strip :* suffix if present
            let argument = if is_wildcard {
                argument.map(|a| a.trim_end_matches(":*").to_string())
            } else {
                argument
            };

            Self {
                tool_name,
                argument,
                is_wildcard,
                glob_matcher: None,
            }
        } else {
            // Fallback: treat entire string as tool name
            Self {
                tool_name: rule.to_string(),
                argument: None,
                is_wildcard: false,
                glob_matcher: None,
            }
        }
    }

    /// Parse with glob compilation for file path rules
    pub fn parse_with_glob(rule: &str, cwd: &Path) -> Self {
        let mut parsed = Self::parse(rule);

        // Compile glob for file-related tools
        if let Some(ref arg) = parsed.argument {
            if is_file_tool(&parsed.tool_name) && !parsed.is_wildcard {
                let normalized = normalize_path(arg, cwd);
                if let Ok(glob) = Glob::new(&normalized) {
                    parsed.glob_matcher = Some(glob.compile_matcher());
                }
            }
        }

        parsed
    }

    /// Check if this rule matches a tool invocation
    pub fn matches(&self, tool_name: &str, tool_input: &serde_json::Value, cwd: &Path) -> bool {
        // Strip ACP prefix if present
        let stripped_name = tool_name.strip_prefix(ACP_TOOL_PREFIX).unwrap_or(tool_name);

        // Check if tool name matches (considering tool groups and MCP tools)
        if !self.matches_tool_name(stripped_name) {
            return false;
        }

        // If no argument specified, match all invocations of this tool
        let Some(ref pattern) = self.argument else {
            return true;
        };

        // Get the relevant argument from tool input
        let actual_arg = extract_tool_argument(stripped_name, tool_input);
        let Some(actual_arg) = actual_arg else {
            return false;
        };

        // Match based on tool type
        if is_bash_tool(stripped_name) {
            self.matches_bash_command(pattern, &actual_arg)
        } else if is_file_tool(stripped_name) {
            self.matches_file_path(pattern, &actual_arg, cwd)
        } else {
            // Exact match for other tools
            pattern == &actual_arg
        }
    }

    /// Check if tool name matches (considering tool groups and MCP tools)
    fn matches_tool_name(&self, tool_name: &str) -> bool {
        // Direct match
        if self.tool_name == tool_name {
            return true;
        }

        // Check if this is an external MCP tool and get its friendly name
        // This allows rules like "deny: [WebFetch]" to match "mcp__web-fetch__webReader"
        if let Some(friendly_name) = ExternalMcpManager::get_friendly_tool_name(tool_name) {
            if self.tool_name == friendly_name {
                return true;
            }
        }

        // Tool group matching
        match self.tool_name.as_str() {
            // Read rule matches Read, Grep, Glob, LS
            "Read" => matches!(tool_name, "Read" | "Grep" | "Glob" | "LS"),
            // Edit rule matches Edit, Write
            "Edit" => matches!(tool_name, "Edit" | "Write"),
            // Task rule matches Task, TaskOutput
            "Task" => matches!(tool_name, "Task" | "TaskOutput"),
            // Web rule matches WebSearch, WebFetch
            "Web" => matches!(tool_name, "WebSearch" | "WebFetch"),
            _ => false,
        }
    }

    /// Match bash command with prefix/exact matching
    fn matches_bash_command(&self, pattern: &str, command: &str) -> bool {
        if self.is_wildcard {
            // Prefix match with wildcard
            if let Some(remainder) = command.strip_prefix(pattern) {
                // Check remainder for shell operators (security)
                if contains_shell_operator(remainder) {
                    return false;
                }
                return true;
            }
            false
        } else {
            // Exact match
            pattern == command
        }
    }

    /// Match file path with glob pattern
    fn matches_file_path(&self, pattern: &str, file_path: &str, cwd: &Path) -> bool {
        // Use pre-compiled glob if available
        if let Some(ref matcher) = self.glob_matcher {
            let normalized_path = normalize_path(file_path, cwd);
            return matcher.is_match(&normalized_path);
        }

        // Fallback: compile glob on demand
        let normalized_pattern = normalize_path(pattern, cwd);
        let normalized_path = normalize_path(file_path, cwd);

        if let Ok(glob) = Glob::new(&normalized_pattern) {
            let matcher = glob.compile_matcher();
            return matcher.is_match(&normalized_path);
        }

        // Last resort: exact match
        normalized_pattern == normalized_path
    }
}

/// Normalize a file path, expanding ~ and resolving relative paths
fn normalize_path(path: &str, cwd: &Path) -> String {
    let path = if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            home.join(rest).to_string_lossy().to_string()
        } else {
            path.to_string()
        }
    } else if let Some(rest) = path.strip_prefix("./") {
        cwd.join(rest).to_string_lossy().to_string()
    } else if !Path::new(path).is_absolute() {
        cwd.join(path).to_string_lossy().to_string()
    } else {
        path.to_string()
    };

    // Normalize path separators and resolve ..
    Path::new(&path)
        .canonicalize()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(path)
}

/// Check if tool is bash-like (command execution)
fn is_bash_tool(tool_name: &str) -> bool {
    matches!(tool_name, "Bash" | "BashOutput" | "KillShell")
}

/// Check if tool operates on files
fn is_file_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "Read" | "Write" | "Edit" | "Grep" | "Glob" | "LS" | "NotebookRead" | "NotebookEdit"
    )
}

/// Extract the relevant argument from tool input for permission matching
fn extract_tool_argument(tool_name: &str, input: &serde_json::Value) -> Option<String> {
    match tool_name {
        // Bash tools use "command"
        "Bash" | "BashOutput" | "KillShell" => input
            .get("command")
            .and_then(|v| v.as_str())
            .map(String::from),
        // File tools use "file_path" or "path"
        "Read" | "Write" | "Edit" | "NotebookRead" | "NotebookEdit" => input
            .get("file_path")
            .or_else(|| input.get("path"))
            .and_then(|v| v.as_str())
            .map(String::from),
        // Search tools use "path" or "pattern"
        "Grep" | "Glob" | "LS" => input
            .get("path")
            .or_else(|| input.get("pattern"))
            .and_then(|v| v.as_str())
            .map(String::from),
        // Task tool: extract subagent_type for permission control
        "Task" => input
            .get("subagent_type")
            .or_else(|| input.get("description"))
            .and_then(|v| v.as_str())
            .map(String::from),
        // TaskOutput tool: extract task_id for permission control
        "TaskOutput" => input
            .get("task_id")
            .and_then(|v| v.as_str())
            .map(String::from),
        // TodoWrite tool: extract todos count for permission control
        "TodoWrite" => input
            .get("todos")
            .and_then(|v| v.as_array())
            .map(|arr| arr.len().to_string())
            .or_else(|| Some("0".to_string())),
        // SlashCommand tool: extract command for permission control
        "SlashCommand" => input
            .get("command")
            .and_then(|v| v.as_str())
            .map(String::from),
        // Skill tool: extract skill for permission control
        "Skill" => input
            .get("skill")
            .and_then(|v| v.as_str())
            .map(String::from),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{manager::Settings, permission_checker::PermissionChecker};
    use serde_json::json;
    use std::path::PathBuf;

    fn settings_with_permissions(permissions: PermissionSettings) -> Settings {
        Settings {
            permissions: Some(permissions),
            ..Default::default()
        }
    }

    #[test]
    fn test_parse_simple_rule() {
        let rule = ParsedRule::parse("Read");
        assert_eq!(rule.tool_name, "Read");
        assert!(rule.argument.is_none());
        assert!(!rule.is_wildcard);
    }

    #[test]
    fn test_parse_rule_with_argument() {
        let rule = ParsedRule::parse("Read(./.env)");
        assert_eq!(rule.tool_name, "Read");
        assert_eq!(rule.argument, Some("./.env".to_string()));
        assert!(!rule.is_wildcard);
    }

    #[test]
    fn test_parse_rule_with_wildcard() {
        let rule = ParsedRule::parse("Bash(npm run:*)");
        assert_eq!(rule.tool_name, "Bash");
        assert_eq!(rule.argument, Some("npm run".to_string()));
        assert!(rule.is_wildcard);
    }

    #[test]
    fn test_parse_glob_pattern() {
        let rule = ParsedRule::parse("Read(./secrets/**)");
        assert_eq!(rule.tool_name, "Read");
        assert_eq!(rule.argument, Some("./secrets/**".to_string()));
        assert!(!rule.is_wildcard);
    }

    #[test]
    fn test_matches_simple_tool() {
        let rule = ParsedRule::parse("Read");
        let cwd = PathBuf::from("/tmp");

        assert!(rule.matches("Read", &json!({}), &cwd));
        assert!(rule.matches("mcp__acp__Read", &json!({}), &cwd));
        assert!(!rule.matches("Write", &json!({}), &cwd));
    }

    #[test]
    fn test_matches_tool_group_read() {
        let rule = ParsedRule::parse("Read");
        let cwd = PathBuf::from("/tmp");

        // Read rule should match Read, Grep, Glob, LS
        assert!(rule.matches("Read", &json!({}), &cwd));
        assert!(rule.matches("Grep", &json!({}), &cwd));
        assert!(rule.matches("Glob", &json!({}), &cwd));
        assert!(rule.matches("LS", &json!({}), &cwd));
        assert!(!rule.matches("Write", &json!({}), &cwd));
    }

    #[test]
    fn test_matches_tool_group_edit() {
        let rule = ParsedRule::parse("Edit");
        let cwd = PathBuf::from("/tmp");

        // Edit rule should match Edit, Write
        assert!(rule.matches("Edit", &json!({}), &cwd));
        assert!(rule.matches("Write", &json!({}), &cwd));
        assert!(!rule.matches("Read", &json!({}), &cwd));
    }

    #[test]
    fn test_matches_bash_exact() {
        let rule = ParsedRule::parse("Bash(npm run lint)");
        let cwd = PathBuf::from("/tmp");

        assert!(rule.matches("Bash", &json!({"command": "npm run lint"}), &cwd));
        assert!(!rule.matches("Bash", &json!({"command": "npm run build"}), &cwd));
        assert!(!rule.matches("Bash", &json!({"command": "npm run lint --fix"}), &cwd));
    }

    #[test]
    fn test_matches_bash_wildcard() {
        let rule = ParsedRule::parse("Bash(npm run:*)");
        let cwd = PathBuf::from("/tmp");

        assert!(rule.matches("Bash", &json!({"command": "npm run"}), &cwd));
        assert!(rule.matches("Bash", &json!({"command": "npm run build"}), &cwd));
        assert!(rule.matches("Bash", &json!({"command": "npm run lint --fix"}), &cwd));
        assert!(!rule.matches("Bash", &json!({"command": "npm install"}), &cwd));
    }

    #[test]
    fn test_matches_bash_wildcard_blocks_shell_operators() {
        let rule = ParsedRule::parse("Bash(npm run:*)");
        let cwd = PathBuf::from("/tmp");

        // Should block commands with shell operators after prefix
        assert!(!rule.matches(
            "Bash",
            &json!({"command": "npm run build && rm -rf /"}),
            &cwd
        ));
        assert!(!rule.matches("Bash", &json!({"command": "npm run build | cat"}), &cwd));
        assert!(!rule.matches(
            "Bash",
            &json!({"command": "npm run build; malicious"}),
            &cwd
        ));
    }

    #[test]
    fn test_permission_check_result() {
        let allow = PermissionCheckResult::allow("Read");
        assert_eq!(allow.decision, PermissionDecision::Allow);
        assert_eq!(allow.rule, Some("Read".to_string()));
        assert_eq!(allow.source, Some("allow".to_string()));

        let deny = PermissionCheckResult::deny("Bash");
        assert_eq!(deny.decision, PermissionDecision::Deny);

        let ask = PermissionCheckResult::ask();
        assert_eq!(ask.decision, PermissionDecision::Ask);
        assert!(ask.rule.is_none());
    }

    #[test]
    fn test_mcp_tool_web_fetch_matching() {
        // Test that "WebFetch" rule matches "mcp__web-fetch__webReader"
        let rule = ParsedRule::parse("WebFetch");
        let cwd = PathBuf::from("/tmp");

        assert!(rule.matches("mcp__web-fetch__webReader", &json!({}), &cwd));
        assert!(rule.matches("mcp__web-reader__webReader", &json!({}), &cwd));
    }

    #[test]
    fn test_mcp_tool_web_search_matching() {
        // Test that "WebSearch" rule matches "mcp__web-search-prime__webSearchPrime"
        let rule = ParsedRule::parse("WebSearch");
        let cwd = PathBuf::from("/tmp");

        assert!(rule.matches("mcp__web-search-prime__webSearchPrime", &json!({}), &cwd));
    }

    #[test]
    fn test_mcp_tool_does_not_match_unrelated_tools() {
        // Test that "WebFetch" rule does NOT match Read or other built-in tools
        let rule = ParsedRule::parse("WebFetch");
        let cwd = PathBuf::from("/tmp");

        assert!(!rule.matches("Read", &json!({}), &cwd));
        assert!(!rule.matches("Bash", &json!({}), &cwd));
        assert!(!rule.matches("Write", &json!({}), &cwd));
    }

    #[test]
    fn test_deny_web_fetch_blocks_mcp_tool() {
        // Test that deny: ["WebFetch"] blocks the MCP tool
        let permissions = PermissionSettings {
            deny: Some(vec!["WebFetch".to_string()]),
            ..Default::default()
        };
        let checker = PermissionChecker::new(settings_with_permissions(permissions), "/tmp");

        let result = checker.check_permission("mcp__web-fetch__webReader", &json!({}));
        assert_eq!(result.decision, PermissionDecision::Deny);
        assert_eq!(result.rule, Some("WebFetch".to_string()));
    }

    #[test]
    fn test_deny_web_search_blocks_mcp_tool() {
        // Test that deny: ["WebSearch"] blocks the MCP tool
        let permissions = PermissionSettings {
            deny: Some(vec!["WebSearch".to_string()]),
            ..Default::default()
        };
        let checker = PermissionChecker::new(settings_with_permissions(permissions), "/tmp");

        let result = checker.check_permission("mcp__web-search-prime__webSearchPrime", &json!({}));
        assert_eq!(result.decision, PermissionDecision::Deny);
        assert_eq!(result.rule, Some("WebSearch".to_string()));
    }

    #[test]
    fn test_allow_web_fetch_allows_mcp_tool() {
        // Test that allow: ["WebFetch"] allows the MCP tool
        let permissions = PermissionSettings {
            allow: Some(vec!["WebFetch".to_string()]),
            ..Default::default()
        };
        let checker = PermissionChecker::new(settings_with_permissions(permissions), "/tmp");

        let result = checker.check_permission("mcp__web-fetch__webReader", &json!({}));
        assert_eq!(result.decision, PermissionDecision::Allow);
        assert_eq!(result.rule, Some("WebFetch".to_string()));
    }

    #[test]
    fn test_deny_web_fetch_blocks_builtin_tool() {
        // Test that deny: ["WebFetch"] also blocks the built-in WebFetch tool
        let permissions = PermissionSettings {
            deny: Some(vec!["WebFetch".to_string()]),
            ..Default::default()
        };
        let checker = PermissionChecker::new(settings_with_permissions(permissions), "/tmp");

        // Built-in tools use the "mcp__acp__" prefix
        let result = checker.check_permission("mcp__acp__WebFetch", &json!({}));
        assert_eq!(result.decision, PermissionDecision::Deny);
        assert_eq!(result.rule, Some("WebFetch".to_string()));

        // Also works without prefix (direct match)
        let result = checker.check_permission("WebFetch", &json!({}));
        assert_eq!(result.decision, PermissionDecision::Deny);
    }

    #[test]
    fn test_deny_web_search_blocks_builtin_tool() {
        // Test that deny: ["WebSearch"] also blocks the built-in WebSearch tool
        let permissions = PermissionSettings {
            deny: Some(vec!["WebSearch".to_string()]),
            ..Default::default()
        };
        let checker = PermissionChecker::new(settings_with_permissions(permissions), "/tmp");

        // Built-in tools use the "mcp__acp__" prefix
        let result = checker.check_permission("mcp__acp__WebSearch", &json!({}));
        assert_eq!(result.decision, PermissionDecision::Deny);
        assert_eq!(result.rule, Some("WebSearch".to_string()));

        // Also works without prefix (direct match)
        let result = checker.check_permission("WebSearch", &json!({}));
        assert_eq!(result.decision, PermissionDecision::Deny);
    }
}
