//! Settings management
//!
//! Loads and merges settings from multiple sources:
//! - User settings: `~/.claude/settings.json`
//! - Project settings: `.claude/settings.json`
//! - Local settings: `.claude/settings.local.json`
//!
//! Priority: Local > Project > User

mod manager;
mod permission_checker;
mod rule;
mod watcher;

pub use manager::{McpServerConfig, Settings, SettingsManager};
pub use permission_checker::PermissionChecker;
pub use rule::{ParsedRule, PermissionCheckResult, PermissionDecision, PermissionSettings};
pub use watcher::{SettingsChangeEvent, SettingsWatcher, WatcherError, WatcherHandle};
