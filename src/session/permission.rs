//! Permission handling for tool execution
//!
//! Phase 1: Simplified permission handling with auto-approve mode.
//! Phase 2: Full permission prompts with settings rules and user interaction.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::settings::{PermissionChecker, PermissionDecision};
use claude_code_agent_sdk::PermissionMode as SdkPermissionMode;

/// Permission mode for tool execution
///
/// Controls how tool calls are approved during a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    /// Default mode - prompt for dangerous operations
    #[default]
    Default,
    /// Auto-approve file edits
    AcceptEdits,
    /// Planning mode - read-only operations
    Plan,
    /// Don't ask mode - deny if not pre-approved
    DontAsk,
    /// Bypass all permission checks (dangerous)
    BypassPermissions,
}

impl PermissionMode {
    /// Parse from string (ACP setMode request)
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "default" => Some(Self::Default),
            "acceptEdits" => Some(Self::AcceptEdits),
            "plan" => Some(Self::Plan),
            "dontAsk" => Some(Self::DontAsk),
            "bypassPermissions" => Some(Self::BypassPermissions),
            _ => None,
        }
    }

    /// Convert to string for SDK
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::AcceptEdits => "acceptEdits",
            Self::Plan => "plan",
            Self::DontAsk => "dontAsk",
            Self::BypassPermissions => "bypassPermissions",
        }
    }

    /// Convert to SDK PermissionMode
    ///
    /// Note: SDK doesn't support DontAsk mode yet, so we map it to Default
    pub fn to_sdk_mode(&self) -> SdkPermissionMode {
        match self {
            PermissionMode::Default => SdkPermissionMode::Default,
            PermissionMode::AcceptEdits => SdkPermissionMode::AcceptEdits,
            PermissionMode::Plan => SdkPermissionMode::Plan,
            PermissionMode::DontAsk => {
                // SDK doesn't support DontAsk yet, treat as Default
                SdkPermissionMode::Default
            }
            PermissionMode::BypassPermissions => SdkPermissionMode::BypassPermissions,
        }
    }

    /// Check if this mode allows write operations
    pub fn allows_writes(&self) -> bool {
        matches!(
            self,
            Self::Default | Self::AcceptEdits | Self::BypassPermissions
        )
    }

    /// Check if this mode auto-approves edits
    pub fn auto_approve_edits(&self) -> bool {
        matches!(self, Self::AcceptEdits | Self::BypassPermissions)
    }
}

/// Permission check result from the handler
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolPermissionResult {
    /// Tool execution is allowed (auto-approved or by rule)
    Allowed,
    /// Tool execution is blocked (by rule or mode)
    Blocked { reason: String },
    /// User should be asked for permission
    NeedsPermission,
}

/// Permission handler for tool execution
///
/// Combines mode-based checking with settings rules.
///
/// The permission checker is shared with the pre_tool_use_hook to ensure
/// that runtime rule changes (e.g., "Always Allow") are reflected in both places.
#[derive(Debug)]
pub struct PermissionHandler {
    mode: PermissionMode,
    /// Shared permission checker from settings (shared with hook)
    checker: Option<Arc<RwLock<PermissionChecker>>>,
}

impl Default for PermissionHandler {
    fn default() -> Self {
        Self {
            mode: PermissionMode::Default,
            checker: None,
        }
    }
}

impl PermissionHandler {
    /// Create a new permission handler
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with a specific mode
    pub fn with_mode(mode: PermissionMode) -> Self {
        Self {
            mode,
            checker: None,
        }
    }

    /// Create with settings-based checker
    pub fn with_checker(checker: Arc<RwLock<PermissionChecker>>) -> Self {
        Self {
            mode: PermissionMode::Default,
            checker: Some(checker),
        }
    }

    /// Create with settings-based checker (non-async, for convenience)
    pub fn with_checker_owned(checker: PermissionChecker) -> Self {
        Self {
            mode: PermissionMode::Default,
            checker: Some(Arc::new(RwLock::new(checker))),
        }
    }

    /// Get current permission mode
    pub fn mode(&self) -> PermissionMode {
        self.mode
    }

    /// Set permission mode
    pub fn set_mode(&mut self, mode: PermissionMode) {
        self.mode = mode;
    }

    /// Set the permission checker
    pub fn set_checker(&mut self, checker: Arc<RwLock<PermissionChecker>>) {
        self.checker = Some(checker);
    }

    /// Get mutable reference to checker (for adding runtime rules)
    pub async fn checker_mut(
        &mut self,
    ) -> Option<tokio::sync::RwLockWriteGuard<'_, PermissionChecker>> {
        if let Some(ref checker) = self.checker {
            Some(checker.write().await)
        } else {
            None
        }
    }

    /// Check if a tool operation should be auto-approved
    ///
    /// Returns true if the operation should proceed without user prompt.
    pub fn should_auto_approve(&self, tool_name: &str, _input: &serde_json::Value) -> bool {
        match self.mode {
            PermissionMode::BypassPermissions => true,
            PermissionMode::AcceptEdits => {
                // Auto-approve read and edit operations
                matches!(
                    tool_name,
                    "Read" | "Edit" | "Write" | "Glob" | "Grep" | "NotebookRead" | "NotebookEdit"
                )
            }
            PermissionMode::Plan => {
                // Only allow read operations in plan mode
                matches!(tool_name, "Read" | "Glob" | "Grep" | "NotebookRead")
            }
            PermissionMode::DontAsk => {
                // DontAsk mode: only pre-approved tools via settings rules
                // No auto-approval
                false
            }
            PermissionMode::Default => {
                // Only auto-approve read operations
                matches!(tool_name, "Read" | "Glob" | "Grep" | "NotebookRead")
            }
        }
    }

    /// Check if a tool is blocked in current mode
    pub fn is_tool_blocked(&self, tool_name: &str) -> bool {
        if self.mode == PermissionMode::Plan {
            // Block write operations in plan mode
            matches!(tool_name, "Edit" | "Write" | "Bash" | "NotebookEdit")
        } else {
            false
        }
    }

    /// Check permission for a tool with full context
    ///
    /// Combines mode-based checking with settings rules.
    /// Returns the permission result.
    pub async fn check_permission(
        &self,
        tool_name: &str,
        tool_input: &serde_json::Value,
    ) -> ToolPermissionResult {
        // BypassPermissions mode allows everything
        if self.mode == PermissionMode::BypassPermissions {
            return ToolPermissionResult::Allowed;
        }

        // Check if tool is blocked in current mode
        if self.is_tool_blocked(tool_name) {
            return ToolPermissionResult::Blocked {
                reason: format!(
                    "Tool {} is blocked in {} mode",
                    tool_name,
                    self.mode.as_str()
                ),
            };
        }

        // Check settings rules if available
        if let Some(ref checker) = self.checker {
            let checker_read = checker.read().await;
            let result = checker_read.check_permission(tool_name, tool_input);
            match result.decision {
                PermissionDecision::Deny => {
                    return ToolPermissionResult::Blocked {
                        reason: result
                            .rule
                            .map(|r| format!("Denied by rule: {}", r))
                            .unwrap_or_else(|| "Denied by settings".to_string()),
                    };
                }
                PermissionDecision::Allow => {
                    return ToolPermissionResult::Allowed;
                }
                PermissionDecision::Ask => {
                    // Fall through to mode-based check
                }
            }
        }

        // Mode-based auto-approve
        if self.should_auto_approve(tool_name, tool_input) {
            return ToolPermissionResult::Allowed;
        }

        // Default: need to ask user
        ToolPermissionResult::NeedsPermission
    }

    /// Add a runtime allow rule (e.g., from user's "Always Allow" choice)
    pub async fn add_allow_rule(&self, tool_name: &str) {
        if let Some(ref checker) = self.checker {
            let mut checker_write = checker.write().await;
            checker_write.add_allow_rule(tool_name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_permission_mode_parse() {
        assert_eq!(
            PermissionMode::parse("default"),
            Some(PermissionMode::Default)
        );
        assert_eq!(
            PermissionMode::parse("acceptEdits"),
            Some(PermissionMode::AcceptEdits)
        );
        assert_eq!(PermissionMode::parse("plan"), Some(PermissionMode::Plan));
        assert_eq!(
            PermissionMode::parse("bypassPermissions"),
            Some(PermissionMode::BypassPermissions)
        );
        assert_eq!(PermissionMode::parse("invalid"), None);
    }

    #[test]
    fn test_permission_mode_str() {
        assert_eq!(PermissionMode::Default.as_str(), "default");
        assert_eq!(PermissionMode::AcceptEdits.as_str(), "acceptEdits");
    }

    #[test]
    fn test_permission_handler_default() {
        let handler = PermissionHandler::new();
        let input = json!({});

        // Default mode auto-approves reads
        assert!(handler.should_auto_approve("Read", &input));
        assert!(handler.should_auto_approve("Glob", &input));
        // But not writes
        assert!(!handler.should_auto_approve("Edit", &input));
        assert!(!handler.should_auto_approve("Bash", &input));
    }

    #[test]
    fn test_permission_handler_accept_edits() {
        let handler = PermissionHandler::with_mode(PermissionMode::AcceptEdits);
        let input = json!({});

        assert!(handler.should_auto_approve("Read", &input));
        assert!(handler.should_auto_approve("Edit", &input));
        assert!(handler.should_auto_approve("Write", &input));
        // Bash still not auto-approved
        assert!(!handler.should_auto_approve("Bash", &input));
    }

    #[test]
    fn test_permission_handler_bypass() {
        let handler = PermissionHandler::with_mode(PermissionMode::BypassPermissions);
        let input = json!({});

        // Everything auto-approved
        assert!(handler.should_auto_approve("Read", &input));
        assert!(handler.should_auto_approve("Edit", &input));
        assert!(handler.should_auto_approve("Bash", &input));
    }

    #[test]
    fn test_permission_handler_plan_mode() {
        let handler = PermissionHandler::with_mode(PermissionMode::Plan);
        let input = json!({});

        // Only reads auto-approved
        assert!(handler.should_auto_approve("Read", &input));
        assert!(!handler.should_auto_approve("Edit", &input));

        // Writes are blocked
        assert!(handler.is_tool_blocked("Edit"));
        assert!(handler.is_tool_blocked("Bash"));
        assert!(!handler.is_tool_blocked("Read"));
    }
}
