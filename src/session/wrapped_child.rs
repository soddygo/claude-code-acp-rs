//! Wrapped child process with process group support
//!
//! Provides a unified interface over process-wrap's ChildWrapper
//! while maintaining compatibility with existing Arc<Mutex<>> pattern.

use process_wrap::tokio::ChildWrapper;
use std::io;
use std::pin::Pin;

/// Wrapper around Box<dyn ChildWrapper> that provides
/// a stable interface compatible with Arc<Mutex<>>
///
/// This wraps the process-wrap ChildWrapper trait to work
/// with our existing Arc<Mutex<>> storage pattern in BackgroundTerminal.
#[derive(Debug)]
pub struct WrappedChild {
    inner: Box<dyn ChildWrapper>,
}

impl WrappedChild {
    /// Create a new wrapped child from a process-wrap ChildWrapper
    pub fn new(inner: Box<dyn ChildWrapper>) -> Self {
        Self { inner }
    }

    /// Get mutable reference to inner ChildWrapper
    pub fn inner_mut(&mut self) -> &mut dyn ChildWrapper {
        self.inner.as_mut()
    }

    /// Kill the process group and wait for exit
    ///
    /// This will terminate the entire process group, not just the parent process.
    pub async fn kill(&mut self) -> io::Result<()> {
        Pin::from(self.inner.kill()).await
    }

    /// Start killing without waiting for exit
    ///
    /// This initiates the kill operation but returns immediately.
    pub fn start_kill(&mut self) -> io::Result<()> {
        self.inner.start_kill()
    }

    /// Wait for the process to exit
    pub async fn wait(&mut self) -> io::Result<std::process::ExitStatus> {
        Pin::from(self.inner.wait()).await
    }

    /// Try to wait without blocking
    ///
    /// Returns Some(status) if the process has exited, None if still running.
    pub fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        self.inner.try_wait()
    }

    /// Send a specific signal to the process group (Unix only)
    ///
    /// # Arguments
    /// * `sig` - Signal number (e.g., libc::SIGTERM, libc::SIGKILL)
    #[cfg(unix)]
    pub fn signal(&self, sig: i32) -> io::Result<()> {
        self.inner.signal(sig)
    }

    /// Get the process ID
    pub fn id(&self) -> u32 {
        self.inner.id().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_wrapped_child_creation() {
        // This is a placeholder test
        // Real tests would require spawning an actual process
        // which is better done as integration tests
    }
}
