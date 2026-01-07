//! Error types for Claude Code ACP Agent

use thiserror::Error;

/// ACP protocol error codes
///
/// Standard JSON-RPC error codes and ACP-specific codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    // Standard JSON-RPC errors (-32xxx)
    /// Parse error: Invalid JSON
    ParseError = -32700,
    /// Invalid request: Not a valid request object
    InvalidRequest = -32600,
    /// Method not found
    MethodNotFound = -32601,
    /// Invalid params
    InvalidParams = -32602,
    /// Internal error
    InternalError = -32603,

    // ACP-specific errors (-32000 to -32099)
    /// Session not found
    SessionNotFound = -32001,
    /// Session already exists
    SessionAlreadyExists = -32002,
    /// Not connected to Claude
    NotConnected = -32003,
    /// Authentication required
    AuthRequired = -32004,
    /// Invalid mode
    InvalidMode = -32005,
    /// Operation cancelled
    Cancelled = -32006,
    /// Connection failed
    ConnectionFailed = -32007,
    /// Streaming error
    StreamingError = -32008,
    /// Tool execution failed
    ToolFailed = -32009,
    /// Configuration error
    ConfigError = -32010,
}

impl ErrorCode {
    /// Get the error code value
    pub fn code(self) -> i32 {
        self as i32
    }
}

/// Main error type for the ACP Agent
#[derive(Debug, Error)]
pub enum AgentError {
    // === Session errors ===
    /// Session not found
    #[error("Session not found: {0}")]
    SessionNotFound(String),

    /// Session already exists
    #[error("Session already exists: {0}")]
    SessionAlreadyExists(String),

    /// Session is closed
    #[error("Session is closed: {0}")]
    SessionClosed(String),

    // === Connection errors ===
    /// Client not connected
    #[error("Client not connected")]
    NotConnected,

    /// Connection failed
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// Connection timeout
    #[error("Connection timeout after {0}ms")]
    ConnectionTimeout(u64),

    /// Already connected
    #[error("Already connected")]
    AlreadyConnected,

    // === Authentication errors ===
    /// Authentication required
    #[error("Authentication required")]
    AuthRequired,

    /// Invalid API key
    #[error("Invalid API key")]
    InvalidApiKey,

    // === Mode errors ===
    /// Invalid mode
    #[error("Invalid mode: {0}")]
    InvalidMode(String),

    // === Prompt errors ===
    /// Empty prompt
    #[error("Prompt cannot be empty")]
    EmptyPrompt,

    /// Prompt too long
    #[error("Prompt exceeds maximum length: {length} > {max}")]
    PromptTooLong { length: usize, max: usize },

    // === Streaming errors ===
    /// Streaming error
    #[error("Streaming error: {0}")]
    StreamingError(String),

    /// Notification send failed
    #[error("Failed to send notification: {0}")]
    NotificationFailed(String),

    // === Tool errors ===
    /// Tool execution failed
    #[error("Tool execution failed: {0}")]
    ToolExecutionFailed(String),

    /// Tool not found
    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    /// Tool permission denied
    #[error("Tool permission denied: {0}")]
    ToolPermissionDenied(String),

    // === Configuration errors ===
    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Missing required configuration
    #[error("Missing required configuration: {0}")]
    MissingConfig(String),

    // === External errors ===
    /// Claude SDK error
    #[error("Claude SDK error: {0}")]
    ClaudeSdk(#[from] claude_code_agent_sdk::ClaudeError),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    // === Generic errors ===
    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),

    /// Cancelled
    #[error("Operation cancelled")]
    Cancelled,
}

/// Result type for the ACP Agent
pub type Result<T> = std::result::Result<T, AgentError>;

impl AgentError {
    /// Get the ACP error code for this error
    pub fn error_code(&self) -> ErrorCode {
        match self {
            AgentError::SessionNotFound(_) => ErrorCode::SessionNotFound,
            AgentError::SessionAlreadyExists(_) => ErrorCode::SessionAlreadyExists,
            AgentError::SessionClosed(_) => ErrorCode::SessionNotFound,
            AgentError::NotConnected => ErrorCode::NotConnected,
            AgentError::ConnectionFailed(_) => ErrorCode::ConnectionFailed,
            AgentError::ConnectionTimeout(_) => ErrorCode::ConnectionFailed,
            AgentError::AlreadyConnected => ErrorCode::InternalError,
            AgentError::AuthRequired => ErrorCode::AuthRequired,
            AgentError::InvalidApiKey => ErrorCode::AuthRequired,
            AgentError::InvalidMode(_) => ErrorCode::InvalidMode,
            AgentError::EmptyPrompt => ErrorCode::InvalidParams,
            AgentError::PromptTooLong { .. } => ErrorCode::InvalidParams,
            AgentError::StreamingError(_) => ErrorCode::StreamingError,
            AgentError::NotificationFailed(_) => ErrorCode::StreamingError,
            AgentError::ToolExecutionFailed(_) => ErrorCode::ToolFailed,
            AgentError::ToolNotFound(_) => ErrorCode::ToolFailed,
            AgentError::ToolPermissionDenied(_) => ErrorCode::ToolFailed,
            AgentError::ConfigError(_) => ErrorCode::ConfigError,
            AgentError::MissingConfig(_) => ErrorCode::ConfigError,
            AgentError::ClaudeSdk(_) => ErrorCode::InternalError,
            AgentError::Io(_) => ErrorCode::InternalError,
            AgentError::Json(_) => ErrorCode::ParseError,
            AgentError::Internal(_) => ErrorCode::InternalError,
            AgentError::Cancelled => ErrorCode::Cancelled,
        }
    }

    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            AgentError::ConnectionFailed(_)
                | AgentError::ConnectionTimeout(_)
                | AgentError::StreamingError(_)
                | AgentError::NotificationFailed(_)
        )
    }

    /// Check if this error is a client error (caused by invalid input)
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            AgentError::SessionNotFound(_)
                | AgentError::InvalidMode(_)
                | AgentError::EmptyPrompt
                | AgentError::PromptTooLong { .. }
                | AgentError::ToolNotFound(_)
                | AgentError::ToolPermissionDenied(_)
        )
    }

    // === Constructor helpers ===

    /// Create an internal error
    pub fn internal(msg: impl Into<String>) -> Self {
        AgentError::Internal(msg.into())
    }

    /// Create a session not found error
    pub fn session_not_found(session_id: impl Into<String>) -> Self {
        AgentError::SessionNotFound(session_id.into())
    }

    /// Create a session already exists error
    pub fn session_already_exists(session_id: impl Into<String>) -> Self {
        AgentError::SessionAlreadyExists(session_id.into())
    }

    /// Create an invalid mode error
    pub fn invalid_mode(mode: impl Into<String>) -> Self {
        AgentError::InvalidMode(mode.into())
    }

    /// Create a tool execution failed error
    pub fn tool_failed(msg: impl Into<String>) -> Self {
        AgentError::ToolExecutionFailed(msg.into())
    }

    /// Create a connection failed error
    pub fn connection_failed(msg: impl Into<String>) -> Self {
        AgentError::ConnectionFailed(msg.into())
    }

    /// Create a streaming error
    pub fn streaming_error(msg: impl Into<String>) -> Self {
        AgentError::StreamingError(msg.into())
    }

    /// Create a notification failed error
    pub fn notification_failed(msg: impl Into<String>) -> Self {
        AgentError::NotificationFailed(msg.into())
    }

    /// Create a configuration error
    pub fn config_error(msg: impl Into<String>) -> Self {
        AgentError::ConfigError(msg.into())
    }

    /// Create a missing config error
    pub fn missing_config(key: impl Into<String>) -> Self {
        AgentError::MissingConfig(key.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = AgentError::session_not_found("test-123");
        assert_eq!(err.to_string(), "Session not found: test-123");

        let err = AgentError::invalid_mode("unknown");
        assert_eq!(err.to_string(), "Invalid mode: unknown");
    }

    #[test]
    fn test_error_codes() {
        let err = AgentError::session_not_found("test");
        assert_eq!(err.error_code(), ErrorCode::SessionNotFound);
        assert_eq!(err.error_code().code(), -32001);

        let err = AgentError::NotConnected;
        assert_eq!(err.error_code(), ErrorCode::NotConnected);

        let err = AgentError::Cancelled;
        assert_eq!(err.error_code(), ErrorCode::Cancelled);
    }

    #[test]
    fn test_is_retryable() {
        assert!(AgentError::connection_failed("timeout").is_retryable());
        assert!(AgentError::streaming_error("lost").is_retryable());
        assert!(!AgentError::session_not_found("x").is_retryable());
        assert!(!AgentError::Cancelled.is_retryable());
    }

    #[test]
    fn test_is_client_error() {
        assert!(AgentError::session_not_found("x").is_client_error());
        assert!(AgentError::invalid_mode("bad").is_client_error());
        assert!(AgentError::EmptyPrompt.is_client_error());
        assert!(!AgentError::NotConnected.is_client_error());
        assert!(!AgentError::internal("oops").is_client_error());
    }

    #[test]
    fn test_prompt_too_long() {
        let err = AgentError::PromptTooLong {
            length: 100_000,
            max: 50_000,
        };
        assert_eq!(
            err.to_string(),
            "Prompt exceeds maximum length: 100000 > 50000"
        );
        assert!(err.is_client_error());
    }

    #[test]
    fn test_constructor_helpers() {
        // Just verify the constructors work and return the expected types
        assert!(matches!(
            AgentError::session_already_exists("sess-1"),
            AgentError::SessionAlreadyExists(_)
        ));
        assert!(matches!(
            AgentError::connection_failed("refused"),
            AgentError::ConnectionFailed(_)
        ));
        assert!(matches!(
            AgentError::streaming_error("disconnected"),
            AgentError::StreamingError(_)
        ));
        assert!(matches!(
            AgentError::notification_failed("timeout"),
            AgentError::NotificationFailed(_)
        ));
        assert!(matches!(
            AgentError::config_error("invalid format"),
            AgentError::ConfigError(_)
        ));
        assert!(matches!(
            AgentError::missing_config("ANTHROPIC_API_KEY"),
            AgentError::MissingConfig(_)
        ));
    }
}
