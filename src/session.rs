use std::sync::Arc;

use anyhow;
use colored::Colorize;
use tokio::sync::mpsc;

use crate::agent::{Agent, AgentConfig, AgentEvent};
use crate::agent::context::ContextManager;
use crate::cli::Cli;
use crate::llm::{LLMProvider, Message};
use crate::llm::{
    AnthropicProvider, DeepSeekProvider, OllamaProvider, OpenAIProvider,
};
use crate::llm::ProviderType;
use crate::storage::config::Config;
use crate::storage::settings::SettingsManager;
use crate::tools::permission::PermissionManager;
use crate::tools::ToolRegistry;

/// Manages a single chat session
pub struct Session {
    config: Config,
    provider: Box<dyn LLMProvider>,
    agent: Agent,
    context_manager: ContextManager,
    messages: Vec<Message>,
    current_provider_type: ProviderType,
    current_model: String,
    settings_manager: SettingsManager,
}

impl Session {
    pub fn new(cli: &Cli) -> Result<Self, anyhow::Error> {
        let config = Config::load()?;

        let provider_type = cli
            .provider
            .as_deref()
            .map(|s| s.parse::<ProviderType>())
            .transpose()
            .map_err(|e: String| anyhow::anyhow!(e))?
            .unwrap_or_else(|| config.general.default_provider.clone());

        let model = cli
            .model
            .clone()
            .unwrap_or_else(|| config.get_default_model(&provider_type));

        let provider = create_provider(&config, &provider_type, &model)?;

        // Build tool registry with all builtin tools
        let mut tool_registry = ToolRegistry::new();
        crate::tools::builtin::register_all(&mut tool_registry);
        // Register planning tool when enabled
        if config.general.enable_planning.unwrap_or(true) {
            tool_registry.register(crate::tools::planning::PlanTool::new());
        }

        // Load persistent settings
        let settings_manager = SettingsManager::new().unwrap_or_else(|_| {
            // Fallback: create with default paths
            SettingsManager::new().unwrap()
        });
        let settings = settings_manager.load().unwrap_or_default();

        // Build permission manager from config + persistent settings
        let mut perm_manager = PermissionManager::new();
        // Apply config allowlist
        for tool_name in &config.tools.allowed_tools {
            perm_manager.allow_tool(tool_name);
        }
        // Apply persistent settings allowlist
        for tool_name in &settings.permissions.allow {
            perm_manager.allow_tool(tool_name);
        }
        // Apply persistent settings denylist
        for tool_name in &settings.permissions.deny {
            perm_manager.block_tool(tool_name);
        }
        // Set allowed paths from settings
        for path in &settings.permissions.additional_directories {
            perm_manager.allow_path(path);
        }

        // Build agent config
        let memory_store = crate::agent::memory::MemoryStore::open().ok();
        let agent_config = AgentConfig {
            max_turns: config.general.max_conversation_turns,
            system_prompt: config.general.system_prompt.clone().unwrap_or_default(),
            temperature: 0.7,
            max_tokens: 4096,
            enable_planning: true,
            show_tool_calls: config.ui.show_tool_calls,
            enable_memory: true,
            memory_store,
        };

        let agent = Agent::new(agent_config, tool_registry, perm_manager);
        let context_manager = ContextManager::default();

        let mut messages = Vec::new();
        if let Some(ref system_prompt) = config.general.system_prompt {
            messages.push(Message::system(system_prompt.as_str()));
        }

        Ok(Self {
            config,
            provider,
            agent,
            context_manager,
            messages,
            current_provider_type: provider_type,
            current_model: model,
            settings_manager,
        })
    }

    /// Run a single prompt and return
    pub async fn run_once(&mut self, prompt: &str) -> Result<(), anyhow::Error> {
        self.messages.push(Message::user(prompt));

        let result = self
            .agent
            .run(
                self.provider.as_ref(),
                &self.messages,
                &self.current_model,
            )
            .await;

        println!("{}", result.content);

        if !result.tools_called.is_empty() {
            eprintln!(
                "{}",
                format!("(tools used: {})", result.tools_called.join(", ")).dimmed()
            );
        }

        Ok(())
    }

    /// Run the session in TUI mode
    pub async fn run_tui(self) -> Result<(), anyhow::Error> {
        let (user_tx, mut user_rx) = mpsc::unbounded_channel::<String>();
        let (event_tx, event_rx) = mpsc::unbounded_channel::<AgentEvent>();

        let provider: Arc<dyn LLMProvider> = Arc::from(self.provider);
        let agent_config = self.agent.config().clone();
        let tool_registry = self.agent.tool_registry().clone();
        let perm_manager = self.agent.permission_manager().clone();
        let model = self.current_model.clone();

        let mut messages: Vec<Message> = self.messages.clone();

        // Build the TUI app
        let app = crate::tui::app::App::new(
            self.current_model.clone(),
            self.current_provider_type.to_string(),
        );

        // Spawn the agent handler
        tokio::spawn(async move {
            while let Some(user_msg) = user_rx.recv().await {
                messages.push(Message::user(&user_msg));

                let agent = Agent::new(
                    agent_config.clone(),
                    tool_registry.clone(),
                    perm_manager.clone(),
                );

                let result = agent
                    .run_streaming(
                        provider.as_ref(),
                        &messages,
                        &model,
                        event_tx.clone(),
                    )
                    .await;

                messages.push(Message::assistant(&result.content));
            }
        });

        // Run the TUI on the main thread (blocking)
        crate::tui::run_tui(app, user_tx, event_rx)?;

        Ok(())
    }

    pub fn current_provider_type(&self) -> ProviderType {
        self.current_provider_type.clone()
    }

    pub fn models(&self) -> Vec<String> {
        self.provider.models()
    }

}

fn create_provider(
    config: &Config,
    provider_type: &ProviderType,
    model: &str,
) -> Result<Box<dyn LLMProvider>, anyhow::Error> {
    match provider_type {
        ProviderType::OpenAI => {
            let conf = config.providers.openai.as_ref().ok_or_else(|| {
                anyhow::anyhow!("OpenAI not configured. Set openai.api_key in config.toml")
            })?;
            Ok(Box::new(OpenAIProvider::new(
                &conf.api_key,
                conf.base_url.clone(),
                model,
            )))
        }
        ProviderType::Anthropic => {
            let conf = config.providers.anthropic.as_ref().ok_or_else(|| {
                anyhow::anyhow!("Anthropic not configured. Set anthropic.api_key in config.toml")
            })?;
            Ok(Box::new(AnthropicProvider::new(&conf.api_key, model)))
        }
        ProviderType::DeepSeek => {
            let conf = config.providers.deepseek.as_ref().ok_or_else(|| {
                anyhow::anyhow!("DeepSeek not configured. Set deepseek.api_key in config.toml")
            })?;
            Ok(Box::new(DeepSeekProvider::new(&conf.api_key, model)))
        }
        ProviderType::Ollama => {
            let base_url = config
                .providers
                .ollama
                .as_ref()
                .map(|c| c.base_url.clone())
                .unwrap_or_else(|| "http://localhost:11434".into());
            Ok(Box::new(OllamaProvider::new(base_url, model)))
        }
        ProviderType::Candle => Err(anyhow::anyhow!("Candle provider not yet implemented")),
    }
}
