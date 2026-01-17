//! ACP Agent implementation
//!
//! This module provides the core Claude ACP Agent that handles:
//! - ACP protocol requests (initialize, session/new, session/prompt, etc.)
//! - Session lifecycle management
//! - Message conversion between ACP and Claude SDK

mod core;
mod flush;
mod handlers;
mod runner;

pub use core::ClaudeAcpAgent;
pub use runner::{run_acp, run_acp_with_cli, shutdown_otel};
