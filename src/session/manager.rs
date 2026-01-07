//! Session manager for tracking active sessions
//!
//! Uses DashMap for concurrent access with entry API to avoid deadlocks.

use std::path::PathBuf;
use std::sync::Arc;

use dashmap::DashMap;

use crate::types::{AgentConfig, AgentError, NewSessionMeta, Result};

use super::session::Session;

/// Manager for active sessions
///
/// Provides thread-safe session storage and lookup using DashMap.
/// Uses entry API for atomic operations to prevent deadlocks.
#[derive(Debug, Default)]
pub struct SessionManager {
    /// Active sessions keyed by session_id
    sessions: DashMap<String, Arc<Session>>,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
        }
    }

    /// Create a new session and store it
    ///
    /// # Arguments
    ///
    /// * `session_id` - Unique identifier for the session
    /// * `cwd` - Working directory for the session
    /// * `config` - Agent configuration
    /// * `meta` - Optional session metadata
    ///
    /// # Returns
    ///
    /// Arc reference to the created session
    pub fn create_session(
        &self,
        session_id: String,
        cwd: PathBuf,
        config: &AgentConfig,
        meta: Option<&NewSessionMeta>,
    ) -> Result<Arc<Session>> {
        // Use entry API to atomically check and insert
        let entry = self.sessions.entry(session_id.clone());

        match entry {
            dashmap::Entry::Occupied(_) => {
                // Session already exists
                Err(AgentError::SessionAlreadyExists(session_id))
            }
            dashmap::Entry::Vacant(vacant) => {
                let session = Session::new(session_id, cwd, config, meta)?;
                let arc_session = Arc::new(session);
                vacant.insert(Arc::clone(&arc_session));
                Ok(arc_session)
            }
        }
    }

    /// Get an existing session
    pub fn get_session(&self, session_id: &str) -> Option<Arc<Session>> {
        self.sessions.get(session_id).map(|r| Arc::clone(&r))
    }

    /// Get an existing session or return SessionNotFound error
    pub fn get_session_or_error(&self, session_id: &str) -> Result<Arc<Session>> {
        self.get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))
    }

    /// Remove a session
    pub fn remove_session(&self, session_id: &str) -> Option<Arc<Session>> {
        self.sessions.remove(session_id).map(|(_, v)| v)
    }

    /// Check if a session exists
    pub fn has_session(&self, session_id: &str) -> bool {
        self.sessions.contains_key(session_id)
    }

    /// Get the number of active sessions
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Get all session IDs
    pub fn session_ids(&self) -> Vec<String> {
        self.sessions.iter().map(|r| r.key().clone()).collect()
    }

    /// Clear all sessions
    pub fn clear(&self) {
        self.sessions.clear();
    }

    /// Execute a function on a session if it exists
    ///
    /// Uses entry API to safely access the session without holding the lock
    pub fn with_session<F, R>(&self, session_id: &str, f: F) -> Option<R>
    where
        F: FnOnce(&Arc<Session>) -> R,
    {
        self.sessions.get(session_id).map(|r| f(&r))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AgentConfig {
        AgentConfig {
            base_url: None,
            auth_token: None,
            model: None,
            small_fast_model: None,
        }
    }

    #[test]
    fn test_manager_new() {
        let manager = SessionManager::new();
        assert_eq!(manager.session_count(), 0);
    }

    #[test]
    fn test_manager_create_session() {
        let manager = SessionManager::new();
        let config = test_config();

        let session = manager
            .create_session("session-1".to_string(), PathBuf::from("/tmp"), &config, None)
            .unwrap();

        assert_eq!(session.session_id, "session-1");
        assert_eq!(manager.session_count(), 1);
        assert!(manager.has_session("session-1"));
    }

    #[test]
    fn test_manager_get_session() {
        let manager = SessionManager::new();
        let config = test_config();

        manager
            .create_session("session-1".to_string(), PathBuf::from("/tmp"), &config, None)
            .unwrap();

        let session = manager.get_session("session-1");
        assert!(session.is_some());
        assert_eq!(session.unwrap().session_id, "session-1");

        let missing = manager.get_session("nonexistent");
        assert!(missing.is_none());
    }

    #[test]
    fn test_manager_get_session_or_error() {
        let manager = SessionManager::new();
        let config = test_config();

        manager
            .create_session("session-1".to_string(), PathBuf::from("/tmp"), &config, None)
            .unwrap();

        let result = manager.get_session_or_error("session-1");
        assert!(result.is_ok());

        let error = manager.get_session_or_error("nonexistent");
        assert!(matches!(error, Err(AgentError::SessionNotFound(_))));
    }

    #[test]
    fn test_manager_remove_session() {
        let manager = SessionManager::new();
        let config = test_config();

        manager
            .create_session("session-1".to_string(), PathBuf::from("/tmp"), &config, None)
            .unwrap();

        assert!(manager.has_session("session-1"));

        let removed = manager.remove_session("session-1");
        assert!(removed.is_some());
        assert!(!manager.has_session("session-1"));
        assert_eq!(manager.session_count(), 0);
    }

    #[test]
    fn test_manager_duplicate_session() {
        let manager = SessionManager::new();
        let config = test_config();

        manager
            .create_session("session-1".to_string(), PathBuf::from("/tmp"), &config, None)
            .unwrap();

        let duplicate = manager.create_session(
            "session-1".to_string(),
            PathBuf::from("/tmp"),
            &config,
            None,
        );

        assert!(matches!(duplicate, Err(AgentError::SessionAlreadyExists(_))));
    }

    #[test]
    fn test_manager_session_ids() {
        let manager = SessionManager::new();
        let config = test_config();

        manager
            .create_session("session-1".to_string(), PathBuf::from("/tmp"), &config, None)
            .unwrap();
        manager
            .create_session("session-2".to_string(), PathBuf::from("/tmp"), &config, None)
            .unwrap();

        let ids = manager.session_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"session-1".to_string()));
        assert!(ids.contains(&"session-2".to_string()));
    }

    #[test]
    fn test_manager_clear() {
        let manager = SessionManager::new();
        let config = test_config();

        manager
            .create_session("session-1".to_string(), PathBuf::from("/tmp"), &config, None)
            .unwrap();
        manager
            .create_session("session-2".to_string(), PathBuf::from("/tmp"), &config, None)
            .unwrap();

        assert_eq!(manager.session_count(), 2);

        manager.clear();
        assert_eq!(manager.session_count(), 0);
    }

    #[test]
    fn test_manager_with_session() {
        let manager = SessionManager::new();
        let config = test_config();

        manager
            .create_session("session-1".to_string(), PathBuf::from("/tmp"), &config, None)
            .unwrap();

        let result = manager.with_session("session-1", |session| session.session_id.clone());

        assert_eq!(result, Some("session-1".to_string()));

        let missing = manager.with_session("nonexistent", |_| "found");
        assert!(missing.is_none());
    }
}
