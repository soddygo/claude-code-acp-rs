//! Error tracing extensions
//!
//! Provides utilities for enriching error handling with tracing context.

use crate::types::AgentError;
use std::error::Error as StdError;

/// Extension trait for adding tracing context to errors
pub trait ErrorTraceExt {
    /// Log error with full context including error code, retryable status, and error chain
    fn trace_error(&self) -> &Self;
}

impl ErrorTraceExt for AgentError {
    fn trace_error(&self) -> &Self {
        // Check if this error type supports error_code and is_retryable
        let error_code = self.error_code();
        let is_retryable = self.is_retryable();
        let is_client_error = self.is_client_error();

        // Get the error chain
        let mut error_chain = Vec::new();
        let mut current_source = self.source();
        while let Some(source) = current_source {
            error_chain.push(source.to_string());
            current_source = source.source();
        }

        tracing::error!(
            error = %self,
            error_code = error_code.code(),
            error_code_name = ?error_code,
            is_retryable = is_retryable,
            is_client_error = is_client_error,
            error_chain_len = error_chain.len(),
            error_chain = ?error_chain,
            "Error occurred with full context"
        );

        self
    }
}

/// Extension trait for Result types
pub trait ResultTraceExt<T, E>: Sized {
    /// Convert error to AgentError and log with context
    fn trace_context(self) -> Result<T, AgentError>
    where
        E: StdError + Send + Sync + 'static;
}

impl<T, E> ResultTraceExt<T, E> for Result<T, E>
where
    E: StdError + Send + Sync + 'static,
    AgentError: From<E>,
{
    fn trace_context(self) -> Result<T, AgentError> {
        self.map_err(|e| {
            let agent_error = AgentError::from(e);
            agent_error.trace_error();
            agent_error
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_trace_ext() {
        let error = AgentError::connection_failed("Test connection failure");
        let _ = error.trace_error(); // Should log without panic
    }

    #[test]
    fn test_result_trace_ext() {
        let result: Result<(), std::io::Error> = Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "Connection refused",
        ));

        // This should convert to AgentError and log
        drop(result.trace_context());
    }
}
