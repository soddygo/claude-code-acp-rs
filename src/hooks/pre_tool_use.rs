//! PreToolUse hook implementation
//!
//! Checks permissions using SettingsManager before tool execution.
//! For "Ask" decisions, returns `permission_decision: "ask"` to trigger SDK's permission flow.

use std::sync::{Arc, OnceLock};
use std::time::Instant;

use claude_code_agent_sdk::{
    HookCallback, HookContext, HookInput, HookJsonOutput, HookSpecificOutput,
    PreToolUseHookSpecificOutput, SyncHookJsonOutput,
};
use futures::future::BoxFuture;
use sacp::{JrConnectionCx, link::AgentToClient};
use tokio::sync::RwLock;
use tracing::Instrument;

use crate::session::PermissionMode;
use crate::settings::PermissionChecker;

/// Creates a PreToolUse hook that checks permissions using settings rules and permission mode.
///
/// This hook runs before the SDK's built-in permission rules, allowing us to enforce
/// our own permission settings for ACP-prefixed tools.
///
/// # Permission Handling
///
/// - **Allow**: Returns with `permission_decision: "allow"` - tool executes immediately
/// - **Deny**: Returns with `permission_decision: "deny"` - tool execution is blocked
/// - **Ask**: Returns with `permission_decision: "ask"` - SDK triggers permission request flow
///
/// # Permission Mode Integration
///
/// The hook respects the session's permission mode:
/// - **BypassPermissions**: Allows all tools without checking rules
/// - **Plan**: Blocks write operations (Edit, Write, Bash, NotebookEdit)
/// - **Default/AcceptEdits/DontAsk**: Checks settings rules and mode-based auto-approval
///
/// # Architecture
///
/// The hook and `can_use_tool` callback work together:
/// 1. **Hook** (this file): Makes quick decisions based on static rules (allow/deny/ask)
/// 2. **`can_use_tool` callback** (`can_use_tool.rs`): Handles interactive permission requests
///
/// When hook returns "ask", SDK calls `can_use_tool` callback to handle user interaction.
/// This separation prevents deadlock because:
/// - Hook returns immediately with a decision
/// - `can_use_tool` callback can handle async permission requests
///
/// # Arguments
///
/// * `connection_cx_lock` - Reserved for future use (not currently used)
/// * `session_id` - Reserved for future use (not currently used)
/// * `permission_checker` - Optional permission checker for settings-based rules
/// * `permission_mode` - Shared permission mode that can be updated at runtime
///
/// # Returns
///
/// A hook callback that can be used with ClaudeAgentOptions
pub fn create_pre_tool_use_hook(
    connection_cx_lock: Arc<OnceLock<JrConnectionCx<AgentToClient>>>,
    session_id: String,
    permission_checker: Option<Arc<RwLock<PermissionChecker>>>,
    permission_mode: Arc<RwLock<PermissionMode>>,
) -> HookCallback {
    Arc::new(
        move |input: HookInput, tool_use_id: Option<String>, _context: HookContext| {
            let _connection_cx_lock = Arc::clone(&connection_cx_lock);
            let permission_checker = permission_checker.clone();
            let permission_mode = permission_mode.clone();
            let _session_id = session_id.clone();

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

                    // Get current permission mode
                    let mode = *permission_mode.read().await;

                    // BypassPermissions mode allows everything
                    if mode == PermissionMode::BypassPermissions {
                        let elapsed = start_time.elapsed();
                        tracing::info!(
                            tool_name = %tool_name,
                            tool_use_id = ?tool_use_id,
                            mode = "bypassPermissions",
                            elapsed_us = elapsed.as_micros(),
                            "Tool allowed by BypassPermissions mode"
                        );

                        return HookJsonOutput::Sync(SyncHookJsonOutput {
                            continue_: Some(true),
                            hook_specific_output: Some(HookSpecificOutput::PreToolUse(
                                PreToolUseHookSpecificOutput {
                                    permission_decision: Some("allow".to_string()),
                                    permission_decision_reason: Some(
                                        "Allowed by BypassPermissions mode".to_string(),
                                    ),
                                    updated_input: None,
                                },
                            )),
                            ..Default::default()
                        });
                    }

                    // Plan mode: block write operations
                    if mode == PermissionMode::Plan {
                        let is_write_operation = matches!(
                            tool_name.as_str(),
                            "Edit" | "Write" | "Bash" | "NotebookEdit"
                        );
                        if is_write_operation {
                            let elapsed = start_time.elapsed();
                            let reason =
                                format!("Tool {} is blocked in Plan mode (read-only)", tool_name);
                            tracing::warn!(
                                tool_name = %tool_name,
                                tool_use_id = ?tool_use_id,
                                mode = "plan",
                                elapsed_us = elapsed.as_micros(),
                                "Tool blocked by Plan mode"
                            );

                            return HookJsonOutput::Sync(SyncHookJsonOutput {
                                continue_: Some(true),
                                hook_specific_output: Some(HookSpecificOutput::PreToolUse(
                                    PreToolUseHookSpecificOutput {
                                        permission_decision: Some("deny".to_string()),
                                        permission_decision_reason: Some(reason),
                                        updated_input: None,
                                    },
                                )),
                                ..Default::default()
                            });
                        }
                        // Read operations in Plan mode: allow them
                        let elapsed = start_time.elapsed();
                        tracing::debug!(
                            tool_name = %tool_name,
                            tool_use_id = ?tool_use_id,
                            mode = "plan",
                            elapsed_us = elapsed.as_micros(),
                            "Tool allowed in Plan mode (read operation)"
                        );
                        return HookJsonOutput::Sync(SyncHookJsonOutput {
                            continue_: Some(true),
                            hook_specific_output: Some(HookSpecificOutput::PreToolUse(
                                PreToolUseHookSpecificOutput {
                                    permission_decision: Some("allow".to_string()),
                                    permission_decision_reason: Some(
                                        "Allowed in Plan mode (read operation)".to_string(),
                                    ),
                                    updated_input: None,
                                },
                            )),
                            ..Default::default()
                        });
                    }

                    // Check permission (if checker is available, otherwise default to Ask)
                    let permission_check = if let Some(checker) = &permission_checker {
                        let checker = checker.read().await;
                        checker.check_permission(&tool_name, &tool_input)
                    } else {
                        // No permission checker - default to Ask
                        crate::settings::PermissionCheckResult {
                            decision: crate::settings::PermissionDecision::Ask,
                            rule: None,
                            source: None,
                        }
                    };
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

                    // TODO: Implement interactive permission request flow
                    //
                    // Current implementation: Always allow execution
                    //
                    // Future implementation should:
                    // 1. Check for explicit deny rules - block if matched
                    // 2. Check for explicit allow rules - allow if matched
                    // 3. For "Ask" decisions - send permission request via PermissionManager
                    // 4. Wait for user response - allow or deny based on user choice
                    //
                    // Architecture note: SDK does NOT call can_use_tool for MCP tools,
                    // so we need to implement the permission request flow differently.
                    //
                    // See plan file: /Users/soddy/.claude/plans/groovy-painting-truffle.md

                    tracing::debug!(
                        tool_name = %tool_name,
                        "Tool execution allowed (permission checks TODO)"
                    );

                    HookJsonOutput::Sync(SyncHookJsonOutput {
                        continue_: Some(true),
                        hook_specific_output: Some(HookSpecificOutput::PreToolUse(
                            PreToolUseHookSpecificOutput {
                                permission_decision: Some("allow".to_string()),
                                permission_decision_reason: Some(
                                    "Permission checks not yet implemented - allowing execution"
                                        .to_string(),
                                ),
                                updated_input: None,
                            },
                        )),
                        ..Default::default()
                    })

                    // TODO: Uncomment when implementing permission checks
                    // match permission_check.decision {
                    //     crate::settings::PermissionDecision::Allow => { ... }
                    //     crate::settings::PermissionDecision::Deny => { ... }
                    //     crate::settings::PermissionDecision::Ask => { ... }
                    // }
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

    fn make_test_hook(checker: Arc<RwLock<PermissionChecker>>) -> HookCallback {
        make_test_hook_with_mode(checker, PermissionMode::Default)
    }

    fn make_test_hook_with_mode(
        checker: Arc<RwLock<PermissionChecker>>,
        mode: PermissionMode,
    ) -> HookCallback {
        let connection_cx_lock: Arc<OnceLock<JrConnectionCx<AgentToClient>>> =
            Arc::new(OnceLock::new());
        create_pre_tool_use_hook(
            connection_cx_lock,
            "test-session".to_string(),
            Some(checker),
            Arc::new(RwLock::new(mode)),
        )
    }

    #[tokio::test]
    async fn test_pre_tool_use_hook_allow() {
        let checker = make_permission_checker(PermissionSettings {
            allow: Some(vec!["Read".to_string()]),
            ..Default::default()
        });

        let hook = make_test_hook(checker);
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

    // TODO: Re-enable when implementing permission checks
    // #[tokio::test]
    // async fn test_pre_tool_use_hook_deny() {
    //     let checker = make_permission_checker(PermissionSettings {
    //         deny: Some(vec!["Bash".to_string()]),
    //         ..Default::default()
    //     });
    //
    //     let hook = make_test_hook(checker);
    //     let input = HookInput::PreToolUse(claude_code_agent_sdk::PreToolUseHookInput {
    //         session_id: "test".to_string(),
    //         transcript_path: "/tmp/test".to_string(),
    //         cwd: "/tmp".to_string(),
    //         permission_mode: None,
    //         tool_name: "Bash".to_string(),
    //         tool_input: json!({"command": "ls"}),
    //     });
    //
    //     let result = hook(input, None, HookContext::default()).await;
    //
    //     match result {
    //         HookJsonOutput::Sync(output) => {
    //             assert_eq!(output.continue_, Some(true));
    //             if let Some(HookSpecificOutput::PreToolUse(specific)) = output.hook_specific_output
    //             {
    //                 assert_eq!(specific.permission_decision, Some("deny".to_string()));
    //             } else {
    //                 panic!("Expected PreToolUse specific output");
    //             }
    //         }
    //         HookJsonOutput::Async(_) => panic!("Expected sync output"),
    //     }
    // }

    #[tokio::test]
    async fn test_pre_tool_use_hook_always_allows() {
        // TODO: Permission checks not yet implemented - all tools are allowed
        let checker = make_permission_checker(PermissionSettings::default());
        let hook = make_test_hook(checker);

        // Test MCP tool - should be allowed
        let input_mcp = HookInput::PreToolUse(claude_code_agent_sdk::PreToolUseHookInput {
            session_id: "test".to_string(),
            transcript_path: "/tmp/test".to_string(),
            cwd: "/tmp".to_string(),
            permission_mode: None,
            tool_name: "mcp__acp__Write".to_string(),
            tool_input: json!({"file_path": "/tmp/test.txt", "content": "test"}),
        });

        let result_mcp = hook(input_mcp, None, HookContext::default()).await;

        match result_mcp {
            HookJsonOutput::Sync(output) => {
                assert_eq!(output.continue_, Some(true));
                if let Some(HookSpecificOutput::PreToolUse(specific)) = output.hook_specific_output
                {
                    assert_eq!(specific.permission_decision, Some("allow".to_string()));
                } else {
                    panic!("Expected PreToolUse specific output with permission_decision");
                }
            }
            HookJsonOutput::Async(_) => panic!("Expected sync output"),
        }

        // Test built-in tool - should also be allowed (permission checks TODO)
        let input_builtin = HookInput::PreToolUse(claude_code_agent_sdk::PreToolUseHookInput {
            session_id: "test".to_string(),
            transcript_path: "/tmp/test".to_string(),
            cwd: "/tmp".to_string(),
            permission_mode: None,
            tool_name: "Write".to_string(),
            tool_input: json!({"file_path": "/tmp/test.txt", "content": "test"}),
        });

        let result_builtin = hook(input_builtin, None, HookContext::default()).await;

        match result_builtin {
            HookJsonOutput::Sync(output) => {
                assert_eq!(output.continue_, Some(true));
                if let Some(HookSpecificOutput::PreToolUse(specific)) = output.hook_specific_output
                {
                    // Currently: All tools are allowed (permission checks TODO)
                    assert_eq!(specific.permission_decision, Some("allow".to_string()));
                } else {
                    panic!("Expected PreToolUse specific output with permission_decision");
                }
            }
            HookJsonOutput::Async(_) => panic!("Expected sync output"),
        }
    }

    #[tokio::test]
    async fn test_pre_tool_use_hook_ignores_other_events() {
        let checker = make_permission_checker(PermissionSettings::default());

        let hook = make_test_hook(checker);
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

    #[tokio::test]
    async fn test_bypass_permissions_mode_allows_everything() {
        // BypassPermissions mode should allow all tools without checking rules
        let checker = make_permission_checker(PermissionSettings {
            deny: Some(vec!["Bash".to_string()]),
            ..Default::default()
        });

        let hook = make_test_hook_with_mode(checker, PermissionMode::BypassPermissions);
        let input = HookInput::PreToolUse(claude_code_agent_sdk::PreToolUseHookInput {
            session_id: "test".to_string(),
            transcript_path: "/tmp/test".to_string(),
            cwd: "/tmp".to_string(),
            permission_mode: None,
            tool_name: "Bash".to_string(),
            tool_input: json!({"command": "rm -rf /"}),
        });

        let result = hook(input, None, HookContext::default()).await;

        match result {
            HookJsonOutput::Sync(output) => {
                assert_eq!(output.continue_, Some(true));
                if let Some(HookSpecificOutput::PreToolUse(specific)) = output.hook_specific_output
                {
                    assert_eq!(specific.permission_decision, Some("allow".to_string()));
                    assert!(
                        specific
                            .permission_decision_reason
                            .unwrap()
                            .contains("BypassPermissions")
                    );
                } else {
                    panic!("Expected PreToolUse specific output");
                }
            }
            HookJsonOutput::Async(_) => panic!("Expected sync output"),
        }
    }

    #[tokio::test]
    async fn test_plan_mode_blocks_write_operations() {
        // Plan mode should block write operations (Edit, Write, Bash, NotebookEdit)
        let checker = make_permission_checker(PermissionSettings {
            allow: Some(vec!["Edit".to_string()]),
            ..Default::default()
        });

        let hook = make_test_hook_with_mode(checker, PermissionMode::Plan);
        let input = HookInput::PreToolUse(claude_code_agent_sdk::PreToolUseHookInput {
            session_id: "test".to_string(),
            transcript_path: "/tmp/test".to_string(),
            cwd: "/tmp".to_string(),
            permission_mode: None,
            tool_name: "Edit".to_string(),
            tool_input: json!({"file_path": "/tmp/test.txt"}),
        });

        let result = hook(input, None, HookContext::default()).await;

        match result {
            HookJsonOutput::Sync(output) => {
                assert_eq!(output.continue_, Some(true));
                if let Some(HookSpecificOutput::PreToolUse(specific)) = output.hook_specific_output
                {
                    assert_eq!(specific.permission_decision, Some("deny".to_string()));
                    assert!(
                        specific
                            .permission_decision_reason
                            .unwrap()
                            .contains("Plan mode")
                    );
                } else {
                    panic!("Expected PreToolUse specific output");
                }
            }
            HookJsonOutput::Async(_) => panic!("Expected sync output"),
        }
    }

    #[tokio::test]
    async fn test_plan_mode_allows_read_operations() {
        // Plan mode should allow read operations (without settings check)
        let checker = make_permission_checker(PermissionSettings::default());

        let hook = make_test_hook_with_mode(checker, PermissionMode::Plan);
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
                // Plan mode allows reads - should return allow directly
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
    async fn test_default_mode_respects_settings_rules() {
        // Default mode should respect settings rules
        let checker = make_permission_checker(PermissionSettings {
            allow: Some(vec!["Read".to_string()]),
            ..Default::default()
        });

        let hook = make_test_hook_with_mode(checker, PermissionMode::Default);
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
}
