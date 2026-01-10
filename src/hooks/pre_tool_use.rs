//! PreToolUse hook implementation
//!
//! Checks permissions using SettingsManager before tool execution.

use std::sync::Arc;
use std::time::Instant;

use claude_code_agent_sdk::{
    HookCallback, HookContext, HookInput, HookJsonOutput, HookSpecificOutput,
    PreToolUseHookSpecificOutput, SyncHookJsonOutput,
};
use futures::future::BoxFuture;
use tokio::sync::RwLock;
use tracing::Instrument;

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
    Arc::new(
        move |input: HookInput, tool_use_id: Option<String>, _context: HookContext| {
            let permission_checker = permission_checker.clone();

            // Extract tool name early for span naming
            let (tool_name, is_pre_tool) = match &input {
                HookInput::PreToolUse(pre_tool) => (pre_tool.tool_name.clone(), true),
                _ => ("".to_string(), false),
            };

            // Create a span for this hook execution
            let span = if is_pre_tool {
                tracing::info_span!(
                    "pre_tool_use_hook",
                    tool_name = %tool_name,
                    tool_use_id = ?tool_use_id,
                    permission_decision = tracing::field::Empty,
                    permission_rule = tracing::field::Empty,
                    check_duration_us = tracing::field::Empty,
                )
            } else {
                tracing::debug_span!(
                    "pre_tool_use_hook_skip",
                    event_type = ?std::mem::discriminant(&input)
                )
            };

            Box::pin(
                async move {
                    let start_time = Instant::now();

                    // Only handle PreToolUse events
                    let (tool_name, tool_input) = match &input {
                        HookInput::PreToolUse(pre_tool) => {
                            (pre_tool.tool_name.clone(), pre_tool.tool_input.clone())
                        }
                        _ => {
                            tracing::debug!("Ignoring non-PreToolUse event");
                            return HookJsonOutput::Sync(SyncHookJsonOutput {
                                continue_: Some(true),
                                ..Default::default()
                            });
                        }
                    };

                    tracing::debug!(
                        tool_name = %tool_name,
                        tool_use_id = ?tool_use_id,
                        "PreToolUse hook triggered"
                    );

                    // Check permission
                    let checker = permission_checker.read().await;
                    let permission_check = checker.check_permission(&tool_name, &tool_input);
                    let elapsed = start_time.elapsed();

                    // Record permission decision to span (batched for performance)
                    let span = tracing::Span::current();
                    span.record(
                        "permission_decision",
                        format!("{:?}", permission_check.decision),
                    );
                    span.record("check_duration_us", elapsed.as_micros());
                    if let Some(ref rule) = permission_check.rule {
                        span.record("permission_rule", rule.as_str());
                    }

                    tracing::info!(
                        tool_name = %tool_name,
                        tool_use_id = ?tool_use_id,
                        decision = ?permission_check.decision,
                        rule = ?permission_check.rule,
                        elapsed_us = elapsed.as_micros(),
                        "Permission check completed"
                    );

                    match permission_check.decision {
                        crate::settings::PermissionDecision::Allow => {
                            let reason = format!(
                                "Allowed by settings rule: {}",
                                permission_check.rule.as_deref().unwrap_or("(implicit)")
                            );

                            tracing::debug!(
                                tool_name = %tool_name,
                                reason = %reason,
                                "Tool execution allowed"
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

                            tracing::warn!(
                                tool_name = %tool_name,
                                reason = %reason,
                                rule = ?permission_check.rule,
                                "Tool execution denied"
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
                            tracing::debug!(
                                tool_name = %tool_name,
                                "Tool requires permission prompt"
                            );

                            // Let the normal permission flow continue
                            HookJsonOutput::Sync(SyncHookJsonOutput {
                                continue_: Some(true),
                                ..Default::default()
                            })
                        }
                    }
                }
                .instrument(span),
            ) as BoxFuture<'static, HookJsonOutput>
        },
    )
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
                if let Some(HookSpecificOutput::PreToolUse(specific)) = output.hook_specific_output
                {
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
                if let Some(HookSpecificOutput::PreToolUse(specific)) = output.hook_specific_output
                {
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
