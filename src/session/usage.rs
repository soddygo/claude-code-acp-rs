//! Token usage tracking for sessions
//!
//! Tracks cumulative token usage across a session's lifetime.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::types::TokenUsage;

/// Tracks token usage across a session
///
/// Thread-safe usage tracking using atomic operations.
#[derive(Debug, Default)]
pub struct UsageTracker {
    /// Total input tokens consumed
    input_tokens: AtomicU64,
    /// Total output tokens generated
    output_tokens: AtomicU64,
    /// Total cache read tokens
    cache_read_input_tokens: AtomicU64,
    /// Total cache creation tokens
    cache_creation_input_tokens: AtomicU64,
}

impl UsageTracker {
    /// Create a new usage tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Add usage from a completed request
    pub fn add(&self, usage: &TokenUsage) {
        self.input_tokens
            .fetch_add(usage.input_tokens, Ordering::Relaxed);
        self.output_tokens
            .fetch_add(usage.output_tokens, Ordering::Relaxed);
        if let Some(v) = usage.cache_read_input_tokens {
            self.cache_read_input_tokens.fetch_add(v, Ordering::Relaxed);
        }
        if let Some(v) = usage.cache_creation_input_tokens {
            self.cache_creation_input_tokens
                .fetch_add(v, Ordering::Relaxed);
        }
    }

    /// Get current cumulative usage
    pub fn get(&self) -> TokenUsage {
        TokenUsage {
            input_tokens: self.input_tokens.load(Ordering::Relaxed),
            output_tokens: self.output_tokens.load(Ordering::Relaxed),
            cache_read_input_tokens: Some(self.cache_read_input_tokens.load(Ordering::Relaxed)),
            cache_creation_input_tokens: Some(
                self.cache_creation_input_tokens.load(Ordering::Relaxed),
            ),
        }
    }

    /// Reset usage counters
    pub fn reset(&self) {
        self.input_tokens.store(0, Ordering::Relaxed);
        self.output_tokens.store(0, Ordering::Relaxed);
        self.cache_read_input_tokens.store(0, Ordering::Relaxed);
        self.cache_creation_input_tokens.store(0, Ordering::Relaxed);
    }

    /// Get input tokens
    pub fn input_tokens(&self) -> u64 {
        self.input_tokens.load(Ordering::Relaxed)
    }

    /// Get output tokens
    pub fn output_tokens(&self) -> u64 {
        self.output_tokens.load(Ordering::Relaxed)
    }

    /// Get total tokens (input + output)
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens() + self.output_tokens()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usage_tracker_new() {
        let tracker = UsageTracker::new();
        let usage = tracker.get();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
    }

    #[test]
    fn test_usage_tracker_add() {
        let tracker = UsageTracker::new();
        let usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_input_tokens: Some(10),
            cache_creation_input_tokens: Some(5),
        };

        tracker.add(&usage);
        let total = tracker.get();
        assert_eq!(total.input_tokens, 100);
        assert_eq!(total.output_tokens, 50);
    }

    #[test]
    fn test_usage_tracker_cumulative() {
        let tracker = UsageTracker::new();

        tracker.add(&TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
        });

        tracker.add(&TokenUsage {
            input_tokens: 200,
            output_tokens: 100,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
        });

        let total = tracker.get();
        assert_eq!(total.input_tokens, 300);
        assert_eq!(total.output_tokens, 150);
        assert_eq!(tracker.total_tokens(), 450);
    }

    #[test]
    fn test_usage_tracker_reset() {
        let tracker = UsageTracker::new();
        tracker.add(&TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_input_tokens: Some(10),
            cache_creation_input_tokens: Some(5),
        });

        tracker.reset();
        let total = tracker.get();
        assert_eq!(total.input_tokens, 0);
        assert_eq!(total.output_tokens, 0);
    }
}
