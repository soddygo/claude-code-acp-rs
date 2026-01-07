//! Message conversion utilities for ACP ↔ Claude SDK
//!
//! This module handles conversion between:
//! - ACP `PromptRequest` → Claude SDK `UserContentBlock`
//! - Claude SDK `Message` → ACP `SessionNotification`

mod notification;
mod prompt;
mod tool;

pub use notification::NotificationConverter;
pub use prompt::PromptConverter;
pub use tool::extract_tool_info;
