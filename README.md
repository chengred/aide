# Aide — AI Agent CLI

高性能、模块化的 AI Agent CLI 工具，Rust 实现。支持多种 LLM 后端、交互式 TUI、13 个内置工具。

## 特性

- **多模型支持**: DeepSeek（默认）、OpenAI、Anthropic Claude、Ollama 本地模型
- **交互式 TUI**: Ratatui 终端界面，流式输出，工具调用可视化
- **Agent 循环**: plan → action → observe，自动工具选择与执行
- **13 个内置工具**: read/write/edit/grep/glob/bash/webfetch/websearch/plan/lsp + task_create/task_update/task_list/task_get
- **代码智能 (LSP)**: 跳转定义、查找引用、悬停信息、符号列表
- **任务追踪**: 复杂任务自动拆解、状态管理、依赖追踪
- **MCP 协议**: JSON-RPC 子进程管理，连接外部工具服务器
- **权限系统**: 三级权限 (Allow/Confirm/Deny)，持久化到 settings.json
- **斜杠命令**: /help /model /provider /allow /deny /permissions /clear /save /exit
- **首次向导**: 7 步 TUI 设置向导，支持环境变量自动检测

## 快速开始

### 安装

```powershell
# Windows PowerShell（一键安装）
.\install.ps1

# 或者手动
cargo install --path .
```

安装后在任意目录输入 `aide` 即可使用。

### 首次启动

```powershell
aide
```

首次运行无配置文件时自动进入 7 步设置向导：选择主题 → 运行模式 → 提供商 → 粘贴 API Key → 选择模型 → 完成。

如果设置了环境变量 `DEEPSEEK_API_KEY`（或 `ANTHROPIC_API_KEY`、`OPENAI_API_KEY`），会自动跳过向导。

### 使用

```powershell
aide                    # 启动 TUI 交互模式
aide run "问题描述"      # 单次非交互查询
aide cfg init           # 初始化配置
aide cfg show           # 查看当前配置
aide list               # 查看可用模型
aide tool               # 查看可用工具
aide hist               # 会话历史
aide rag index          # 索引代码库
aide mcp list           # MCP 服务器列表
```

旧命令名兼容：`config`/`models`/`tools`/`history` 仍可使用。

### TUI 快捷键

| 键 | 操作 |
|---|------|
| `Enter` | 发送消息 |
| `Shift+Enter` | 换行 |
| `Ctrl+C` | 退出 |
| `Esc` | 取消 Agent |
| `↑` `↓` | 滚动历史 |
| `Y` / `N` | 确认弹窗中批准/拒绝 |

### 斜杠命令

| 命令 | 功能 |
|------|------|
| `/help` | 帮助 |
| `/model <name>` | 切换模型 |
| `/provider <p>` | 切换提供商 |
| `/allow <tool>` | 永久允许工具 |
| `/deny <tool>` | 永久拒绝工具 |
| `/permissions` | 查看权限 |
| `/clear` | 重置上下文 |
| `/save` | 保存会话 |
| `/exit` | 退出 |

### 配置

```toml
# aide.toml
[general]
default_provider = "deepseek"
default_model = "deepseek-chat"

# 快速切换预设
profile = "deepseek"       # deepseek / openai / anthropic / privacy-first / cloud-max

[providers.deepseek]
api_key = "sk-xxx"
model = "deepseek-chat"
```

## 架构

```
src/
├── agent/       # Agent 引擎 (loop, planner, context, memory, subagent)
├── cli/         # CLI 定义
├── llm/         # LLM 抽象 (OpenAI/Anthropic/DeepSeek/Ollama)
├── services/    # ModelRouter, RagEngine, McpProtocol, LspClient
├── session.rs   # 会话管理
├── storage/     # Config, History, Settings
├── tools/       # 13 个内置工具 + 权限系统
├── tui/         # Ratatui 界面 + widgets
└── utils/       # Token 计数器
```

## 测试

```bash
cargo test          # 50 个测试
```

## License

MIT
