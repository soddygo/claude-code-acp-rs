//! Session management for ACP Agent
//!
//! This module handles:
//! - Session lifecycle (create, get, remove)
//! - Token usage tracking
//! - Permission handling
//! - Session state management
//! - Interactive permission requests
//! - Background process management

mod background_processes;
mod manager;
mod wrapped_child;
mod permission;
mod permission_request;
#[allow(clippy::module_inception)]
mod session;
mod usage;

pub use background_processes::{BackgroundProcessManager, BackgroundTerminal, ChildHandle, TerminalExitStatus};
pub use wrapped_child::WrappedChild;
pub use manager::SessionManager;
pub use permission::{PermissionHandler, PermissionMode, ToolPermissionResult};
pub use permission_request::{PermissionOutcome, PermissionRequestBuilder};
pub use session::Session;
pub use usage::UsageTracker;
