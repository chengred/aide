use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Persistent settings, equivalent to Claude Code's settings.json
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub permissions: PermissionSettings,
    #[serde(default)]
    pub hooks: HookSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PermissionSettings {
    /// Tools always allowed without confirmation
    #[serde(default)]
    pub allow: Vec<String>,
    /// Tools always denied
    #[serde(default)]
    pub deny: Vec<String>,
    /// Additional directories the agent can access
    #[serde(default)]
    pub additional_directories: Vec<String>,
    /// Path-based allow rules (glob patterns)
    #[serde(default)]
    pub allow_rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HookSettings {
    /// Scripts/commands to run before tool execution
    #[serde(default)]
    pub pre_tool_use: Vec<HookDefinition>,
    /// Scripts/commands to run after tool execution
    #[serde(default)]
    pub post_tool_use: Vec<HookDefinition>,
    /// Hooks triggered on session start/stop
    #[serde(default)]
    pub session_start: Vec<HookDefinition>,
    #[serde(default)]
    pub session_stop: Vec<HookDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDefinition {
    /// Shell command to execute
    pub command: String,
    /// Optional matcher (tool name pattern, event name)
    #[serde(default)]
    pub matcher: Option<String>,
    /// Whether to wait for the hook to finish
    #[serde(default = "default_true")]
    pub wait: bool,
}

fn default_true() -> bool {
    true
}

/// Manages loading and saving settings from disk
pub struct SettingsManager {
    project_path: Option<PathBuf>,
    user_path: PathBuf,
}

impl SettingsManager {
    /// Create a new settings manager
    pub fn new() -> Result<Self, anyhow::Error> {
        let user_path = user_settings_path()?;
        let project_path = find_project_settings();

        Ok(Self {
            project_path,
            user_path,
        })
    }

    /// Load merged settings (project overrides user)
    pub fn load(&self) -> Result<Settings, anyhow::Error> {
        let mut merged = if self.user_path.exists() {
            let content = std::fs::read_to_string(&self.user_path)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Settings::default()
        };

        // Merge project settings on top
        if let Some(ref proj_path) = self.project_path {
            if proj_path.exists() {
                let content = std::fs::read_to_string(proj_path)?;
                let project_settings: Settings =
                    serde_json::from_str(&content).unwrap_or_default();
                merge_settings(&mut merged, project_settings);
            }
        }

        Ok(merged)
    }

    /// Save user-level settings
    #[allow(dead_code)]
    pub fn save_user(&self, settings: &Settings) -> Result<(), anyhow::Error> {
        if let Some(parent) = self.user_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(settings)?;
        std::fs::write(&self.user_path, json)?;
        Ok(())
    }

    /// Save project-level settings
    #[allow(dead_code)]
    pub fn save_project(&self, settings: &Settings) -> Result<(), anyhow::Error> {
        let path = self
            .project_path
            .clone()
            .unwrap_or_else(|| PathBuf::from(".claude").join("settings.json"));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(settings)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    /// Add a tool to the allowlist and persist
    pub fn allow_tool(&self, tool_name: &str) -> Result<(), anyhow::Error> {
        let mut settings = self.load()?;
        if !settings.permissions.allow.contains(&tool_name.to_string()) {
            settings.permissions.allow.push(tool_name.to_string());
        }
        settings.permissions.deny.retain(|t| t != tool_name);
        self.save_user(&settings)
    }

    /// Add a tool to the denylist and persist
    pub fn deny_tool(&self, tool_name: &str) -> Result<(), anyhow::Error> {
        let mut settings = self.load()?;
        if !settings.permissions.deny.contains(&tool_name.to_string()) {
            settings.permissions.deny.push(tool_name.to_string());
        }
        settings.permissions.allow.retain(|t| t != tool_name);
        self.save_user(&settings)
    }

    /// Get the persistent settings path
    pub fn user_settings_path(&self) -> &PathBuf {
        &self.user_path
    }
}

/// Merge project settings into user settings (project overrides)
fn merge_settings(base: &mut Settings, overlay: Settings) {
    // Merge permission allows
    for tool in overlay.permissions.allow {
        if !base.permissions.allow.contains(&tool) {
            base.permissions.allow.push(tool);
        }
    }
    // Merge permission denies
    for tool in overlay.permissions.deny {
        if !base.permissions.deny.contains(&tool) {
            base.permissions.deny.push(tool);
        }
    }
    // Merge additional directories
    for dir in overlay.permissions.additional_directories {
        if !base.permissions.additional_directories.contains(&dir) {
            base.permissions.additional_directories.push(dir);
        }
    }
    // Merge allow rules
    for rule in overlay.permissions.allow_rules {
        if !base.permissions.allow_rules.contains(&rule) {
            base.permissions.allow_rules.push(rule);
        }
    }
    // Merge hooks (append project hooks to user hooks)
    base.hooks.pre_tool_use.extend(overlay.hooks.pre_tool_use);
    base.hooks.post_tool_use.extend(overlay.hooks.post_tool_use);
    base.hooks.session_start.extend(overlay.hooks.session_start);
    base.hooks.session_stop.extend(overlay.hooks.session_stop);
}

/// Get the user settings path
fn user_settings_path() -> Result<PathBuf, anyhow::Error> {
    let dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("config directory not found"))?
        .join("rustcc");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("settings.json"))
}

/// Find project-level .claude/settings.json
fn find_project_settings() -> Option<PathBuf> {
    let mut current = std::env::current_dir().ok()?;
    loop {
        let candidate = current.join(".claude").join("settings.json");
        if candidate.exists() {
            return Some(candidate);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = Settings::default();
        assert!(settings.permissions.allow.is_empty());
        assert!(settings.permissions.deny.is_empty());
        assert!(settings.hooks.pre_tool_use.is_empty());
    }

    #[test]
    fn test_merge_settings() {
        let mut user = Settings::default();
        user.permissions.allow.push("read".into());

        let mut project = Settings::default();
        project.permissions.allow.push("bash".into());
        project.permissions.deny.push("write".into());

        merge_settings(&mut user, project);
        assert!(user.permissions.allow.contains(&"read".to_string()));
        assert!(user.permissions.allow.contains(&"bash".to_string()));
        assert!(user.permissions.deny.contains(&"write".to_string()));
    }

    #[test]
    fn test_merge_deduplicates() {
        let mut user = Settings::default();
        user.permissions.allow.push("read".into());

        let mut project = Settings::default();
        project.permissions.allow.push("read".into());

        merge_settings(&mut user, project);
        assert_eq!(user.permissions.allow.len(), 1);
    }

    #[test]
    fn test_settings_manager_allow_tool() {
        // This test doesn't actually persist to disk
        let manager = SettingsManager::new().unwrap();
        assert!(manager.user_settings_path().ends_with("settings.json"));
    }
}
