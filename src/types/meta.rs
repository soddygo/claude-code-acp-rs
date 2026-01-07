//! Meta field parsing for ACP requests
//!
//! ACP protocol's `new_session` and `load_session` requests support a `_meta` field
//! for passing additional configuration.

use serde::{Deserialize, Serialize};

/// System prompt configuration from meta field
///
/// Allows clients to customize the system prompt via the `_meta.systemPrompt` field.
///
/// # JSON Structure
///
/// ```json
/// {
///   "_meta": {
///     "systemPrompt": {
///       "append": "Additional instructions..."
///     }
///   }
/// }
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemPromptMeta {
    /// Text to append to the system prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub append: Option<String>,

    /// Text to replace the entire system prompt (higher priority than append)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace: Option<String>,
}

impl SystemPromptMeta {
    /// Parse from a JSON value (the `_meta` object)
    pub fn from_meta(meta: &serde_json::Value) -> Option<Self> {
        meta.get("systemPrompt")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Check if any system prompt modification is configured
    pub fn is_configured(&self) -> bool {
        self.append.is_some() || self.replace.is_some()
    }
}

/// Claude Code specific options from meta field
///
/// # JSON Structure
///
/// ```json
/// {
///   "_meta": {
///     "claudeCode": {
///       "options": {
///         "resume": "session-uuid-12345"
///       }
///     }
///   }
/// }
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClaudeCodeOptions {
    /// Session ID to resume
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume: Option<String>,
}

/// Claude Code meta configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClaudeCodeMeta {
    /// Claude Code options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<ClaudeCodeOptions>,
}

impl ClaudeCodeMeta {
    /// Parse from a JSON value (the `_meta` object)
    pub fn from_meta(meta: &serde_json::Value) -> Option<Self> {
        meta.get("claudeCode")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Get the session ID to resume, if any
    pub fn get_resume_session_id(&self) -> Option<&str> {
        self.options.as_ref()?.resume.as_deref()
    }
}

/// Combined meta configuration for new session requests
///
/// Parses all supported meta fields from ACP request's `_meta` field.
#[derive(Debug, Clone, Default)]
pub struct NewSessionMeta {
    /// System prompt configuration
    pub system_prompt: Option<SystemPromptMeta>,

    /// Claude Code specific configuration
    pub claude_code: Option<ClaudeCodeMeta>,

    /// Whether to disable built-in tools
    pub disable_built_in_tools: bool,
}

impl NewSessionMeta {
    /// Parse from ACP request's `_meta` field
    ///
    /// # Arguments
    ///
    /// * `meta` - The `_meta` field from the ACP request (optional)
    ///
    /// # Returns
    ///
    /// A `NewSessionMeta` with all parsed fields, or defaults if meta is None
    pub fn from_request_meta(meta: Option<&serde_json::Value>) -> Self {
        let Some(meta) = meta else {
            return Self::default();
        };

        Self {
            system_prompt: SystemPromptMeta::from_meta(meta),
            claude_code: ClaudeCodeMeta::from_meta(meta),
            disable_built_in_tools: meta
                .get("disableBuiltInTools")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        }
    }

    /// Get the text to append to the system prompt, if any
    pub fn get_system_prompt_append(&self) -> Option<&str> {
        self.system_prompt.as_ref()?.append.as_deref()
    }

    /// Get the text to replace the system prompt, if any
    pub fn get_system_prompt_replace(&self) -> Option<&str> {
        self.system_prompt.as_ref()?.replace.as_deref()
    }

    /// Get the session ID to resume, if any
    pub fn get_resume_session_id(&self) -> Option<&str> {
        self.claude_code.as_ref()?.get_resume_session_id()
    }

    /// Check if this session should resume from a previous session
    pub fn should_resume(&self) -> bool {
        self.get_resume_session_id().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_system_prompt_meta_parse() {
        let meta = json!({
            "systemPrompt": {
                "append": "Please respond in Chinese"
            }
        });

        let parsed = SystemPromptMeta::from_meta(&meta).unwrap();
        assert_eq!(parsed.append, Some("Please respond in Chinese".to_string()));
        assert!(parsed.replace.is_none());
        assert!(parsed.is_configured());
    }

    #[test]
    fn test_claude_code_meta_parse() {
        let meta = json!({
            "claudeCode": {
                "options": {
                    "resume": "session-uuid-12345"
                }
            }
        });

        let parsed = ClaudeCodeMeta::from_meta(&meta).unwrap();
        assert_eq!(parsed.get_resume_session_id(), Some("session-uuid-12345"));
    }

    #[test]
    fn test_new_session_meta_full() {
        let meta = json!({
            "systemPrompt": {
                "append": "Be concise"
            },
            "claudeCode": {
                "options": {
                    "resume": "abc-123"
                }
            },
            "disableBuiltInTools": true
        });

        let parsed = NewSessionMeta::from_request_meta(Some(&meta));
        assert_eq!(parsed.get_system_prompt_append(), Some("Be concise"));
        assert_eq!(parsed.get_resume_session_id(), Some("abc-123"));
        assert!(parsed.disable_built_in_tools);
        assert!(parsed.should_resume());
    }

    #[test]
    fn test_new_session_meta_empty() {
        let parsed = NewSessionMeta::from_request_meta(None);
        assert!(parsed.system_prompt.is_none());
        assert!(parsed.claude_code.is_none());
        assert!(!parsed.disable_built_in_tools);
        assert!(!parsed.should_resume());
    }

    #[test]
    fn test_new_session_meta_partial() {
        let meta = json!({
            "systemPrompt": {
                "replace": "You are a helpful assistant"
            }
        });

        let parsed = NewSessionMeta::from_request_meta(Some(&meta));
        assert_eq!(
            parsed.get_system_prompt_replace(),
            Some("You are a helpful assistant")
        );
        assert!(parsed.get_system_prompt_append().is_none());
        assert!(parsed.get_resume_session_id().is_none());
    }
}
