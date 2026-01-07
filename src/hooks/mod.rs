//! Hooks system for tool execution lifecycle
//!
//! Provides PreToolUse and PostToolUse hooks that integrate with
//! permission checking and ACP client notifications.

mod callback_registry;
mod post_tool_use;
mod pre_tool_use;

pub use callback_registry::{HookCallbackRegistry, PostToolUseCallback};
pub use post_tool_use::create_post_tool_use_hook;
pub use pre_tool_use::create_pre_tool_use_hook;
