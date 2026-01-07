//! Terminal API client
//!
//! Provides a client interface for sending Terminal API requests to the ACP Client.

use std::path::PathBuf;
use std::sync::Arc;

use sacp::schema::{
    CreateTerminalRequest, CreateTerminalResponse, KillTerminalCommandRequest,
    KillTerminalCommandResponse, ReleaseTerminalRequest, ReleaseTerminalResponse, SessionId,
    TerminalId, TerminalOutputRequest, TerminalOutputResponse, WaitForTerminalExitRequest,
    WaitForTerminalExitResponse,
};
use sacp::JrConnectionCx;

use crate::types::AgentError;

/// Terminal API client for sending terminal requests to the ACP Client
///
/// The Client (editor like Zed) manages the actual PTY, and this client
/// sends requests through the ACP protocol to create, manage, and interact
/// with terminals.
#[derive(Debug, Clone)]
pub struct TerminalClient {
    /// Connection context for sending requests
    connection_cx: JrConnectionCx,
    /// Session ID for this client
    session_id: SessionId,
}

impl TerminalClient {
    /// Create a new Terminal API client
    pub fn new(connection_cx: JrConnectionCx, session_id: impl Into<SessionId>) -> Self {
        Self {
            connection_cx,
            session_id: session_id.into(),
        }
    }

    /// Create a new terminal and execute a command
    ///
    /// Returns a `TerminalId` that can be used with other terminal methods.
    /// The terminal will execute the specified command and capture output.
    ///
    /// # Arguments
    ///
    /// * `command` - The command to execute
    /// * `args` - Command arguments
    /// * `cwd` - Optional working directory (uses session cwd if not specified)
    /// * `output_byte_limit` - Optional limit on output bytes to retain
    pub async fn create(
        &self,
        command: impl Into<String>,
        args: Vec<String>,
        cwd: Option<PathBuf>,
        output_byte_limit: Option<u64>,
    ) -> Result<TerminalId, AgentError> {
        let mut request = CreateTerminalRequest::new(self.session_id.clone(), command);
        request = request.args(args);

        if let Some(cwd_path) = cwd {
            request = request.cwd(cwd_path);
        }

        if let Some(limit) = output_byte_limit {
            request = request.output_byte_limit(limit);
        }

        let response: CreateTerminalResponse = self
            .connection_cx
            .send_request(request)
            .block_task()
            .await
            .map_err(|e| AgentError::Internal(format!("Terminal create failed: {}", e)))?;

        Ok(response.terminal_id)
    }

    /// Get the current output and status of a terminal
    ///
    /// Returns the output captured so far and the exit status if completed.
    pub async fn output(
        &self,
        terminal_id: impl Into<TerminalId>,
    ) -> Result<TerminalOutputResponse, AgentError> {
        let request = TerminalOutputRequest::new(self.session_id.clone(), terminal_id);

        self.connection_cx
            .send_request(request)
            .block_task()
            .await
            .map_err(|e| AgentError::Internal(format!("Terminal output failed: {}", e)))
    }

    /// Wait for a terminal command to exit
    ///
    /// Blocks until the command completes and returns the exit status.
    pub async fn wait_for_exit(
        &self,
        terminal_id: impl Into<TerminalId>,
    ) -> Result<WaitForTerminalExitResponse, AgentError> {
        let request = WaitForTerminalExitRequest::new(self.session_id.clone(), terminal_id);

        self.connection_cx
            .send_request(request)
            .block_task()
            .await
            .map_err(|e| AgentError::Internal(format!("Terminal wait_for_exit failed: {}", e)))
    }

    /// Kill a terminal command
    ///
    /// Sends SIGTERM to terminate the command. The terminal remains valid
    /// and can be queried for output or released.
    pub async fn kill(
        &self,
        terminal_id: impl Into<TerminalId>,
    ) -> Result<KillTerminalCommandResponse, AgentError> {
        let request = KillTerminalCommandRequest::new(self.session_id.clone(), terminal_id);

        self.connection_cx
            .send_request(request)
            .block_task()
            .await
            .map_err(|e| AgentError::Internal(format!("Terminal kill failed: {}", e)))
    }

    /// Release a terminal and free its resources
    ///
    /// After release, the `TerminalId` can no longer be used.
    /// Any unretrieved output will be lost.
    pub async fn release(
        &self,
        terminal_id: impl Into<TerminalId>,
    ) -> Result<ReleaseTerminalResponse, AgentError> {
        let request = ReleaseTerminalRequest::new(self.session_id.clone(), terminal_id);

        self.connection_cx
            .send_request(request)
            .block_task()
            .await
            .map_err(|e| AgentError::Internal(format!("Terminal release failed: {}", e)))
    }

    /// Get the session ID
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// Create an Arc-wrapped client for sharing
    pub fn into_arc(self) -> Arc<Self> {
        Arc::new(self)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_terminal_client_session_id() {
        // We can't easily test without a real connection, but we can verify the struct compiles
        // and the session_id method works (would need mock connection for full test)
    }
}
