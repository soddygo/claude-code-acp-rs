//! Background process management for terminal commands
//!
//! Manages background terminal processes that are started with `run_in_background=true`.
//! Supports retrieving incremental output and killing running processes.

use std::sync::Arc;

use dashmap::DashMap;
use tokio::process::Child;
use tokio::sync::Mutex;

/// Terminal exit status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalExitStatus {
    /// Process exited normally with exit code
    Exited(i32),
    /// Process was killed by user
    Killed,
    /// Process timed out
    TimedOut,
    /// Process was aborted
    Aborted,
}

impl TerminalExitStatus {
    /// Get status string for API response
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Exited(_) => "exited",
            Self::Killed => "killed",
            Self::TimedOut => "timedOut",
            Self::Aborted => "aborted",
        }
    }
}

/// Background terminal state
#[derive(Debug)]
pub enum BackgroundTerminal {
    /// Terminal is still running
    Running {
        /// The child process
        child: Arc<Mutex<Child>>,
        /// Accumulated output buffer
        output_buffer: Arc<Mutex<String>>,
        /// Last read offset for incremental output
        last_read_offset: Arc<Mutex<usize>>,
    },
    /// Terminal has finished
    Finished {
        /// Exit status
        status: TerminalExitStatus,
        /// Final output
        final_output: String,
    },
}

impl BackgroundTerminal {
    /// Create a new running terminal
    pub fn new_running(child: Child) -> Self {
        Self::Running {
            child: Arc::new(Mutex::new(child)),
            output_buffer: Arc::new(Mutex::new(String::new())),
            last_read_offset: Arc::new(Mutex::new(0)),
        }
    }

    /// Check if terminal is still running
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    /// Get the status string
    pub fn status_str(&self) -> &'static str {
        match self {
            Self::Running { .. } => "running",
            Self::Finished { status, .. } => status.as_str(),
        }
    }

    /// Get incremental output since last read
    pub async fn get_incremental_output(&self) -> String {
        match self {
            Self::Running {
                output_buffer,
                last_read_offset,
                ..
            } => {
                let buffer = output_buffer.lock().await;
                let mut offset = last_read_offset.lock().await;
                let new_output = buffer[*offset..].to_string();
                *offset = buffer.len();
                new_output
            }
            Self::Finished { final_output, .. } => final_output.clone(),
        }
    }

    /// Append output to the buffer (for running terminals)
    pub async fn append_output(&self, output: &str) {
        if let Self::Running { output_buffer, .. } = self {
            let mut buffer = output_buffer.lock().await;
            buffer.push_str(output);
        }
    }

    /// Get all output
    pub async fn get_all_output(&self) -> String {
        match self {
            Self::Running { output_buffer, .. } => {
                let buffer = output_buffer.lock().await;
                buffer.clone()
            }
            Self::Finished { final_output, .. } => final_output.clone(),
        }
    }

    /// Transition to finished state
    pub async fn finish(self, status: TerminalExitStatus) -> Self {
        match self {
            Self::Running { output_buffer, .. } => {
                let final_output = output_buffer.lock().await.clone();
                Self::Finished {
                    status,
                    final_output,
                }
            }
            finished @ Self::Finished { .. } => finished,
        }
    }
}

/// Manager for background terminal processes
#[derive(Debug, Default)]
pub struct BackgroundProcessManager {
    /// Map of shell ID to background terminal
    terminals: DashMap<String, BackgroundTerminal>,
}

impl BackgroundProcessManager {
    /// Create a new background process manager
    pub fn new() -> Self {
        Self {
            terminals: DashMap::new(),
        }
    }

    /// Register a new background terminal
    pub fn register(&self, shell_id: String, terminal: BackgroundTerminal) {
        self.terminals.insert(shell_id, terminal);
    }

    /// Check if a terminal exists
    pub fn has_terminal(&self, shell_id: &str) -> bool {
        self.terminals.contains_key(shell_id)
    }

    /// Get terminal by ID (returns reference for reading)
    pub fn get(
        &self,
        shell_id: &str,
    ) -> Option<dashmap::mapref::one::Ref<'_, String, BackgroundTerminal>> {
        self.terminals.get(shell_id)
    }

    /// Get mutable terminal by ID
    pub fn get_mut(
        &self,
        shell_id: &str,
    ) -> Option<dashmap::mapref::one::RefMut<'_, String, BackgroundTerminal>> {
        self.terminals.get_mut(shell_id)
    }

    /// Remove terminal by ID
    pub fn remove(&self, shell_id: &str) -> Option<(String, BackgroundTerminal)> {
        self.terminals.remove(shell_id)
    }

    /// Update a terminal to finished state
    pub async fn finish_terminal(&self, shell_id: &str, status: TerminalExitStatus) {
        if let Some((id, terminal)) = self.terminals.remove(shell_id) {
            let finished = terminal.finish(status).await;
            self.terminals.insert(id, finished);
        }
    }

    /// Get number of terminals
    pub fn count(&self) -> usize {
        self.terminals.len()
    }

    /// Get all shell IDs
    pub fn shell_ids(&self) -> Vec<String> {
        self.terminals.iter().map(|r| r.key().clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_exit_status() {
        assert_eq!(TerminalExitStatus::Exited(0).as_str(), "exited");
        assert_eq!(TerminalExitStatus::Killed.as_str(), "killed");
        assert_eq!(TerminalExitStatus::TimedOut.as_str(), "timedOut");
        assert_eq!(TerminalExitStatus::Aborted.as_str(), "aborted");
    }

    #[test]
    fn test_background_process_manager_new() {
        let manager = BackgroundProcessManager::new();
        assert_eq!(manager.count(), 0);
    }

    #[test]
    fn test_background_process_manager_has_terminal() {
        let manager = BackgroundProcessManager::new();
        assert!(!manager.has_terminal("test-id"));
    }

    #[tokio::test]
    async fn test_background_terminal_finished() {
        let terminal = BackgroundTerminal::Finished {
            status: TerminalExitStatus::Exited(0),
            final_output: "test output".to_string(),
        };

        assert!(!terminal.is_running());
        assert_eq!(terminal.status_str(), "exited");
        assert_eq!(terminal.get_all_output().await, "test output");
    }
}
