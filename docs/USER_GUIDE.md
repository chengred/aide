# RustCC 使用文档

## 1. 简介

RustCC 是一个 AI Agent CLI 工具，支持多种大语言模型后端，可在终端中提供智能代码助手功能。具备文件读写、代码搜索、Shell 执行等工具调用能力，内置 TUI 交互界面。

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

### 2.2 编译安装

```bash
git clone <repo-url> rustcc
cd rustcc
cargo build --release

# 将二进制添加到 PATH
# Linux/macOS:
cp target/release/rustcc /usr/local/bin/
# Windows:
copy target\release\rustcc.exe C:\Users\<用户名>\.cargo\bin\
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

### 3.2 配置 API Key

编辑配置文件，填入对应提供商的 API Key:

```toml
[general]
default_provider = "openai"
default_model = "gpt-4o"
max_conversation_turns = 100

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

### 3.3 预设模式

在配置中设置 `profile` 可快速切换运行模式:

```toml
# 隐私优先：纯本地运行，零数据离境
profile = "privacy-first"

# 均衡模式：简单问题本地，复杂问题云端
profile = "balanced"

# 云端最大化：全部使用云端模型
profile = "cloud-max"
```

三种模式会自动配置 provider、model 和权限策略。

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
| `Enter` | 发送消息 |
| `Shift+Enter` | 换行 |
| `Ctrl+C` | 退出程序 |
| `Esc` | 取消当前 Agent 任务 |
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
| `/tools` | 列出可用工具 |
| `/models` | 列出可用模型 |
| `/clear` | 清空对话上下文 |
| `/save` | 保存当前会话 |
| `/exit` 或 `/quit` | 退出 |

## 6. Agent 工具系统

RustCC 内置 6 个工具，Agent 会自动判断何时使用:

### 6.1 文件工具

| 工具 | 功能 | 示例 |
|------|------|------|
| `read` | 读取文件内容 | "帮我看看 src/main.rs 的内容" |
| `write` | 写入文件 | "创建一个 config.toml 文件" |
| `edit` | 精确替换 | "把 main.rs 第10行的 foo 改成 bar" |

### 6.2 搜索工具

| 工具 | 功能 | 示例 |
|------|------|------|
| `grep` | 正则搜索 | "找出所有使用 tokio::spawn 的地方" |
| `glob` | 文件查找 | "列出所有的 .rs 文件" |

### 6.3 系统工具

| 工具 | 功能 | 示例 |
|------|------|------|
| `bash` | 执行命令 | "运行 cargo test 看看哪些测试失败了" |

### 6.4 权限说明

工具执行遵循三层权限:
- **绿色工具** (read, grep, glob): 直接执行，无需确认
- **黄色工具** (write, edit, bash): 默认需要用户确认
- 可通过 `/allow <tool>` 临时授权，或在配置中永久授权

## 7. RAG 代码检索

### 7.1 索引代码库

```bash
# 索引当前目录的所有代码文件
rustcc rag index

# 索引指定目录
rustcc rag index src/
```

索引会分析代码文件，建立 BM25 关键词索引和语义向量索引。

### 7.2 在对话中使用

在 TUI 模式下，Agent 可以通过斜杠命令触发代码检索:

```
/rag 错误处理的实现
```

支持的语言: Rust, Python, JavaScript, TypeScript, Go, Java, C, C++

## 8. 会话管理

### 8.1 保存和加载

```bash
# 在 TUI 中使用 /save 保存
/save

# 查看已保存的会话
rustcc history

# 加载指定会话
rustcc history <session-id>

# 导出会话
# 会话文件位于:
#   Linux:   ~/.local/share/rustcc/sessions/
#   macOS:   ~/Library/Application Support/rustcc/sessions/
#   Windows: %APPDATA%/rustcc/sessions/
```

## 9. MCP 服务器

RustCC 支持 Model Context Protocol (MCP)，可连接外部工具服务器。

### 9.1 配置 MCP 服务器

在 `%APPDATA%/rustcc/mcp_servers.json` 中添加:

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

### 9.2 管理

```bash
rustcc mcp list          # 列出所有服务器
```

## 10. 常见问题

### Q: 提示 "Provider not configured"
A: 运行 `rustcc config init` 初始化配置，然后编辑配置文件填入 API key。

### Q: Ollama 连接失败
A: 确保 Ollama 正在运行: `ollama serve`。默认地址是 `http://localhost:11434`。

### Q: target 目录占用空间大
A: 运行 `cargo clean` 清理构建缓存。只保留源代码即可（约 300 KB）。

### Q: 如何切换到本地模式
A: 在 config.toml 中设置 `profile = "privacy-first"` 或手动设置 `default_provider = "ollama"`。

### Q: 工具执行被拒绝
A: 工具权限可在配置中调整。将工具添加到 `allowed_tools` 并移除 `require_approval`:
```toml
[tools]
allowed_tools = ["read", "write", "edit", "grep", "glob", "bash"]
require_approval = []
```

## 11. 卸载

```bash
# 删除二进制
rm /usr/local/bin/rustcc       # Linux/macOS

# 删除配置和数据
rm -rf ~/.config/rustcc        # Linux
rm -rf ~/Library/Application\ Support/rustcc  # macOS
rmdir /s %APPDATA%\rustcc      # Windows
```
