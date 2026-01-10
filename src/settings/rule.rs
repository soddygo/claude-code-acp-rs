//! Permission rule parsing and matching
//!
//! Implements rule parsing for allow/deny/ask permission rules with glob pattern support.

use std::path::Path;

use globset::{Glob, GlobMatcher};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::mcp::tools::bash::contains_shell_operator;

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
        // Regex pattern: ToolName or ToolName(argument)
        let re = Regex::new(r"^(\w+)(?:\((.+)\))?$").expect("Invalid regex");

        if let Some(caps) = re.captures(rule) {
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

        // Check if tool name matches (considering tool groups)
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

    /// Check if tool name matches (considering tool groups)
    fn matches_tool_name(&self, tool_name: &str) -> bool {
        // Direct match
        if self.tool_name == tool_name {
            return true;
        }

        // Tool group matching
        match self.tool_name.as_str() {
            // Read rule matches Read, Grep, Glob, LS
            "Read" => matches!(tool_name, "Read" | "Grep" | "Glob" | "LS"),
            // Edit rule matches Edit, Write
            "Edit" => matches!(tool_name, "Edit" | "Write"),
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
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

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
}
