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
        Some(Commands::Config { action }) => {
            let mut config = storage::config::Config::load()?;
            match action {
                Some(cli::ConfigAction::Show) | None => {
                    let toml_str = toml::to_string_pretty(&config)?;
                    println!("{}", toml_str);
                }
                Some(cli::ConfigAction::Init) => {
                    config = storage::config::Config::default();
                    config.save()?;
                    println!("Configuration initialized with defaults.");
                    println!("Config file: {}", dirs::config_dir().unwrap().join("rustcc").join("config.toml").display());
                }
                Some(cli::ConfigAction::Set { .. }) => {
                    println!("Use 'rustcc config init' to create a default config, then edit it directly.");
                }
            }
        }
        Some(Commands::Models) => {
            let session = session::Session::new(&cli)?;
            println!("Provider: {}", session.current_provider_type().to_string());
            println!("Available models:");
            for m in session.models() {
                println!("  {}", m);
            }
        }
        Some(Commands::Tools) => {
            let mut registry = tools::ToolRegistry::new();
            tools::builtin::register_all(&mut registry);
            println!("Available tools ({}):", registry.names().len());
            for name in &registry.names() {
                let approval = if registry.requires_approval(name) {
                    " [requires approval]"
                } else {
                    ""
                };
                println!("  {}{}", name, approval);
            }
        }
        Some(Commands::History { action }) => {
            let history = storage::history::HistoryManager::new()?;
            match action.as_deref() {
                Some("list") | None => {
                    let sessions = history.list()?;
                    if sessions.is_empty() {
                        println!("No saved sessions.");
                    } else {
                        println!("Saved sessions ({}):", sessions.len());
                        for s in &sessions {
                            println!(
                                "  {} | {} | {} | {} msgs | {}",
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
                            println!("Loaded session: {}", record.title);
                            println!("  Messages: {}", record.message_count);
                            for (i, msg) in record.messages.iter().enumerate() {
                                println!("  [{}] {}: {}", i, msg.role, msg.content.chars().take(200).collect::<String>());
                            }
                        }
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(id) => {
                    match history.load(id) {
                        Ok(record) => {
                            println!("Session: {}", record.title);
                            println!("  Model: {}", record.model);
                            println!("  Messages: {}", record.message_count);
                            println!("  Created: {}", record.created_at);
                        }
                        Err(e) => eprintln!("Error: {}", e),
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
                    println!("Indexed {} documents.", engine.len());
                }
                Some("search") => {
                    println!("Use 'rustcc chat' and type /rag <query> to search.");
                }
                _ => {
                    println!("RAG commands:");
                    println!("  rustcc rag index [path]  - Index code files for retrieval");
                    println!("  rustcc rag search <q>    - Search indexed code");
                }
            }
        }
        Some(Commands::Mcp { action }) => {
            let manager = services::mcp::McpManager::new()?;
            match action.as_deref() {
                Some("list") => {
                    let servers = manager.list_servers();
                    if servers.is_empty() {
                        println!("No MCP servers configured.");
                        println!("Add servers in: {}", manager.configs_path());
                    } else {
                        println!("MCP Servers:");
                        for s in servers {
                            println!("  {} | {} | {}", s.name, s.command, if s.enabled { "enabled" } else { "disabled" });
                        }
                    }
                }
                _ => {
                    println!("MCP commands:");
                    println!("  rustcc mcp list   - List configured MCP servers");
                }
            }
        }
        None => {
            // Check if config exists; if not, run setup wizard on first launch
            let config_path = dirs::config_dir()
                .unwrap_or_default()
                .join("rustcc")
                .join("config.toml");

            if !config_path.exists() {
                match tui::run_setup()? {
                    Some(config) => {
                        config.save()?;
                        println!("\nConfiguration saved! Starting RustCC...\n");
                    }
                    None => {
                        println!("Setup cancelled. Run 'rustcc config init' to configure later.");
                        return Ok(());
                    }
                }
            }

            let session = session::Session::new(&cli)?;
            session.run_tui().await?;
        }
    }

    Ok(())
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
