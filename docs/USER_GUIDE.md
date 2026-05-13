# RustCC 使用文档

## 1. 简介

RustCC 是一个 AI Agent CLI 工具，支持多种大语言模型后端，可在终端中提供智能代码助手功能。具备文件读写、代码搜索、Shell 执行、任务追踪、代码智能、MCP 扩展等工具调用能力，内置 TUI 交互界面。

## 2. 安装

### 2.1 前置条件

- **Rust 工具链** 1.70+
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- **Ollama** (可选, 用于本地模型)
  ```bash
  # macOS / Linux
  curl -fsSL https://ollama.com/install.sh | sh
  # Windows: 下载 https://ollama.com/download
  ```
- **语言服务器** (可选, 用于 LSP 代码智能)
  - Rust: `rustup component add rust-analyzer`
  - Python: `pip install pyright`
  - TypeScript: `npm install -g typescript-language-server typescript`

### 2.2 编译安装

```bash
# 如果还没克隆仓库
git clone <repo-url> rustcc
cd rustcc

# 编译 release 版本
cargo build --release
```

编译完成后，二进制位于 `target/release/rustcc` (Linux/macOS) 或 `target\release\rustcc.exe` (Windows)。

**直接运行（无需安装）**:

```bash
# Linux/macOS
./target/release/rustcc config init

# Windows PowerShell
.\target\release\rustcc.exe config init
```

**添加到 PATH（可选，之后可直接打 rustcc）**:

```bash
# Linux/macOS
sudo cp target/release/rustcc /usr/local/bin/

# Windows PowerShell (永久)
[Environment]::SetEnvironmentVariable("Path", $env:Path + ";D:\Projects\RustCC\target\release", "User")
# 重新打开终端后生效

# Windows PowerShell (临时，仅当前窗口)
$env:Path += ";D:\Projects\RustCC\target\release"
```

### 2.3 验证安装

```bash
rustcc --version
rustcc --help
```

## 3. 配置

### 3.1 初始化配置

```bash
rustcc config init
```

配置文件位置:
| 平台 | 路径 |
|------|------|
| Linux | `~/.config/rustcc/config.toml` |
| macOS | `~/Library/Application Support/rustcc/config.toml` |
| Windows | `%APPDATA%\rustcc\config.toml` |

### 3.2 首次启动向导

直接运行 `rustcc` (无参数)，如果配置文件不存在，会自动启动交互式设置向导，引导你完成:
1. 选择 UI 主题 (dark/light)
2. 选择运行模式 (privacy-first / balanced / cloud-max)
3. 配置云提供商和 API Key
4. 选择默认模型

### 3.3 配置 API Key

编辑配置文件，填入对应提供商的 API Key:

```toml
[general]
default_provider = "openai"
default_model = "gpt-4o"
max_conversation_turns = 100
enable_planning = true    # 是否启用计划模式

[providers.openai]
api_key = "sk-your-key-here"
model = "gpt-4o"
# base_url = "https://api.openai.com/v1"  # 可选，支持自定义端点

[providers.anthropic]
api_key = "sk-ant-your-key-here"
model = "claude-sonnet-4-6"

[providers.deepseek]
api_key = "sk-your-deepseek-key"
model = "deepseek-chat"

[providers.ollama]
base_url = "http://localhost:11434"
model = "codellama"

[tools]
allowed_tools = ["read", "write", "edit", "grep", "glob"]
require_approval = ["bash", "write", "edit"]

[ui]
theme = "default"
show_tokens = true
show_tool_calls = true
```

### 3.4 预设模式

在配置中设置 `profile` 可快速切换运行模式:

```toml
# 隐私优先：纯本地运行，零数据离境
profile = "privacy-first"

# 均衡模式：简单问题本地，复杂问题云端
profile = "balanced"

# 云端最大化：全部使用云端模型
profile = "cloud-max"
```

### 3.5 持久化权限设置

权限可跨会话持久化，无需每次重新授权。设置存储在:
- **用户级**: `~/.config/rustcc/settings.json`
- **项目级**: `<project>/.claude/settings.json` (覆盖用户设置)

```json
{
  "permissions": {
    "allow": ["read", "grep", "glob", "bash"],
    "deny": [],
    "additional_directories": ["/tmp/projects"],
    "allow_rules": ["src/**"]
  },
  "hooks": {
    "pre_tool_use": [
      { "command": "echo 'Running tool...'", "matcher": "bash", "wait": true }
    ]
  }
}
```

也可在 TUI 中使用 `/allow` 和 `/deny` 斜杠命令即时管理，修改会自动持久化。

## 4. 基本使用

### 4.1 交互式对话 (TUI 模式)

```bash
# 使用默认配置启动
rustcc

# 指定 provider 和 model
rustcc -p anthropic -m claude-sonnet-4-6

# 带初始 prompt
rustcc chat "帮我重构 src/main.rs 中的错误处理"
```

### 4.2 单次查询

```bash
rustcc run "解释这段代码的作用"
rustcc run "src/main.rs 有哪些函数？"
```

### 4.3 命令行参考

```
rustcc [OPTIONS] [COMMAND]

Options:
  -c, --config <CONFIG>      配置文件路径
  -p, --provider <PROVIDER>  模型提供商 (openai, anthropic, deepseek, ollama)
  -m, --model <MODEL>        模型名称
  -h, --help                 帮助信息
  -V, --version              版本信息

Commands:
  chat         交互式对话 (TUI)
  run          单次查询
  config       配置管理
  models       查看可用模型
  tools        查看可用工具
  history      会话历史管理
  rag          代码检索 (RAG)
  mcp          MCP 服务器管理
  help         帮助信息
```

## 5. TUI 操作指南

### 5.1 界面布局

```
┌──────────────────────────────────────────────┐
│  RustCC | anthropic | claude-sonnet-4-6      │  状态栏
├──────────────────────────────────────────────┤
│  用户消息...                                   │  消息区
│  代理回复...                                   │
│    ✓ read  src/main.rs                       │  工具执行记录
│    ✓ edit  main.rs                           │
├──────────────────────────────────────────────┤
│  > 输入消息 _                                  │  输入区
└──────────────────────────────────────────────┘
```

### 5.2 按键操作

| 按键 | 操作 |
|------|------|
| `Enter` | 发送消息 / 执行斜杠命令 |
| `Shift+Enter` | 换行 |
| `Ctrl+C` | 退出程序 |
| `Esc` | 取消当前 Agent 任务 |
| `Y` / `N` | 在确认弹窗中批准/拒绝工具调用 |
| `↑` / `↓` | 滚动消息历史 |
| `PgUp` / `PgDn` | 翻页 |
| `Home` / `End` | 光标移到行首/行尾 |
| `←` / `→` | 移动光标 |
| `Backspace` | 删除前一个字符 |

### 5.3 斜杠命令

在输入框输入以 `/` 开头的命令:

| 命令 | 说明 |
|------|------|
| `/help` | 显示帮助 |
| `/model <name>` | 切换模型 |
| `/provider <name>` | 切换提供商 (openai/anthropic/deepseek/ollama) |
| `/tools` | 列出可用工具及权限状态 |
| `/models` | 列出可用模型 |
| `/allow <tool>` | 永久允许工具（持久化到 settings.json） |
| `/deny <tool>` | 永久拒绝工具（持久化到 settings.json） |
| `/permissions` | 查看当前权限设置 |
| `/clear` | 清空对话上下文 |
| `/save` | 保存当前会话 |
| `/exit` 或 `/quit` | 退出 |

## 6. Agent 工具系统

RustCC 内置 13 个工具，Agent 会自动判断何时使用:

### 6.1 文件工具

| 工具 | 功能 | 示例 |
|------|------|------|
| `read` | 读取文件内容 | "帮我看看 src/main.rs 的内容" |
| `write` | 写入文件 | "创建一个 config.toml 文件" |
| `edit` | 精确替换 | "把 main.rs 中的 foo 改成 bar" |

### 6.2 搜索工具

| 工具 | 功能 | 示例 |
|------|------|------|
| `grep` | 正则搜索 | "找出所有使用 tokio::spawn 的地方" |
| `glob` | 文件查找 | "列出所有的 .rs 文件" |

### 6.3 系统工具

| 工具 | 功能 | 示例 |
|------|------|------|
| `bash` | 执行命令 | "运行 cargo test" |

### 6.4 网络工具

| 工具 | 功能 | 示例 |
|------|------|------|
| `webfetch` | 获取网页内容 | "抓取这个 API 文档页面" |
| `websearch` | 网络搜索 | "搜索 Rust async trait 最新用法" |

### 6.5 智能工具

| 工具 | 功能 | 前提条件 | 示例 |
|------|------|---------|------|
| `plan` | 生成结构化执行计划 | 无 | Agent 自动在复杂任务前调用 |
| `lsp` | 代码智能 (跳转定义/查找引用/悬停/符号列表) | 需安装语言服务器 | "找到这个函数的定义" |

### 6.6 任务追踪工具

| 工具 | 功能 |
|------|------|
| `task_create` | 创建新任务（标题、描述、依赖关系） |
| `task_update` | 更新任务状态 (pending → in_progress → completed) |
| `task_list` | 列出所有任务（可按状态过滤） |
| `task_get` | 获取任务详情和依赖信息 |

Agent 在复杂任务中会自动使用任务追踪工具来拆解和管理进度:
1. 用 `task_create` 将大任务拆成子任务
2. 用 `task_update` 标记当前进度
3. 用 `task_list` 查看下一步要做什么
4. 用 `task_get` 查看具体任务要求

### 6.7 权限说明

工具执行遵循三层权限:
- **绿色工具** (read, grep, glob, plan, task_*, lsp): 直接执行，无需确认
- **黄色工具** (write, edit, bash, webfetch): 默认需要用户确认，TUI 会弹出 diff 预览
- **红色禁止**: 在 denylist 中的工具直接拒绝

权限管理方式:
- 使用 `/allow <tool>` 和 `/deny <tool>` 即时管理（自动持久化）
- 使用 `/permissions` 查看当前设置
- 编辑 `~/.config/rustcc/settings.json` 手动配置
- 项目级 `.claude/settings.json` 可覆盖用户全局设置

## 7. 代码智能 (LSP)

### 7.1 安装语言服务器

```bash
# Rust
rustup component add rust-analyzer

# Python
pip install pyright

# TypeScript/JavaScript
npm install -g typescript-language-server typescript

# Go
go install golang.org/x/tools/gopls@latest
```

### 7.2 使用方式

Agent 会自动调用 `lsp` 工具。LSP 支持的操作:

| 操作 | 说明 |
|------|------|
| `goToDefinition` | 跳转到符号定义 |
| `findReferences` | 查找所有引用 |
| `hover` | 查看类型和文档 |
| `documentSymbol` | 列出文件所有符号 |

Agent 会根据用户问题自动选择操作，例如:
- "这个函数在哪里定义的？" → goToDefinition
- "谁在调用这个方法？" → findReferences
- "这个变量是什么类型？" → hover

## 8. MCP 扩展

RustCC 支持 Model Context Protocol (MCP)，可连接外部工具服务器扩展能力。

### 8.1 配置 MCP 服务器

在 `~/.config/rustcc/mcp_servers.json` 中添加:

```json
[
  {
    "name": "my-server",
    "command": "node",
    "args": ["server.js"],
    "env": {},
    "enabled": true,
    "description": "My custom MCP server"
  }
]
```

配置后，Agent 在启动时会自动连接配置的 MCP 服务器并发现其工具。

### 8.2 管理

```bash
rustcc mcp list          # 列出所有服务器
```

## 9. RAG 代码检索

### 9.1 索引代码库

```bash
# 索引当前目录的所有代码文件
rustcc rag index

# 索引指定目录
rustcc rag index src/
```

索引会分析代码文件，建立 BM25 关键词索引和语义向量索引。

### 9.2 在对话中使用

Agent 可通过工具自动检索相关代码上下文。

支持的语言: Rust, Python, JavaScript, TypeScript, Go, Java, C, C++

## 10. 会话管理

### 10.1 保存和加载

```bash
# 在 TUI 中使用 /save 保存
/save

# 查看已保存的会话
rustcc history

# 加载指定会话
rustcc history <session-id>
```

会话文件位置:
| 平台 | 路径 |
|------|------|
| Linux | `~/.local/share/rustcc/sessions/` |
| macOS | `~/Library/Application Support/rustcc/sessions/` |
| Windows | `%APPDATA%/rustcc/sessions/` |

## 11. 常见问题

### Q: 提示 "Provider not configured"
A: 运行 `rustcc config init` 初始化配置，然后编辑配置文件填入 API key。

### Q: Ollama 连接失败
A: 确保 Ollama 正在运行: `ollama serve`。默认地址是 `http://localhost:11434`。

### Q: LSP 工具报错
A: 确保对应的语言服务器已安装且在 PATH 中。参见第 7 节。

### Q: MCP 服务器连接失败
A: 检查 `mcp_servers.json` 中的 command 和 args 是否正确，确认服务器可独立启动。

### Q: target 目录占用空间大
A: 运行 `cargo clean` 清理构建缓存。

### Q: 如何切换到本地模式
A: 在 config.toml 中设置 `profile = "privacy-first"` 或手动设置 `default_provider = "ollama"`。

### Q: 工具执行被拒绝
A: 使用 `/allow <tool>` 永久授权，或在 settings.json 中配置权限白名单。

### Q: 权限设置不生效
A: 项目级 `.claude/settings.json` 会覆盖用户全局设置。检查两个文件是否存在冲突。

### Q: 任务工具如何工作
A: Agent 会在处理复杂任务时自动使用 task_create/task_update 追踪进度。你也可以明确要求 Agent 使用，例如 "帮我创建任务来跟踪这个重构工作"。

## 12. 卸载

```bash
# 删除二进制
rm /usr/local/bin/rustcc       # Linux/macOS

# 删除配置和数据
rm -rf ~/.config/rustcc        # Linux
rm -rf ~/Library/Application\ Support/rustcc  # macOS
rmdir /s %APPDATA%\rustcc      # Windows
```
