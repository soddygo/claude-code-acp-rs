//! Core ACP Agent structure
//!
//! The ClaudeAcpAgent holds shared state and configuration for handling
//! ACP protocol requests.

use std::sync::Arc;

use crate::session::SessionManager;
use crate::types::AgentConfig;

/// Claude ACP Agent
///
/// The main agent struct that holds configuration and session state.
/// This is shared across all request handlers.
#[derive(Debug)]
pub struct ClaudeAcpAgent {
    /// Agent configuration from environment
    config: AgentConfig,
    /// Session manager for tracking active sessions
    sessions: Arc<SessionManager>,
}

impl ClaudeAcpAgent {
    /// Create a new agent with configuration from environment
    pub fn new() -> Self {
        Self {
            config: AgentConfig::from_env(),
            sessions: Arc::new(SessionManager::new()),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: AgentConfig) -> Self {
        Self {
            config,
            sessions: Arc::new(SessionManager::new()),
        }
    }

    /// Get the agent configuration
    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    /// Get the session manager
    pub fn sessions(&self) -> &Arc<SessionManager> {
        &self.sessions
    }

    /// Get agent name for logging
    pub fn name(&self) -> &'static str {
        "claude-code-acp-rs"
    }

    /// Get agent version
    pub fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}

impl Default for ClaudeAcpAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_new() {
        let agent = ClaudeAcpAgent::new();
        assert_eq!(agent.name(), "claude-code-acp-rs");
        assert_eq!(agent.sessions().session_count(), 0);
    }

    #[test]
    fn test_agent_with_config() {
        let config = AgentConfig {
            base_url: Some("https://api.example.com".to_string()),
            api_key: Some("test-key".to_string()),
            model: Some("claude-3-opus".to_string()),
            small_fast_model: None,
            max_thinking_tokens: Some(4096),
        };

        let agent = ClaudeAcpAgent::with_config(config);
        assert_eq!(
            agent.config().base_url,
            Some("https://api.example.com".to_string())
        );
        assert_eq!(agent.config().max_thinking_tokens, Some(4096));
    }
}
