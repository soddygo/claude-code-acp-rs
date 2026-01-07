//! PostToolUse hook implementation
//!
//! Executes registered callbacks after tool execution completes.

use std::sync::Arc;

use claude_code_agent_sdk::{
    HookCallback, HookContext, HookInput, HookJsonOutput, SyncHookJsonOutput,
};
use futures::future::BoxFuture;

use super::callback_registry::HookCallbackRegistry;

/// Creates a PostToolUse hook that executes registered callbacks.
///
/// This hook runs after tool execution and invokes any callbacks registered
/// for the tool use ID. Callbacks can be used to send updates to the ACP client.
///
/// # Arguments
///
/// * `callback_registry` - The callback registry to use for looking up callbacks
///
/// # Returns
///
/// A hook callback that can be used with ClaudeAgentOptions
pub fn create_post_tool_use_hook(callback_registry: Arc<HookCallbackRegistry>) -> HookCallback {
    Arc::new(
        move |input: HookInput, tool_use_id: Option<String>, _context: HookContext| {
            let callback_registry = callback_registry.clone();

            Box::pin(async move {
                // Only handle PostToolUse events
                let (tool_name, tool_input, tool_response) = match &input {
                    HookInput::PostToolUse(post_tool) => (
                        post_tool.tool_name.clone(),
                        post_tool.tool_input.clone(),
                        post_tool.tool_response.clone(),
                    ),
                    _ => {
                        return HookJsonOutput::Sync(SyncHookJsonOutput {
                            continue_: Some(true),
                            ..Default::default()
                        });
                    }
                };

                // Execute callback if registered
                if let Some(tool_use_id) = tool_use_id {
                    let executed = callback_registry
                        .execute_post_tool_use(&tool_use_id, tool_input.clone(), tool_response)
                        .await;

                    if executed {
                        tracing::debug!(
                            "[PostToolUseHook] Executed callback for tool: {}, ID: {}",
                            tool_name,
                            tool_use_id
                        );
                    } else {
                        tracing::debug!(
                            "[PostToolUseHook] No callback found for tool: {}, ID: {}",
                            tool_name,
                            tool_use_id
                        );
                    }
                } else {
                    tracing::debug!(
                        "[PostToolUseHook] No tool_use_id provided for tool: {}",
                        tool_name
                    );
                }

                HookJsonOutput::Sync(SyncHookJsonOutput {
                    continue_: Some(true),
                    ..Default::default()
                })
            }) as BoxFuture<'static, HookJsonOutput>
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test]
    async fn test_post_tool_use_hook_executes_callback() {
        let registry = Arc::new(HookCallbackRegistry::new());
        let was_called = Arc::new(AtomicBool::new(false));
        let was_called_clone = was_called.clone();

        registry.register_post_tool_use(
            "test-id".to_string(),
            Box::new(move |_id, _input, _response| {
                let was_called = was_called_clone.clone();
                async move {
                    was_called.store(true, Ordering::SeqCst);
                }
                .boxed()
            }),
        );

        let hook = create_post_tool_use_hook(registry);
        let input = HookInput::PostToolUse(claude_code_agent_sdk::PostToolUseHookInput {
            session_id: "test".to_string(),
            transcript_path: "/tmp/test".to_string(),
            cwd: "/tmp".to_string(),
            permission_mode: None,
            tool_name: "Bash".to_string(),
            tool_input: json!({"command": "ls"}),
            tool_response: json!("file1\nfile2"),
        });

        let result = hook(input, Some("test-id".to_string()), HookContext::default()).await;

        match result {
            HookJsonOutput::Sync(output) => {
                assert_eq!(output.continue_, Some(true));
            }
            HookJsonOutput::Async(_) => panic!("Expected sync output"),
        }

        assert!(was_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_post_tool_use_hook_no_callback() {
        let registry = Arc::new(HookCallbackRegistry::new());
        let hook = create_post_tool_use_hook(registry);

        let input = HookInput::PostToolUse(claude_code_agent_sdk::PostToolUseHookInput {
            session_id: "test".to_string(),
            transcript_path: "/tmp/test".to_string(),
            cwd: "/tmp".to_string(),
            permission_mode: None,
            tool_name: "Read".to_string(),
            tool_input: json!({"file_path": "/tmp/test.txt"}),
            tool_response: json!("content"),
        });

        let result = hook(
            input,
            Some("nonexistent-id".to_string()),
            HookContext::default(),
        )
        .await;

        match result {
            HookJsonOutput::Sync(output) => {
                assert_eq!(output.continue_, Some(true));
            }
            HookJsonOutput::Async(_) => panic!("Expected sync output"),
        }
    }

    #[tokio::test]
    async fn test_post_tool_use_hook_no_tool_use_id() {
        let registry = Arc::new(HookCallbackRegistry::new());
        let hook = create_post_tool_use_hook(registry);

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
            }
            HookJsonOutput::Async(_) => panic!("Expected sync output"),
        }
    }

    #[tokio::test]
    async fn test_post_tool_use_hook_ignores_other_events() {
        let registry = Arc::new(HookCallbackRegistry::new());
        let hook = create_post_tool_use_hook(registry);

        let input = HookInput::PreToolUse(claude_code_agent_sdk::PreToolUseHookInput {
            session_id: "test".to_string(),
            transcript_path: "/tmp/test".to_string(),
            cwd: "/tmp".to_string(),
            permission_mode: None,
            tool_name: "Read".to_string(),
            tool_input: json!({}),
        });

        let result = hook(input, Some("test-id".to_string()), HookContext::default()).await;

        match result {
            HookJsonOutput::Sync(output) => {
                assert_eq!(output.continue_, Some(true));
            }
            HookJsonOutput::Async(_) => panic!("Expected sync output"),
        }
    }
}
