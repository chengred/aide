use clap::{Parser, Subcommand};

/// RustCC - A high-performance, modular AI Agent CLI tool
#[derive(Parser, Debug)]
#[command(name = "rustcc", version, about, long_about = None)]
pub struct Cli {
    /// Configuration file path
    #[arg(short, long, global = true)]
    pub config: Option<String>,

    /// Model provider to use (openai, anthropic, deepseek, ollama)
    #[arg(short, long, global = true)]
    pub provider: Option<String>,

    /// Model name to use
    #[arg(short, long, global = true)]
    pub model: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start interactive chat session
    Chat {
        /// Initial prompt to send immediately
        prompt: Option<String>,
    },

    /// Run a single prompt and exit
    Run {
        /// The prompt to run
        prompt: String,

        /// Output format
        #[arg(short, long, default_value = "text")]
        output: String,
    },

    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },

    /// Show available models
    Models,

    /// Show available tools
    Tools,

    /// Session history management
    History {
        /// Action: list, <id>, load:<id>
        action: Option<String>,
    },

    /// RAG code retrieval
    Rag {
        /// Action: index, search
        action: Option<String>,
        /// Path or query
        path: Option<String>,
    },

    /// MCP server management
    Mcp {
        /// Action: list
        action: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Show current configuration
    Show,
    /// Set a configuration value
    Set {
        key: String,
        value: String,
    },
    /// Initialize configuration with defaults
    Init {
        /// Create config in current directory instead of global
        #[arg(short, long)]
        local: bool,
    },
}
