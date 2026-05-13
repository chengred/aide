use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::llm::ProviderType;

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub general: GeneralConfig,
    pub providers: ProviderConfigs,
    pub tools: ToolConfig,
    pub ui: UiConfig,
    #[serde(default)]
    pub mode: Option<OperationMode>,
    #[serde(default)]
    pub profile: Option<String>,
}

/// Operation mode for privacy/performance tradeoffs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OperationMode {
    /// Use only local models, zero data leaves the machine
    Local,
    /// Use cloud APIs for all queries
    Cloud,
    /// Auto-route: simple queries to local, complex to cloud
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub default_provider: ProviderType,
    pub default_model: String,
    pub system_prompt: Option<String>,
    pub max_conversation_turns: u32,
    pub token_budget: Option<u32>,
    #[serde(default = "default_enable_planning")]
    pub enable_planning: Option<bool>,
}

fn default_enable_planning() -> Option<bool> {
    Some(true)
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            default_provider: ProviderType::DeepSeek,
            default_model: "deepseek-chat".into(),
            system_prompt: Some(
                "你是一个专业的 AI 编程助手。你帮助用户完成软件工程任务。"
                    .into(),
            ),
            max_conversation_turns: 100,
            token_budget: None,
            enable_planning: Some(true),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfigs {
    pub openai: Option<OpenAIConfig>,
    pub anthropic: Option<AnthropicConfig>,
    pub deepseek: Option<DeepSeekConfig>,
    pub ollama: Option<OllamaConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIConfig {
    pub api_key: String,
    pub base_url: Option<String>,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekConfig {
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    pub base_url: String,
    pub model: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".into(),
            model: "codellama".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    pub allowed_tools: Vec<String>,
    pub require_approval: Vec<String>,
}

impl Default for ToolConfig {
    fn default() -> Self {
        Self {
            allowed_tools: vec![
                "read".into(),
                "write".into(),
                "edit".into(),
                "grep".into(),
                "glob".into(),
            ],
            require_approval: vec!["bash".into(), "run".into()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub theme: String,
    pub show_tokens: bool,
    pub show_tool_calls: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "default".into(),
            show_tokens: true,
            show_tool_calls: true,
        }
    }
}

impl Config {
    /// Load config, checking multiple paths in order:
    /// 1. Custom path (from -c flag)
    /// 2. ./rustcc.toml in current directory
    /// 3. ./config.toml in current directory
    /// 4. Global %APPDATA%/rustcc/config.toml
    pub fn load(custom_path: Option<&str>) -> Result<Self, ConfigError> {
        let path = find_config_path(custom_path)?;
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let mut config: Config = toml::from_str(&content)?;

            let profile_name = config.profile.clone();
            if let Some(profile) = profile_name {
                config.apply_profile(&profile);
            }

            Ok(config)
        } else {
            Err(ConfigError::NotFound("No config file found. Run 'rustcc cfg init' to create one.".into()))
        }
    }

    /// Check if a config file exists without loading
    pub fn exists(custom_path: Option<&str>) -> bool {
        find_config_path(custom_path).map(|p| p.exists()).unwrap_or(false)
    }

    /// Save config — writes to custom path if specified, else global path
    pub fn save_to_path(&self, custom_path: Option<&str>) -> Result<(), ConfigError> {
        let path = find_or_create_config_dir(custom_path)?;
        save_to(self, &path)
    }

    /// Apply a named profile preset
    pub fn apply_profile(&mut self, profile: &str) {
        match profile {
            "deepseek" => {
                self.mode = Some(OperationMode::Cloud);
                self.general.default_provider = ProviderType::DeepSeek;
                self.general.default_model = "deepseek-chat".into();
                self.general.system_prompt = Some(
                    "你是一个专业的 AI 编程助手，由 DeepSeek 驱动。".into()
                );
            }
            "openai" => {
                self.mode = Some(OperationMode::Cloud);
                self.general.default_provider = ProviderType::OpenAI;
                self.general.default_model = "gpt-4o".into();
                self.general.token_budget = Some(200_000);
                self.general.system_prompt = Some(
                    "你是一个专业的 AI 编程助手，由 OpenAI 驱动。".into()
                );
            }
            "anthropic" => {
                self.mode = Some(OperationMode::Cloud);
                self.general.default_provider = ProviderType::Anthropic;
                self.general.default_model = "claude-sonnet-4-6".into();
                self.general.system_prompt = Some(
                    "你是一个专业的 AI 编程助手，由 Anthropic Claude 驱动。".into()
                );
            }
            "privacy-first" => {
                self.mode = Some(OperationMode::Local);
                self.general.default_provider = ProviderType::Ollama;
                self.general.default_model = "codellama".into();
                self.providers.openai = None;
                self.providers.anthropic = None;
                self.providers.deepseek = None;
                self.tools.require_approval = vec!["bash".into(), "run".into(), "write".into(), "edit".into()];
                self.general.system_prompt = Some(
                    "你是一个注重隐私的 AI 编程助手，完全离线运行。你的数据不会离开本机。".into()
                );
            }
            "cloud-max" => {
                self.mode = Some(OperationMode::Cloud);
                self.general.default_provider = ProviderType::OpenAI;
                self.general.default_model = "gpt-4o".into();
                self.general.token_budget = Some(200_000);
                self.general.system_prompt = Some(
                    "你是一个强大的 AI 编程助手，可使用云端模型获得最大能力。".into()
                );
            }
            _ => {}
        }
    }

    /// Set the operation mode
    #[allow(dead_code)]
    pub fn set_mode(&mut self, mode: OperationMode) {
        self.mode = Some(mode);
    }

    /// Check if local-only mode is active
    #[allow(dead_code)]
    pub fn is_local_only(&self) -> bool {
        matches!(self.mode, Some(OperationMode::Local))
    }

    /// Save config to the default path (current dir rustcc.toml)
    pub fn save(&self) -> Result<(), ConfigError> {
        self.save_to_path(None)
    }

    /// Save config to the global path
    pub fn save_global(&self) -> Result<(), ConfigError> {
        let path = config_dir_path()?;
        save_to(self, &path)
    }

    /// Get the API key for a given provider
    #[allow(dead_code)]
    pub fn get_api_key(&self, provider: &ProviderType) -> Option<&str> {
        match provider {
            ProviderType::OpenAI => self.providers.openai.as_ref().map(|c| c.api_key.as_str()),
            ProviderType::Anthropic => self.providers.anthropic.as_ref().map(|c| c.api_key.as_str()),
            ProviderType::DeepSeek => self.providers.deepseek.as_ref().map(|c| c.api_key.as_str()),
            ProviderType::Ollama => None,
            ProviderType::Candle => None,
        }
    }

    /// Get the default model for a given provider
    pub fn get_default_model(&self, provider: &ProviderType) -> String {
        match provider {
            ProviderType::OpenAI => self
                .providers
                .openai
                .as_ref()
                .map(|c| c.model.clone())
                .unwrap_or_else(|| "gpt-4o".into()),
            ProviderType::Anthropic => self
                .providers
                .anthropic
                .as_ref()
                .map(|c| c.model.clone())
                .unwrap_or_else(|| "claude-sonnet-4-6".into()),
            ProviderType::DeepSeek => self
                .providers
                .deepseek
                .as_ref()
                .map(|c| c.model.clone())
                .unwrap_or_else(|| "deepseek-chat".into()),
            ProviderType::Ollama => self
                .providers
                .ollama
                .as_ref()
                .map(|c| c.model.clone())
                .unwrap_or_else(|| "codellama".into()),
            ProviderType::Candle => "local".into(),
        }
    }
}

/// Search for an existing config file, checking:
/// 1. Custom path (from -c flag)
/// 2. ./rustcc.toml
/// 3. ./config.toml
/// 4. Global %APPDATA%/rustcc/config.toml
fn find_config_path(custom: Option<&str>) -> Result<PathBuf, ConfigError> {
    if let Some(path) = custom {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
        return Err(ConfigError::NotFound(format!("Config not found: {}", path)));
    }

    // Check local files
    for local in &["rustcc.toml", "config.toml"] {
        let p = PathBuf::from(local);
        if p.exists() {
            return Ok(p);
        }
    }

    // Fall back to global
    let global = dirs::config_dir()
        .ok_or_else(|| ConfigError::NotFound("config directory not found".into()))?
        .join("rustcc")
        .join("config.toml");
    Ok(global)
}

/// Find where to write a new config:
/// 1. Custom path (directory must exist or be creatable)
/// 2. Current directory (prefer ./rustcc.toml)
/// 3. Global config dir
fn find_or_create_config_dir(custom: Option<&str>) -> Result<PathBuf, ConfigError> {
    if let Some(path) = custom {
        let p = PathBuf::from(path);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        return Ok(p);
    }
    Ok(PathBuf::from("rustcc.toml"))
}

fn config_dir_path() -> Result<PathBuf, ConfigError> {
    let dir = dirs::config_dir()
        .ok_or_else(|| ConfigError::NotFound("config directory not found".into()))?;
    Ok(dir.join("rustcc").join("config.toml"))
}

fn save_to(config: &Config, path: &PathBuf) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(config)?;
    std::fs::write(path, content)?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::ser::Error),
    #[error("TOML deserialize error: {0}")]
    TomlDe(#[from] toml::de::Error),
    #[error("Config not found: {0}")]
    NotFound(String),
}
