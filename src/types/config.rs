//! Agent configuration from environment variables

use std::collections::HashMap;

/// Agent configuration loaded from environment variables
///
/// Supports configuring alternative AI model providers (e.g., domestic providers in China)
/// through environment variables.
#[derive(Debug, Clone, Default)]
pub struct AgentConfig {
    /// Anthropic API base URL
    /// Environment variable: `ANTHROPIC_BASE_URL`
    pub base_url: Option<String>,

    /// API key for authentication
    /// Environment variable: `ANTHROPIC_API_KEY` (preferred) or `ANTHROPIC_AUTH_TOKEN` (legacy)
    pub api_key: Option<String>,

    /// Primary model name
    /// Environment variable: `ANTHROPIC_MODEL`
    pub model: Option<String>,

    /// Small/fast model name (fallback)
    /// Environment variable: `ANTHROPIC_SMALL_FAST_MODEL`
    pub small_fast_model: Option<String>,

    /// Maximum tokens for thinking blocks (extended thinking mode)
    /// Environment variable: `MAX_THINKING_TOKENS`
    /// Typical values: 4096, 8000, 16000
    pub max_thinking_tokens: Option<u32>,
}

impl AgentConfig {
    /// Create a new empty configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Load configuration from environment variables
    ///
    /// Reads the following environment variables:
    /// - `ANTHROPIC_BASE_URL`: API base URL
    /// - `ANTHROPIC_API_KEY`: API key (preferred)
    /// - `ANTHROPIC_AUTH_TOKEN`: Auth token (legacy, fallback if API_KEY not set)
    /// - `ANTHROPIC_MODEL`: Primary model name
    /// - `ANTHROPIC_SMALL_FAST_MODEL`: Small/fast model name
    /// - `MAX_THINKING_TOKENS`: Maximum tokens for thinking blocks
    pub fn from_env() -> Self {
        // Prefer ANTHROPIC_API_KEY, fallback to ANTHROPIC_AUTH_TOKEN for compatibility
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .or_else(|| std::env::var("ANTHROPIC_AUTH_TOKEN").ok());

        // Parse MAX_THINKING_TOKENS if present
        let max_thinking_tokens = std::env::var("MAX_THINKING_TOKENS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok());

        Self {
            base_url: std::env::var("ANTHROPIC_BASE_URL").ok(),
            api_key,
            model: std::env::var("ANTHROPIC_MODEL").ok(),
            small_fast_model: std::env::var("ANTHROPIC_SMALL_FAST_MODEL").ok(),
            max_thinking_tokens,
        }
    }

    /// Check if any configuration is set
    pub fn is_configured(&self) -> bool {
        self.base_url.is_some()
            || self.api_key.is_some()
            || self.model.is_some()
            || self.small_fast_model.is_some()
            || self.max_thinking_tokens.is_some()
    }

    /// Get environment variables to pass to Claude Code CLI
    ///
    /// Returns a HashMap of environment variable names and values
    /// that should be passed to the subprocess.
    pub fn to_env_vars(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();

        if let Some(ref url) = self.base_url {
            env.insert("ANTHROPIC_BASE_URL".to_string(), url.clone());
        }
        // Pass as ANTHROPIC_API_KEY (standard name for Claude CLI)
        if let Some(ref key) = self.api_key {
            env.insert("ANTHROPIC_API_KEY".to_string(), key.clone());
        }
        if let Some(ref model) = self.model {
            env.insert("ANTHROPIC_MODEL".to_string(), model.clone());
        }
        if let Some(ref model) = self.small_fast_model {
            env.insert("ANTHROPIC_SMALL_FAST_MODEL".to_string(), model.clone());
        }

        env
    }

    /// Apply configuration to ClaudeAgentOptions
    ///
    /// Sets the model and environment variables on the options.
    pub fn apply_to_options(&self, options: &mut claude_code_agent_sdk::ClaudeAgentOptions) {
        // Set model if configured
        if let Some(ref model) = self.model {
            options.model = Some(model.clone());
        }

        // Set fallback model if configured
        if let Some(ref fallback) = self.small_fast_model {
            options.fallback_model = Some(fallback.clone());
        }

        // Set max_thinking_tokens if configured (enables extended thinking mode)
        if let Some(tokens) = self.max_thinking_tokens {
            options.max_thinking_tokens = Some(tokens);
        }

        // Pass base_url and api_key as environment variables
        let env_vars = self.to_env_vars();
        if !env_vars.is_empty() {
            options.env = env_vars;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AgentConfig::default();
        assert!(config.base_url.is_none());
        assert!(config.api_key.is_none());
        assert!(config.model.is_none());
        assert!(config.small_fast_model.is_none());
        assert!(config.max_thinking_tokens.is_none());
        assert!(!config.is_configured());
    }

    #[test]
    fn test_to_env_vars() {
        let config = AgentConfig {
            base_url: Some("https://api.example.com".to_string()),
            api_key: Some("secret-key".to_string()),
            model: Some("claude-3".to_string()),
            small_fast_model: None,
            max_thinking_tokens: None,
        };

        let env = config.to_env_vars();
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").unwrap(),
            "https://api.example.com"
        );
        assert_eq!(env.get("ANTHROPIC_API_KEY").unwrap(), "secret-key");
        assert_eq!(env.get("ANTHROPIC_MODEL").unwrap(), "claude-3");
        assert!(!env.contains_key("ANTHROPIC_SMALL_FAST_MODEL"));
    }

    #[test]
    fn test_is_configured() {
        let mut config = AgentConfig::default();
        assert!(!config.is_configured());

        config.model = Some("test".to_string());
        assert!(config.is_configured());
    }

    #[test]
    fn test_max_thinking_tokens_config() {
        let config = AgentConfig {
            base_url: None,
            api_key: None,
            model: None,
            small_fast_model: None,
            max_thinking_tokens: Some(4096),
        };

        assert!(config.is_configured());
        assert_eq!(config.max_thinking_tokens, Some(4096));
    }
}
