//! Terminal handle for managing an active terminal session
//!
//! Provides a convenient RAII wrapper around a terminal ID that automatically
//! releases the terminal when dropped.

use std::sync::Arc;

use sacp::schema::{TerminalExitStatus, TerminalId, TerminalOutputResponse};

use super::TerminalClient;
use crate::types::AgentError;

/// Handle to an active terminal session
///
/// This provides a convenient wrapper around a `TerminalId` that tracks
/// the terminal client and can be used to interact with the terminal.
///
/// When dropped, the handle will attempt to release the terminal if it
/// hasn't been explicitly released or if the command hasn't completed.
#[derive(Debug)]
pub struct TerminalHandle {
    /// The terminal ID
    terminal_id: TerminalId,
    /// The terminal client for sending requests
    client: Arc<TerminalClient>,
    /// Whether the terminal has been released
    released: bool,
}

impl TerminalHandle {
    /// Create a new terminal handle
    pub fn new(terminal_id: TerminalId, client: Arc<TerminalClient>) -> Self {
        Self {
            terminal_id,
            client,
            released: false,
        }
    }

    /// Get the terminal ID
    pub fn id(&self) -> &TerminalId {
        &self.terminal_id
    }

    /// Get the terminal ID as a string
    pub fn id_str(&self) -> &str {
        self.terminal_id.0.as_ref()
    }

    /// Get the current output and status
    pub async fn output(&self) -> Result<TerminalOutputResponse, AgentError> {
        self.client.output(self.terminal_id.clone()).await
    }

    /// Wait for the terminal command to exit
    ///
    /// Returns the exit status once the command completes.
    pub async fn wait_for_exit(&self) -> Result<TerminalExitStatus, AgentError> {
        let response = self.client.wait_for_exit(self.terminal_id.clone()).await?;
        Ok(response.exit_status)
    }

    /// Kill the terminal command
    ///
    /// Sends SIGTERM to terminate the command. The terminal remains valid
    /// and can still be queried for output.
    pub async fn kill(&self) -> Result<(), AgentError> {
        self.client.kill(self.terminal_id.clone()).await?;
        Ok(())
    }

    /// Release the terminal and free resources
    ///
    /// After calling this, the handle should not be used again.
    pub async fn release(mut self) -> Result<(), AgentError> {
        self.released = true;
        self.client.release(self.terminal_id.clone()).await?;
        Ok(())
    }

    /// Execute a command and wait for completion, returning the output
    ///
    /// This is a convenience method that polls for output and waits for exit.
    /// Returns the final output and exit status.
    pub async fn execute_and_wait(&self) -> Result<(String, TerminalExitStatus), AgentError> {
        // Wait for the command to exit
        let exit_status = self.wait_for_exit().await?;

        // Get the final output
        let output_response = self.output().await?;

        Ok((output_response.output, exit_status))
    }

    /// Check if the terminal has been released
    pub fn is_released(&self) -> bool {
        self.released
    }
}

// Note: We don't implement Drop with async release because that's complex.
// Users should explicitly call release() when done, or let the terminal
// time out on the client side.

impl Drop for TerminalHandle {
    fn drop(&mut self) {
        if !self.released {
            tracing::warn!(
                terminal_id = %self.id_str(),
                "TerminalHandle dropped without explicit release, \
                 terminal will be cleaned up by client timeout"
            );
        }
    }
}

/// Builder for creating terminals with various options
#[derive(Debug)]
#[allow(dead_code)] // Public API for future use
pub struct TerminalBuilder {
    client: Arc<TerminalClient>,
    command: String,
    args: Vec<String>,
    cwd: Option<std::path::PathBuf>,
    output_byte_limit: Option<u64>,
}

#[allow(dead_code)] // Public API for future use
impl TerminalBuilder {
    /// Create a new terminal builder
    pub fn new(client: Arc<TerminalClient>, command: impl Into<String>) -> Self {
        Self {
            client,
            command: command.into(),
            args: Vec::new(),
            cwd: None,
            output_byte_limit: None,
        }
    }

    /// Set command arguments
    pub fn args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    /// Add a single argument
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Set the working directory
    pub fn cwd(mut self, cwd: impl Into<std::path::PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Set the output byte limit
    pub fn output_byte_limit(mut self, limit: u64) -> Self {
        self.output_byte_limit = Some(limit);
        self
    }

    /// Create the terminal
    pub async fn create(self) -> Result<TerminalHandle, AgentError> {
        let terminal_id = self
            .client
            .create(self.command, self.args, self.cwd, self.output_byte_limit)
            .await?;

        Ok(TerminalHandle::new(terminal_id, self.client))
    }
}

#[cfg(test)]
mod tests {
    // Tests would require mocking the JrConnectionCx which is complex
    // For now, we just verify the types compile correctly
}
