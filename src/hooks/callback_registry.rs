//! Callback registry for tool use hooks
//!
//! Stores callbacks that are executed when receiving hooks from Claude Code.

use dashmap::DashMap;

/// Callback type for PostToolUse events
pub type PostToolUseCallback = Box<
    dyn Fn(String, serde_json::Value, serde_json::Value) -> futures::future::BoxFuture<'static, ()>
        + Send
        + Sync,
>;

/// Registry for tool use callbacks
///
/// Stores callbacks keyed by tool use ID that are called when
/// receiving PostToolUse hooks.
#[derive(Default)]
pub struct HookCallbackRegistry {
    /// Callbacks keyed by tool use ID
    callbacks: DashMap<String, ToolUseCallbacks>,
}

/// Callbacks for a specific tool use
struct ToolUseCallbacks {
    /// Callback for PostToolUse hook
    on_post_tool_use: Option<PostToolUseCallback>,
}

impl HookCallbackRegistry {
    /// Create a new empty callback registry
    pub fn new() -> Self {
        Self {
            callbacks: DashMap::new(),
        }
    }

    /// Register a PostToolUse callback for a specific tool use
    pub fn register_post_tool_use(&self, tool_use_id: String, callback: PostToolUseCallback) {
        self.callbacks.insert(
            tool_use_id,
            ToolUseCallbacks {
                on_post_tool_use: Some(callback),
            },
        );
    }

    /// Execute and remove the PostToolUse callback for a tool use
    ///
    /// Returns None if no callback was registered for this tool use ID.
    pub async fn execute_post_tool_use(
        &self,
        tool_use_id: &str,
        tool_input: serde_json::Value,
        tool_response: serde_json::Value,
    ) -> bool {
        if let Some((_, callbacks)) = self.callbacks.remove(tool_use_id) {
            if let Some(callback) = callbacks.on_post_tool_use {
                callback(tool_use_id.to_string(), tool_input, tool_response).await;
                return true;
            }
        }
        false
    }

    /// Check if a callback is registered for a tool use ID
    pub fn has_callback(&self, tool_use_id: &str) -> bool {
        self.callbacks.contains_key(tool_use_id)
    }

    /// Remove a callback without executing it
    pub fn remove(&self, tool_use_id: &str) {
        self.callbacks.remove(tool_use_id);
    }

    /// Get the number of registered callbacks
    pub fn len(&self) -> usize {
        self.callbacks.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.callbacks.is_empty()
    }
}

impl std::fmt::Debug for HookCallbackRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookCallbackRegistry")
            .field("count", &self.callbacks.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test]
    async fn test_register_and_execute() {
        let registry = HookCallbackRegistry::new();
        let was_called = Arc::new(AtomicBool::new(false));
        let was_called_clone = was_called.clone();

        let callback: PostToolUseCallback = Box::new(move |_id, _input, _response| {
            let was_called = was_called_clone.clone();
            async move {
                was_called.store(true, Ordering::SeqCst);
            }
            .boxed()
        });

        registry.register_post_tool_use("test-id".to_string(), callback);
        assert!(registry.has_callback("test-id"));
        assert_eq!(registry.len(), 1);

        let result = registry
            .execute_post_tool_use(
                "test-id",
                serde_json::json!({"command": "ls"}),
                serde_json::json!("output"),
            )
            .await;

        assert!(result);
        assert!(was_called.load(Ordering::SeqCst));
        assert!(!registry.has_callback("test-id"));
        assert!(registry.is_empty());
    }

    #[tokio::test]
    async fn test_execute_nonexistent() {
        let registry = HookCallbackRegistry::new();
        let result = registry
            .execute_post_tool_use("nonexistent", serde_json::json!({}), serde_json::json!({}))
            .await;

        assert!(!result);
    }

    #[test]
    fn test_remove() {
        let registry = HookCallbackRegistry::new();
        let callback: PostToolUseCallback = Box::new(|_id, _input, _response| async {}.boxed());

        registry.register_post_tool_use("test-id".to_string(), callback);
        assert!(registry.has_callback("test-id"));

        registry.remove("test-id");
        assert!(!registry.has_callback("test-id"));
    }
}
