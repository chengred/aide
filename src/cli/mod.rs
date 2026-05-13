use clap::{Parser, Subcommand};

/// Aide - 高性能模块化 AI Agent CLI 工具
#[derive(Parser, Debug)]
#[command(name = "aide", version, about, long_about = None)]
pub struct Cli {
    /// 配置文件路径
    #[arg(short, long, global = true)]
    pub config: Option<String>,

    /// 模型提供商 (openai, anthropic, deepseek, ollama)
    #[arg(short, long, global = true)]
    pub provider: Option<String>,

    /// 模型名称
    #[arg(short, long, global = true)]
    pub model: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// 启动交互式对话 (TUI)
    #[command(alias = "chat")]
    Chat {
        /// 初始提示词
        prompt: Option<String>,
    },

    /// 单次查询
    #[command(alias = "run")]
    Run {
        /// 提示词
        prompt: String,

        /// 输出格式
        #[arg(short, long, default_value = "text")]
        output: String,
    },

    /// 配置管理
    #[command(alias = "config")]
    Cfg {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },

    /// 查看可用模型
    #[command(alias = "models")]
    List,

    /// 查看可用工具
    #[command(alias = "tools")]
    Tool,

    /// 会话历史管理
    #[command(alias = "history")]
    Hist {
        /// 操作: list, <id>, load:<id>
        action: Option<String>,
    },

    /// 代码检索 (RAG)
    #[command(alias = "rag")]
    Rag {
        /// 操作: index, search
        action: Option<String>,
        /// 路径或搜索词
        path: Option<String>,
    },

    /// MCP 服务器管理
    #[command(alias = "mcp")]
    Mcp {
        /// 操作: list
        action: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// 显示当前配置
    Show,
    /// 设置配置项
    Set {
        key: String,
        value: String,
    },
    /// 初始化默认配置
    Init {
        /// 在当前目录创建配置（而非全局目录）
        #[arg(short, long)]
        local: bool,
    },
}
