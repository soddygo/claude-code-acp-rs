//! PreToolUse hook implementation
//!
//! Checks permissions using SettingsManager before tool execution.

use std::sync::Arc;

use claude_code_agent_sdk::{
    HookCallback, HookContext, HookInput, HookJsonOutput, HookSpecificOutput,
    PreToolUseHookSpecificOutput, SyncHookJsonOutput,
};
use futures::future::BoxFuture;
use tokio::sync::RwLock;

use crate::settings::PermissionChecker;

/// Creates a PreToolUse hook that checks permissions using the PermissionChecker.
///
/// This hook runs before the SDK's built-in permission rules, allowing us to enforce
/// our own permission settings for ACP-prefixed tools.
///
/// # Arguments
///
/// * `permission_checker` - The permission checker to use for checking tool permissions
///
/// # Returns
///
/// A hook callback that can be used with ClaudeAgentOptions
pub fn create_pre_tool_use_hook(
    permission_checker: Arc<RwLock<PermissionChecker>>,
) -> HookCallback {
    Arc::new(move |input: HookInput, _tool_use_id: Option<String>, _context: HookContext| {
        let permission_checker = permission_checker.clone();

        Box::pin(async move {
            // Only handle PreToolUse events
            let (tool_name, tool_input) = match &input {
                HookInput::PreToolUse(pre_tool) => {
                    (pre_tool.tool_name.clone(), pre_tool.tool_input.clone())
                }
                _ => {
                    return HookJsonOutput::Sync(SyncHookJsonOutput {
                        continue_: Some(true),
                        ..Default::default()
                    });
                }
            };

            // Check permission
            let checker = permission_checker.read().await;
            let permission_check = checker.check_permission(&tool_name, &tool_input);

            tracing::debug!(
                "[PreToolUseHook] Tool: {}, Decision: {:?}, Rule: {:?}",
                tool_name,
                permission_check.decision,
                permission_check.rule
            );

            match permission_check.decision {
                crate::settings::PermissionDecision::Allow => {
                    let reason = format!(
                        "Allowed by settings rule: {}",
                        permission_check.rule.as_deref().unwrap_or("(implicit)")
                    );

                    HookJsonOutput::Sync(SyncHookJsonOutput {
                        continue_: Some(true),
                        hook_specific_output: Some(HookSpecificOutput::PreToolUse(
                            PreToolUseHookSpecificOutput {
                                permission_decision: Some("allow".to_string()),
                                permission_decision_reason: Some(reason),
                                updated_input: None,
                            },
                        )),
                        ..Default::default()
                    })
                }

                crate::settings::PermissionDecision::Deny => {
                    let reason = format!(
                        "Denied by settings rule: {}",
                        permission_check.rule.as_deref().unwrap_or("(implicit)")
                    );

                    HookJsonOutput::Sync(SyncHookJsonOutput {
                        continue_: Some(true),
                        hook_specific_output: Some(HookSpecificOutput::PreToolUse(
                            PreToolUseHookSpecificOutput {
                                permission_decision: Some("deny".to_string()),
                                permission_decision_reason: Some(reason),
                                updated_input: None,
                            },
                        )),
                        ..Default::default()
                    })
                }

                crate::settings::PermissionDecision::Ask => {
                    // Let the normal permission flow continue
                    HookJsonOutput::Sync(SyncHookJsonOutput {
                        continue_: Some(true),
                        ..Default::default()
                    })
                }
            }
        }) as BoxFuture<'static, HookJsonOutput>
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{PermissionSettings, Settings};
    use serde_json::json;

    fn make_permission_checker(permissions: PermissionSettings) -> Arc<RwLock<PermissionChecker>> {
        let settings = Settings {
            permissions: Some(permissions),
            ..Default::default()
        };
        Arc::new(RwLock::new(PermissionChecker::new(settings, "/tmp")))
    }

    #[tokio::test]
    async fn test_pre_tool_use_hook_allow() {
        let checker = make_permission_checker(PermissionSettings {
            allow: Some(vec!["Read".to_string()]),
            ..Default::default()
        });

        let hook = create_pre_tool_use_hook(checker);
        let input = HookInput::PreToolUse(claude_code_agent_sdk::PreToolUseHookInput {
            session_id: "test".to_string(),
            transcript_path: "/tmp/test".to_string(),
            cwd: "/tmp".to_string(),
            permission_mode: None,
            tool_name: "Read".to_string(),
            tool_input: json!({"file_path": "/tmp/test.txt"}),
        });

        let result = hook(input, None, HookContext::default()).await;

        match result {
            HookJsonOutput::Sync(output) => {
                assert_eq!(output.continue_, Some(true));
                if let Some(HookSpecificOutput::PreToolUse(specific)) = output.hook_specific_output {
                    assert_eq!(specific.permission_decision, Some("allow".to_string()));
                } else {
                    panic!("Expected PreToolUse specific output");
                }
            }
            HookJsonOutput::Async(_) => panic!("Expected sync output"),
        }
    }

    #[tokio::test]
    async fn test_pre_tool_use_hook_deny() {
        let checker = make_permission_checker(PermissionSettings {
            deny: Some(vec!["Bash".to_string()]),
            ..Default::default()
        });

        let hook = create_pre_tool_use_hook(checker);
        let input = HookInput::PreToolUse(claude_code_agent_sdk::PreToolUseHookInput {
            session_id: "test".to_string(),
            transcript_path: "/tmp/test".to_string(),
            cwd: "/tmp".to_string(),
            permission_mode: None,
            tool_name: "Bash".to_string(),
            tool_input: json!({"command": "ls"}),
        });

        let result = hook(input, None, HookContext::default()).await;

        match result {
            HookJsonOutput::Sync(output) => {
                assert_eq!(output.continue_, Some(true));
                if let Some(HookSpecificOutput::PreToolUse(specific)) = output.hook_specific_output {
                    assert_eq!(specific.permission_decision, Some("deny".to_string()));
                } else {
                    panic!("Expected PreToolUse specific output");
                }
            }
            HookJsonOutput::Async(_) => panic!("Expected sync output"),
        }
    }

    #[tokio::test]
    async fn test_pre_tool_use_hook_ask() {
        // No rules = ask
        let checker = make_permission_checker(PermissionSettings::default());

        let hook = create_pre_tool_use_hook(checker);
        let input = HookInput::PreToolUse(claude_code_agent_sdk::PreToolUseHookInput {
            session_id: "test".to_string(),
            transcript_path: "/tmp/test".to_string(),
            cwd: "/tmp".to_string(),
            permission_mode: None,
            tool_name: "Write".to_string(),
            tool_input: json!({"file_path": "/tmp/test.txt", "content": "test"}),
        });

        let result = hook(input, None, HookContext::default()).await;

        match result {
            HookJsonOutput::Sync(output) => {
                assert_eq!(output.continue_, Some(true));
                // No hook_specific_output for ask decision
                assert!(output.hook_specific_output.is_none());
            }
            HookJsonOutput::Async(_) => panic!("Expected sync output"),
        }
    }

    #[tokio::test]
    async fn test_pre_tool_use_hook_ignores_other_events() {
        let checker = make_permission_checker(PermissionSettings::default());

        let hook = create_pre_tool_use_hook(checker);
        let input = HookInput::PostToolUse(claude_code_agent_sdk::PostToolUseHookInput {
            session_id: "test".to_string(),
            transcript_path: "/tmp/test".to_string(),
            cwd: "/tmp".to_string(),
            permission_mode: None,
            tool_name: "Read".to_string(),
            tool_input: json!({}),
            tool_response: json!("content"),
        });

        let result = hook(input, None, HookContext::default()).await;

        match result {
            HookJsonOutput::Sync(output) => {
                assert_eq!(output.continue_, Some(true));
                assert!(output.hook_specific_output.is_none());
            }
            HookJsonOutput::Async(_) => panic!("Expected sync output"),
        }
    }
}
