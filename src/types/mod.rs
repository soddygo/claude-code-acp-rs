//! Public types for Claude Code ACP Agent
//!
//! This module contains all the shared types used across the crate.

mod config;
mod error;
mod meta;
mod session;
mod tool;

pub use config::AgentConfig;
pub use error::{AgentError, ErrorCode, Result};
pub use meta::{ClaudeCodeMeta, ClaudeCodeOptions, NewSessionMeta, SystemPromptMeta};
pub use session::{SessionStats, TokenUsage};
pub use tool::{ToolCallLocation, ToolInfo, ToolInfoContent, ToolKind, ToolUseEntry, ToolUseType};
