//! Claude Code ACP Agent
//!
//! A Rust implementation of the ACP (Agent Client Protocol) Agent for Claude Code,
//! enabling editors like Zed to use Claude Code capabilities.
//!
//! ## Features
//!
//! - ACP protocol support over stdio
//! - Session management with token usage tracking
//! - Streaming responses
//! - Permission mode handling
//!
//! ## Quick Start
//!
//! ```no_run
//! use claude_code_acp::run_acp;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     run_acp().await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Environment Variables
//!
//! - `ANTHROPIC_BASE_URL`: Custom API base URL
//! - `ANTHROPIC_API_KEY`: API key (preferred)
//! - `ANTHROPIC_AUTH_TOKEN`: Auth token (legacy, fallback)
//! - `ANTHROPIC_MODEL`: Model to use (default: claude-sonnet-4-20250514)
//! - `ANTHROPIC_SMALL_FAST_MODEL`: Model for fast operations

pub mod agent;
pub mod cli;
pub mod converter;
pub mod hooks;
pub mod mcp;
pub mod session;
pub mod settings;
pub mod terminal;
pub mod types;

pub use agent::{run_acp, run_acp_with_cli, shutdown_otel};
pub use cli::Cli;
pub use hooks::{create_post_tool_use_hook, create_pre_tool_use_hook, HookCallbackRegistry};
pub use mcp::{AcpMcpServer, McpServer, ToolContext, ToolRegistry, ToolResult, get_disallowed_tools};
pub use settings::{Settings, SettingsManager};
pub use terminal::{TerminalClient, TerminalHandle};
pub use types::{AgentConfig, AgentError, NewSessionMeta, Result};
