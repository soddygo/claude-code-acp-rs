//! Settings manager implementation
//!
//! Handles loading, merging, and accessing settings from multiple sources.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::rule::PermissionSettings;
use crate::types::Result;

/// Settings file names
const USER_SETTINGS_DIR: &str = ".claude";
const PROJECT_SETTINGS_DIR: &str = ".claude";
const SETTINGS_FILE: &str = "settings.json";
const LOCAL_SETTINGS_FILE: &str = "settings.local.json";

/// Claude Code settings structure
///
/// This mirrors the settings structure used by Claude Code.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Custom system prompt additions
    #[serde(default)]
    pub system_prompt: Option<String>,

    /// Permission mode settings
    #[serde(default)]
    pub permission_mode: Option<String>,

    /// Model to use
    #[serde(default)]
    pub model: Option<String>,

    /// Small/fast model for quick operations
    #[serde(default)]
    pub small_fast_model: Option<String>,

    /// API base URL override
    #[serde(default)]
    pub api_base_url: Option<String>,

    /// Allowed tools list (legacy, use permissions instead)
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,

    /// Denied tools list (legacy, use permissions instead)
    #[serde(default)]
    pub denied_tools: Option<Vec<String>>,

    /// Permission settings with allow/deny/ask rules
    #[serde(default)]
    pub permissions: Option<PermissionSettings>,

    /// MCP servers configuration
    #[serde(default)]
    pub mcp_servers: Option<HashMap<String, McpServerConfig>>,

    /// Custom environment variables
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,

    /// Additional settings as raw JSON
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    /// Command to start the MCP server
    pub command: String,

    /// Arguments for the command
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables for the server
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,

    /// Whether the server is disabled
    #[serde(default)]
    pub disabled: bool,
}

impl Settings {
    /// Create empty settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Merge another settings into this one
    ///
    /// Values from `other` take precedence over `self`.
    pub fn merge(&mut self, other: Settings) {
        if other.system_prompt.is_some() {
            self.system_prompt = other.system_prompt;
        }
        if other.permission_mode.is_some() {
            self.permission_mode = other.permission_mode;
        }
        if other.model.is_some() {
            self.model = other.model;
        }
        if other.small_fast_model.is_some() {
            self.small_fast_model = other.small_fast_model;
        }
        if other.api_base_url.is_some() {
            self.api_base_url = other.api_base_url;
        }
        if other.allowed_tools.is_some() {
            self.allowed_tools = other.allowed_tools;
        }
        if other.denied_tools.is_some() {
            self.denied_tools = other.denied_tools;
        }
        // Merge permissions (combine rules from all sources)
        if let Some(other_perms) = other.permissions {
            let perms = self.permissions.get_or_insert_with(PermissionSettings::default);
            // Merge allow rules
            if let Some(other_allow) = other_perms.allow {
                let allow = perms.allow.get_or_insert_with(Vec::new);
                allow.extend(other_allow);
            }
            // Merge deny rules
            if let Some(other_deny) = other_perms.deny {
                let deny = perms.deny.get_or_insert_with(Vec::new);
                deny.extend(other_deny);
            }
            // Merge ask rules
            if let Some(other_ask) = other_perms.ask {
                let ask = perms.ask.get_or_insert_with(Vec::new);
                ask.extend(other_ask);
            }
            // Override additional_directories and default_mode
            if other_perms.additional_directories.is_some() {
                perms.additional_directories = other_perms.additional_directories;
            }
            if other_perms.default_mode.is_some() {
                perms.default_mode = other_perms.default_mode;
            }
        }
        if other.mcp_servers.is_some() {
            // Merge MCP servers
            let mut servers = self.mcp_servers.take().unwrap_or_default();
            if let Some(other_servers) = other.mcp_servers {
                for (name, config) in other_servers {
                    servers.insert(name, config);
                }
            }
            self.mcp_servers = Some(servers);
        }
        if other.env.is_some() {
            // Merge env vars
            let mut env = self.env.take().unwrap_or_default();
            if let Some(other_env) = other.env {
                for (key, value) in other_env {
                    env.insert(key, value);
                }
            }
            self.env = Some(env);
        }
        // Merge extra fields
        for (key, value) in other.extra {
            self.extra.insert(key, value);
        }
    }
}

/// Settings manager for loading and accessing settings
#[derive(Debug)]
pub struct SettingsManager {
    /// The merged settings
    settings: Settings,
    /// Project working directory
    project_dir: PathBuf,
}

impl SettingsManager {
    /// Create a new settings manager and load settings
    ///
    /// # Arguments
    ///
    /// * `project_dir` - The project working directory
    pub fn new(project_dir: impl AsRef<Path>) -> Result<Self> {
        let project_dir = project_dir.as_ref().to_path_buf();
        let settings = Self::load_all_settings(&project_dir);

        Ok(Self {
            settings,
            project_dir,
        })
    }

    /// Load and merge all settings sources
    ///
    /// Priority: Local > Project > User
    fn load_all_settings(project_dir: &Path) -> Settings {
        let mut settings = Settings::new();

        // 1. Load user settings (~/.claude/settings.json)
        if let Some(user_settings) = Self::load_user_settings() {
            tracing::debug!("Loaded user settings");
            settings.merge(user_settings);
        }

        // 2. Load project settings (.claude/settings.json)
        if let Some(project_settings) = Self::load_project_settings(project_dir) {
            tracing::debug!("Loaded project settings from {:?}", project_dir);
            settings.merge(project_settings);
        }

        // 3. Load local settings (.claude/settings.local.json)
        if let Some(local_settings) = Self::load_local_settings(project_dir) {
            tracing::debug!("Loaded local settings from {:?}", project_dir);
            settings.merge(local_settings);
        }

        settings
    }

    /// Load user settings from ~/.claude/settings.json
    fn load_user_settings() -> Option<Settings> {
        let home = dirs::home_dir()?;
        let path = home.join(USER_SETTINGS_DIR).join(SETTINGS_FILE);
        Self::load_settings_file(&path)
    }

    /// Load project settings from .claude/settings.json
    fn load_project_settings(project_dir: &Path) -> Option<Settings> {
        let path = project_dir.join(PROJECT_SETTINGS_DIR).join(SETTINGS_FILE);
        Self::load_settings_file(&path)
    }

    /// Load local settings from .claude/settings.local.json
    fn load_local_settings(project_dir: &Path) -> Option<Settings> {
        let path = project_dir
            .join(PROJECT_SETTINGS_DIR)
            .join(LOCAL_SETTINGS_FILE);
        Self::load_settings_file(&path)
    }

    /// Load settings from a file
    fn load_settings_file(path: &Path) -> Option<Settings> {
        if !path.exists() {
            return None;
        }

        match std::fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(settings) => Some(settings),
                Err(e) => {
                    tracing::warn!("Failed to parse settings file {:?}: {}", path, e);
                    None
                }
            },
            Err(e) => {
                tracing::warn!("Failed to read settings file {:?}: {}", path, e);
                None
            }
        }
    }

    /// Get the merged settings
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Get the project directory
    pub fn project_dir(&self) -> &Path {
        &self.project_dir
    }

    /// Reload settings from all sources
    pub fn reload(&mut self) {
        self.settings = Self::load_all_settings(&self.project_dir);
    }

    /// Get the system prompt if configured
    pub fn system_prompt(&self) -> Option<&str> {
        self.settings.system_prompt.as_deref()
    }

    /// Get the permission mode if configured
    pub fn permission_mode(&self) -> Option<&str> {
        self.settings.permission_mode.as_deref()
    }

    /// Get the model if configured
    pub fn model(&self) -> Option<&str> {
        self.settings.model.as_deref()
    }

    /// Get the small/fast model if configured
    pub fn small_fast_model(&self) -> Option<&str> {
        self.settings.small_fast_model.as_deref()
    }

    /// Get the API base URL if configured
    pub fn api_base_url(&self) -> Option<&str> {
        self.settings.api_base_url.as_deref()
    }

    /// Get MCP servers configuration
    pub fn mcp_servers(&self) -> Option<&HashMap<String, McpServerConfig>> {
        self.settings.mcp_servers.as_ref()
    }

    /// Get environment variables
    pub fn env(&self) -> Option<&HashMap<String, String>> {
        self.settings.env.as_ref()
    }

    /// Check if a tool is allowed
    pub fn is_tool_allowed(&self, tool_name: &str) -> bool {
        // If denied_tools is set and contains the tool, deny it
        if let Some(ref denied) = self.settings.denied_tools {
            if denied.iter().any(|t| t == tool_name) {
                return false;
            }
        }

        // If allowed_tools is set, check if tool is in the list
        if let Some(ref allowed) = self.settings.allowed_tools {
            return allowed.iter().any(|t| t == tool_name);
        }

        // Default: allow all tools
        true
    }
}

impl Default for SettingsManager {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            project_dir: PathBuf::from("."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_settings_default() {
        let settings = Settings::new();
        assert!(settings.system_prompt.is_none());
        assert!(settings.model.is_none());
        assert!(settings.mcp_servers.is_none());
    }

    #[test]
    fn test_settings_merge() {
        let mut base = Settings::new();
        base.model = Some("claude-3".to_string());
        base.system_prompt = Some("Base prompt".to_string());

        let mut override_settings = Settings::new();
        override_settings.model = Some("claude-4".to_string());
        override_settings.permission_mode = Some("acceptEdits".to_string());

        base.merge(override_settings);

        assert_eq!(base.model, Some("claude-4".to_string()));
        assert_eq!(base.system_prompt, Some("Base prompt".to_string()));
        assert_eq!(base.permission_mode, Some("acceptEdits".to_string()));
    }

    #[test]
    fn test_settings_merge_mcp_servers() {
        let mut base = Settings::new();
        let mut base_servers = HashMap::new();
        base_servers.insert(
            "server1".to_string(),
            McpServerConfig {
                command: "cmd1".to_string(),
                args: vec![],
                env: None,
                disabled: false,
            },
        );
        base.mcp_servers = Some(base_servers);

        let mut override_settings = Settings::new();
        let mut override_servers = HashMap::new();
        override_servers.insert(
            "server2".to_string(),
            McpServerConfig {
                command: "cmd2".to_string(),
                args: vec![],
                env: None,
                disabled: false,
            },
        );
        override_settings.mcp_servers = Some(override_servers);

        base.merge(override_settings);

        let servers = base.mcp_servers.unwrap();
        assert_eq!(servers.len(), 2);
        assert!(servers.contains_key("server1"));
        assert!(servers.contains_key("server2"));
    }

    #[test]
    fn test_settings_manager_new() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SettingsManager::new(temp_dir.path()).unwrap();

        // Should load settings (may include user settings from ~/.claude)
        // Just verify the manager was created successfully
        assert_eq!(manager.project_dir(), temp_dir.path());
    }

    #[test]
    fn test_settings_manager_load_project_settings() {
        let temp_dir = TempDir::new().unwrap();
        let settings_dir = temp_dir.path().join(".claude");
        std::fs::create_dir_all(&settings_dir).unwrap();

        let settings_file = settings_dir.join("settings.json");
        let mut file = std::fs::File::create(&settings_file).unwrap();
        writeln!(
            file,
            r#"{{
            "model": "claude-opus",
            "systemPrompt": "You are helpful"
        }}"#
        )
        .unwrap();

        let manager = SettingsManager::new(temp_dir.path()).unwrap();

        assert_eq!(manager.model(), Some("claude-opus"));
        assert_eq!(manager.system_prompt(), Some("You are helpful"));
    }

    #[test]
    fn test_settings_manager_local_overrides_project() {
        let temp_dir = TempDir::new().unwrap();
        let settings_dir = temp_dir.path().join(".claude");
        std::fs::create_dir_all(&settings_dir).unwrap();

        // Create project settings
        let project_settings = settings_dir.join("settings.json");
        let mut file = std::fs::File::create(&project_settings).unwrap();
        writeln!(
            file,
            r#"{{
            "model": "claude-opus",
            "systemPrompt": "Project prompt"
        }}"#
        )
        .unwrap();

        // Create local settings (higher priority)
        let local_settings = settings_dir.join("settings.local.json");
        let mut file = std::fs::File::create(&local_settings).unwrap();
        writeln!(
            file,
            r#"{{
            "model": "claude-sonnet"
        }}"#
        )
        .unwrap();

        let manager = SettingsManager::new(temp_dir.path()).unwrap();

        // Local model should override project
        assert_eq!(manager.model(), Some("claude-sonnet"));
        // System prompt from project should remain
        assert_eq!(manager.system_prompt(), Some("Project prompt"));
    }

    #[test]
    fn test_is_tool_allowed() {
        let mut settings = Settings::new();
        let manager = SettingsManager {
            settings: settings.clone(),
            project_dir: PathBuf::from("."),
        };

        // Default: all tools allowed
        assert!(manager.is_tool_allowed("Read"));
        assert!(manager.is_tool_allowed("Write"));

        // With allowed list
        settings.allowed_tools = Some(vec!["Read".to_string(), "Edit".to_string()]);
        let manager = SettingsManager {
            settings: settings.clone(),
            project_dir: PathBuf::from("."),
        };
        assert!(manager.is_tool_allowed("Read"));
        assert!(!manager.is_tool_allowed("Write"));

        // With denied list
        settings.allowed_tools = None;
        settings.denied_tools = Some(vec!["Bash".to_string()]);
        let manager = SettingsManager {
            settings,
            project_dir: PathBuf::from("."),
        };
        assert!(manager.is_tool_allowed("Read"));
        assert!(!manager.is_tool_allowed("Bash"));
    }

    #[test]
    fn test_settings_manager_reload() {
        let temp_dir = TempDir::new().unwrap();
        let settings_dir = temp_dir.path().join(".claude");
        std::fs::create_dir_all(&settings_dir).unwrap();

        let settings_file = settings_dir.join("settings.json");
        let mut file = std::fs::File::create(&settings_file).unwrap();
        writeln!(file, r#"{{"model": "claude-opus"}}"#).unwrap();

        let mut manager = SettingsManager::new(temp_dir.path()).unwrap();
        assert_eq!(manager.model(), Some("claude-opus"));

        // Update the file
        let mut file = std::fs::File::create(&settings_file).unwrap();
        writeln!(file, r#"{{"model": "claude-sonnet"}}"#).unwrap();

        // Reload
        manager.reload();
        assert_eq!(manager.model(), Some("claude-sonnet"));
    }
}
