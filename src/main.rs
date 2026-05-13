mod agent;
mod cli;
mod llm;
mod services;
mod session;
mod storage;
mod tools;
mod tui;
mod utils;

use clap::Parser;
use cli::{Cli, Commands};
use colored::Colorize;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rustcc=info".into()),
        )
        .init();

    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Run { prompt, .. }) => {
            let mut session = session::Session::new(&cli)?;
            session.run_once(prompt).await?;
        }
        Some(Commands::Chat { prompt }) => {
            let mut session = session::Session::new(&cli)?;
            if let Some(p) = prompt {
                session.run_once(p).await?;
            }
            let session2 = session::Session::new(&cli)?;
            session2.run_tui().await?;
        }
        Some(Commands::Cfg { action }) => {
            match action {
                Some(cli::ConfigAction::Show) | None => {
                    match storage::config::Config::load(cli.config.as_deref()) {
                        Ok(config) => {
                            let toml_str = toml::to_string_pretty(&config)?;
                            println!("{}", toml_str);
                        }
                        Err(_) => {
                            println!("No config found. Run 'rustcc cfg init' to create one.");
                        }
                    }
                }
                Some(cli::ConfigAction::Init { local }) => {
                    let config = storage::config::Config::default();
                    if *local {
                        config.save()?;
                        println!("配置已创建: rustcc.toml (当前目录)");
                    } else {
                        config.save_global()?;
                        println!("配置已创建: {}",
                            dirs::config_dir().unwrap().join("rustcc").join("config.toml").display());
                    }
                    println!("编辑该文件填入 API Key，然后运行 rustcc 即可。");
                }
                Some(cli::ConfigAction::Set { .. }) => {
                    println!("使用 'rustcc cfg init' 创建默认配置后，直接编辑配置文件即可。");
                }
            }
        }
        Some(Commands::List) => {
            let session = session::Session::new(&cli)?;
            println!("当前提供商: {}", session.current_provider_type().to_string());
            println!("可用模型:");
            for m in session.models() {
                println!("  {}", m);
            }
        }
        Some(Commands::Tool) => {
            let mut registry = tools::ToolRegistry::new();
            tools::builtin::register_all(&mut registry);
            println!("可用工具 ({}):", registry.names().len());
            for name in &registry.names() {
                let approval = if registry.requires_approval(name) {
                    " [需确认]"
                } else {
                    ""
                };
                println!("  {}{}", name, approval);
            }
        }
        Some(Commands::Hist { action }) => {
            let history = storage::history::HistoryManager::new()?;
            match action.as_deref() {
                Some("list") | None => {
                    let sessions = history.list()?;
                    if sessions.is_empty() {
                        println!("没有保存的会话。");
                    } else {
                        println!("已保存的会话 ({}):", sessions.len());
                        for s in &sessions {
                            println!(
                                "  {} | {} | {} | {} 条消息 | {}",
                                s.id[..8].dimmed(),
                                s.title,
                                s.model,
                                s.message_count,
                                s.updated_at
                            );
                        }
                    }
                }
                Some(id) if id.starts_with("load:") => {
                    let sid = id.strip_prefix("load:").unwrap();
                    match history.load(sid) {
                        Ok(record) => {
                            println!("已加载会话: {}", record.title);
                            println!("  消息数: {}", record.message_count);
                            for (i, msg) in record.messages.iter().enumerate() {
                                println!("  [{}] {}: {}", i, msg.role, msg.content.chars().take(200).collect::<String>());
                            }
                        }
                        Err(e) => eprintln!("错误: {}", e),
                    }
                }
                Some(id) => {
                    match history.load(id) {
                        Ok(record) => {
                            println!("会话: {}", record.title);
                            println!("  模型: {}", record.model);
                            println!("  消息数: {}", record.message_count);
                            println!("  创建时间: {}", record.created_at);
                        }
                        Err(e) => eprintln!("错误: {}", e),
                    }
                }
            }
        }
        Some(Commands::Rag { action, path }) => {
            let mut engine = services::rag::RagEngine::new();
            match action.as_deref() {
                Some("index") => {
                    let p = path.as_deref().unwrap_or(".");
                    index_directory(&mut engine, p);
                    println!("已索引 {} 个文档。", engine.len());
                }
                Some("search") => {
                    println!("使用 'rustcc' 启动后在对话中使用 RAG 搜索。");
                }
                _ => {
                    println!("RAG 命令:");
                    println!("  rustcc rag index [path]  - 索引代码文件");
                    println!("  rustcc rag search <关键词> - 搜索已索引的代码");
                }
            }
        }
        Some(Commands::Mcp { action }) => {
            let manager = services::mcp::McpManager::new()?;
            match action.as_deref() {
                Some("list") => {
                    let servers = manager.list_servers();
                    if servers.is_empty() {
                        println!("没有配置 MCP 服务器。");
                        println!("在以下位置添加: {}", manager.configs_path());
                    } else {
                        println!("MCP 服务器:");
                        for s in servers {
                            println!("  {} | {} | {}", s.name, s.command, if s.enabled { "启用" } else { "禁用" });
                        }
                    }
                }
                _ => {
                    println!("MCP 命令:");
                    println!("  rustcc mcp list   - 列出已配置的 MCP 服务器");
                }
            }
        }
        None => {
            // Check env vars first, then config files, then offer quick setup
            let config_exists = storage::config::Config::exists(cli.config.as_deref());

            if !config_exists {
                // Try to auto-configure from environment variables
                if let Some(config) = try_auto_config() {
                    config.save()?;
                    println!("\n已从环境变量自动配置！启动 RustCC...\n");
                } else {
                    // Simple text-based quick setup
                    println!();
                    println!("{}", "  RustCC — AI Agent CLI".bright_green().bold());
                    println!();
                    match quick_setup() {
                        Ok(config) => {
                            config.save()?;
                            println!("\n配置已保存！启动 RustCC...\n");
                        }
                        Err(_) => {
                            println!("\n设置已取消。运行 'rustcc cfg init' 可随时配置。");
                            return Ok(());
                        }
                    }
                }
            }

            let session = session::Session::new(&cli)?;
            session.run_tui().await?;
        }
    }

    Ok(())
}

/// Try to auto-configure from environment variables
fn try_auto_config() -> Option<storage::config::Config> {
    use storage::config::{Config, DeepSeekConfig, OpenAIConfig, AnthropicConfig};
    use llm::ProviderType;

    let mut config = Config::default();

    if let Ok(key) = std::env::var("DEEPSEEK_API_KEY") {
        config.providers.deepseek = Some(DeepSeekConfig {
            api_key: key,
            model: "deepseek-chat".into(),
        });
        config.general.default_provider = ProviderType::DeepSeek;
        config.general.default_model = "deepseek-chat".into();
        return Some(config);
    }
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        config.providers.anthropic = Some(AnthropicConfig {
            api_key: key,
            model: "claude-sonnet-4-6".into(),
        });
        config.general.default_provider = ProviderType::Anthropic;
        config.general.default_model = "claude-sonnet-4-6".into();
        return Some(config);
    }
    if let Ok(key) = std::env::var("OPENAI_API_KEY") {
        config.providers.openai = Some(OpenAIConfig {
            api_key: key,
            base_url: None,
            model: "gpt-4o".into(),
        });
        config.general.default_provider = ProviderType::OpenAI;
        config.general.default_model = "gpt-4o".into();
        return Some(config);
    }

    None
}

/// Simple text-based quick setup (no full-screen TUI)
fn quick_setup() -> Result<storage::config::Config, anyhow::Error> {
    use std::io::{self, Write};
    use storage::config::{Config, DeepSeekConfig, OpenAIConfig, AnthropicConfig, OllamaConfig};
    use llm::ProviderType;

    println!("  首次使用需要配置 LLM 提供商和 API Key。");
    println!();
    println!("  选择提供商:");
    println!("    1. DeepSeek  (默认)");
    println!("    2. Anthropic (Claude)");
    println!("    3. OpenAI   (GPT-4o)");
    println!("    4. Ollama   (本地模型)");
    print!("  [1-4, 回车确认]: ");
    io::stdout().flush()?;

    let mut choice = String::new();
    io::stdin().read_line(&mut choice)?;
    let choice = choice.trim();

    let (provider_type, provider_name) = match choice {
        "2" => (ProviderType::Anthropic, "Anthropic"),
        "3" => (ProviderType::OpenAI, "OpenAI"),
        "4" => (ProviderType::Ollama, "Ollama"),
        _ => (ProviderType::DeepSeek, "DeepSeek"),
    };

    let mut config = Config::default();

    if provider_type == ProviderType::Ollama {
        print!("  Ollama 地址 [http://localhost:11434]: ");
        io::stdout().flush()?;
        let mut url = String::new();
        io::stdin().read_line(&mut url)?;
        let url = url.trim();
        let url = if url.is_empty() { "http://localhost:11434" } else { url };

        print!("  模型名称 [codellama]: ");
        io::stdout().flush()?;
        let mut model = String::new();
        io::stdin().read_line(&mut model)?;
        let model = model.trim();
        let model = if model.is_empty() { "codellama" } else { model };

        config.providers.ollama = Some(OllamaConfig {
            base_url: url.to_string(),
            model: model.to_string(),
        });
        config.general.default_provider = ProviderType::Ollama;
        config.general.default_model = model.to_string();
    } else {
        print!("  {} API Key: ", provider_name);
        io::stdout().flush()?;
        let mut api_key = String::new();
        io::stdin().read_line(&mut api_key)?;
        let api_key = api_key.trim().to_string();

        if api_key.is_empty() {
            anyhow::bail!("API Key 不能为空");
        }

        let default_model = match provider_type {
            ProviderType::Anthropic => "claude-sonnet-4-6",
            ProviderType::OpenAI => "gpt-4o",
            _ => "deepseek-chat",
        };

        print!("  模型名称 [{}]: ", default_model);
        io::stdout().flush()?;
        let mut model = String::new();
        io::stdin().read_line(&mut model)?;
        let model = model.trim();
        let model = if model.is_empty() { default_model.to_string() } else { model.to_string() };

        match provider_type {
            ProviderType::DeepSeek => {
                config.providers.deepseek = Some(DeepSeekConfig { api_key, model: model.clone() });
            }
            ProviderType::Anthropic => {
                config.providers.anthropic = Some(AnthropicConfig { api_key, model: model.clone() });
            }
            ProviderType::OpenAI => {
                config.providers.openai = Some(OpenAIConfig { api_key, base_url: None, model: model.clone() });
            }
            _ => {}
        }
        config.general.default_provider = provider_type;
        config.general.default_model = model;
    }

    Ok(config)
}

fn index_directory(engine: &mut services::rag::RagEngine, path: &str) {
    let base = std::path::Path::new(path);
    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                    if !name.starts_with('.') && name != "target" && name != "node_modules" {
                        index_directory(engine, &p.display().to_string());
                    }
                }
            } else if p.is_file() {
                if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                    let lang = match ext {
                        "rs" => "rust",
                        "py" => "python",
                        "js" => "javascript",
                        "ts" => "typescript",
                        "tsx" => "tsx",
                        "go" => "go",
                        "java" => "java",
                        "c" | "h" => "c",
                        "cpp" | "hpp" => "cpp",
                        "toml" | "yaml" | "yml" | "json" => "config",
                        _ => continue,
                    };
                    if let Ok(content) = std::fs::read_to_string(&p) {
                        engine.index_file(&p.display().to_string(), &content, lang);
                    }
                }
            }
        }
    }
}
