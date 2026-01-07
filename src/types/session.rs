//! Session-related types for token usage and statistics

use serde::{Deserialize, Serialize};

/// Token usage statistics
///
/// Tracks the number of tokens used in a session or query.
/// Can be parsed from SDK's `ResultMessage.usage` field.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Number of input tokens
    pub input_tokens: u64,

    /// Number of output tokens
    pub output_tokens: u64,

    /// Number of tokens read from cache
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u64>,

    /// Number of tokens written to cache
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u64>,
}

impl TokenUsage {
    /// Create a new empty token usage
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse from SDK's usage JSON value
    ///
    /// # Arguments
    ///
    /// * `usage` - The `usage` field from `ResultMessage` or `AssistantMessage`
    pub fn from_sdk_usage(usage: &serde_json::Value) -> Self {
        Self {
            input_tokens: usage["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: usage["output_tokens"].as_u64().unwrap_or(0),
            cache_read_input_tokens: usage["cache_read_input_tokens"].as_u64(),
            cache_creation_input_tokens: usage["cache_creation_input_tokens"].as_u64(),
        }
    }

    /// Add another usage to this one
    pub fn add(&mut self, other: &TokenUsage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;

        if let Some(v) = other.cache_read_input_tokens {
            *self.cache_read_input_tokens.get_or_insert(0) += v;
        }
        if let Some(v) = other.cache_creation_input_tokens {
            *self.cache_creation_input_tokens.get_or_insert(0) += v;
        }
    }

    /// Get total token count (input + output)
    pub fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }

    /// Check if any tokens were used
    pub fn is_empty(&self) -> bool {
        self.input_tokens == 0 && self.output_tokens == 0
    }
}

/// Session statistics
#[derive(Debug, Clone, Default)]
pub struct SessionStats {
    /// Number of active sessions
    pub active_sessions: usize,

    /// Total token usage across all sessions
    pub total_usage: TokenUsage,

    /// Total cost in USD
    pub total_cost_usd: f64,
}

impl SessionStats {
    /// Create new empty stats
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_token_usage_default() {
        let usage = TokenUsage::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert!(usage.cache_read_input_tokens.is_none());
        assert!(usage.is_empty());
        assert_eq!(usage.total(), 0);
    }

    #[test]
    fn test_token_usage_from_sdk() {
        let sdk_usage = json!({
            "input_tokens": 1000,
            "output_tokens": 500,
            "cache_read_input_tokens": 200,
            "cache_creation_input_tokens": 100
        });

        let usage = TokenUsage::from_sdk_usage(&sdk_usage);
        assert_eq!(usage.input_tokens, 1000);
        assert_eq!(usage.output_tokens, 500);
        assert_eq!(usage.cache_read_input_tokens, Some(200));
        assert_eq!(usage.cache_creation_input_tokens, Some(100));
        assert_eq!(usage.total(), 1500);
        assert!(!usage.is_empty());
    }

    #[test]
    fn test_token_usage_add() {
        let mut usage1 = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_input_tokens: Some(10),
            cache_creation_input_tokens: None,
        };

        let usage2 = TokenUsage {
            input_tokens: 200,
            output_tokens: 100,
            cache_read_input_tokens: Some(20),
            cache_creation_input_tokens: Some(5),
        };

        usage1.add(&usage2);

        assert_eq!(usage1.input_tokens, 300);
        assert_eq!(usage1.output_tokens, 150);
        assert_eq!(usage1.cache_read_input_tokens, Some(30));
        assert_eq!(usage1.cache_creation_input_tokens, Some(5));
    }

    #[test]
    fn test_token_usage_serialization() {
        let usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
        };

        let json = serde_json::to_string(&usage).unwrap();
        let parsed: TokenUsage = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.input_tokens, 100);
        assert_eq!(parsed.output_tokens, 50);
    }
}
