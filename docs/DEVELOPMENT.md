# RustCC 开发文档

## 1. 项目概述

RustCC 是一个用 Rust 实现的高性能、模块化 AI Agent CLI 工具。借鉴 Claude Code 的核心设计模式，支持多种 LLM 后端，提供交互式 TUI 终端体验和完整的 Agent 工具调用系统。

**技术栈**: Rust 1.95 + tokio + ratatui + reqwest + clap
**代码规模**: 45 个源文件，约 8000 行代码，20 个依赖

## 2. 架构总览

```
┌──────────────────────────────────────────────────────┐
│                    用户界面层 (TUI)                    │
│  Ratatui 终端界面 | 流式输出 | 工具可视化 | 斜杠命令  │
│  widgets: status | messages | input                  │
└────────────────────────┬─────────────────────────────┘
                         │ mpsc channel
┌────────────────────────▼─────────────────────────────┐
│                  控制器层 (Session)                    │
│  Session Manager | Agent Loop | Context Mgmt         │
│  Slash Commands | Permission Management             │
└────────────────────────┬─────────────────────────────┘
                         │
┌────────────────────────▼─────────────────────────────┐
│                   服务层 (Services)                    │
│  ModelRouter | RagEngine | McpProtocol | LspClient   │
└────────────────────────┬─────────────────────────────┘
                         │
┌────────────────────────▼─────────────────────────────┐
│              基础设施层 (Infrastructure)               │
│  LLM Providers | Tool System | File I/O | Shell      │
│  OpenAI/Anthropic/DeepSeek/Ollama                    │
│  Settings | Config | History                         │
└──────────────────────────────────────────────────────┘
```

## 3. 模块设计

### 3.1 LLM 抽象层 (`src/llm/`)

统一的多模型后端接口，通过 `LLMProvider` trait 抽象。

```rust
#[async_trait]
pub trait LLMProvider: Send + Sync {
    async fn chat(&self, messages: &[Message], options: &ChatOptions)
        -> Result<ChatResponse, LLMError>;
    async fn stream(&self, messages: &[Message], options: &ChatOptions)
        -> Result<Box<dyn Stream<Item = Result<StreamChunk, LLMError>>>>;
    fn models(&self) -> Vec<String>;
    fn supports_tools(&self) -> bool;
    fn provider_type(&self) -> ProviderType;
}
```

**已实现的 Provider**:
| Provider | 协议 | 流式 | 工具调用 |
|----------|------|------|----------|
| OpenAI | REST + SSE | ✓ | ✓ |
| Anthropic | REST + SSE | ✓ | ✓ |
| DeepSeek | OpenAI 兼容 | ✓ | ✓ |
| Ollama | REST + NDJSON | ✓ | ✓ |

**消息类型**:
- `Message`: System / User / Assistant / Tool 四种角色
- `ToolCall` / `ToolDefinition`: 符合 OpenAI Function Calling 规范的 JSON Schema
- `StreamChunk` / `ToolCallDelta`: 流式增量数据

### 3.2 Agent 引擎 (`src/agent/`)

核心执行引擎，实现 plan → action → observe 循环。

**Agent Loop 工作流程**:
1. 构建系统提示（含记忆上下文 + 计划指令 + 活跃计划状态）
2. 发送 LLM 推理请求
3. 解析响应：
   - 文本内容 → 流式输出
   - 工具调用 → 权限检查 → 执行工具 → 结果回传
   - plan 工具调用 → 解析为 ExecutionPlan → 注入后续上下文
4. 循环直到 end_turn / max_turns / refusal

**双模式执行**:
- `run()`: 使用 `chat()` 接口，适合非交互模式 (run_once)
- `run_streaming()`: 使用 `stream()` 接口 + AgentEvent 通道，适合 TUI

**AgentEvent 流式事件**:
```rust
pub enum AgentEvent {
    TextDelta(String),              // 流式文本片段
    ToolCallStart { id, name, args }, // 工具开始调用
    ToolCallEnd { id, name, result, success }, // 工具调用完成
    AgentDone { content, turns, total_tokens }, // Agent 完成
    AgentError(String),             // 错误
    ConfirmRequest {                // 权限确认请求
        details: ConfirmationDetails,
        response_tx: oneshot::Sender<bool>,
    },
}
```

**子代理系统** (`subagent.rs`):
- `SubAgentManager::spawn()`: 创建隔离子代理，独立上下文 + 受限工具集
- `SubAgentManager::run_parallel()`: 并行执行多个子任务
- 结果收集和整合

**上下文管理** (`context.rs`):
- 滑动窗口: 保留最近 N 轮完整对话
- 摘要压缩: 旧消息生成摘要附加到系统提示
- Token 估算: 基于模型的 token 计数器

**规划器** (`planner.rs`):
- `ExecutionPlan`: 结构化执行计划，含步骤列表
- `PlanStep`: 单个步骤 (状态跟踪 + 依赖管理)
- `Planner`: 计划生成与解析
- `format_plan_context()`: 生成表格化的计划状态并注入 Agent Loop
- PlanTool 被 LLM 调用后，其结果自动解析为 ExecutionPlan 并追踪

**记忆系统** (`memory/mod.rs`):
- 四类记忆: User / Feedback / Project / Reference
- 文件存储: YAML frontmatter 的 .md 文件
- MEMORY.md 索引
- 项目级 (`.claude/memory/`) 和用户级 (`~/.config/rustcc/memory/`) 双路径

### 3.3 工具系统 (`src/tools/`)

基于 trait 的可扩展工具注册表。

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;  // JSON Schema
    async fn execute(&self, params: serde_json::Value) -> ToolResult;
    fn requires_approval(&self) -> bool { false }
}
```

**内置工具 (13个)**:
| 工具 | 功能 | 需确认 | 备注 |
|------|------|--------|------|
| read | 读取文件 (支持分页/行号) | 否 | |
| write | 写入文件 (自动创建目录) | 是 | |
| edit | 精确字符串替换 | 是 | |
| grep | 正则搜索 (递归目录) | 否 | |
| glob | 文件模式匹配 | 否 | |
| bash | Shell命令执行 | 是 | |
| webfetch | 获取网页内容 | 是 | |
| websearch | 网络搜索 | 否 | |
| plan | 结构化计划工具 | 否 | 上下文工程，触发计划追踪 |
| lsp | 代码智能 (定义跳转/引用/悬停/符号) | 否 | 需安装对应语言服务器 |
| task_create | 创建任务 | 否 | 共享状态，4 工具联动 |
| task_update | 更新任务状态/依赖 | 否 | |
| task_list | 列出任务 (支持过滤) | 否 | |
| task_get | 获取任务详情 | 否 | |

**权限系统** (`permission.rs`):
三层防护:
1. **Allow**: 白名单工具 + 路径过滤
2. **Confirm**: 高影响操作需用户确认（TUI 弹窗含 diff 预览）
3. **Deny**: 黑名单工具 + 路径排除

权限持久化通过 settings.json: 项目级 `.claude/settings.json` 覆盖用户级 `~/.config/rustcc/settings.json`。

**斜杠命令** (TUI 内):
| 命令 | 功能 |
|------|------|
| `/help` | 显示帮助 |
| `/clear` | 重置会话上下文 |
| `/model <name>` | 切换模型 |
| `/provider <p>` | 切换提供商 |
| `/tools` | 列出工具及权限状态 |
| `/models` | 列出模型 |
| `/allow <tool>` | 永久允许工具（持久化） |
| `/deny <tool>` | 永久拒绝工具（持久化） |
| `/permissions` | 查看当前权限 |
| `/save` | 保存会话 |
| `/exit` | 退出 |

### 3.4 TUI 界面 (`src/tui/`)

基于 Ratatui 的全功能终端界面，已拆分为独立的 widget 组件。

**组件结构**:
```
src/tui/
├── app.rs          # App 状态 + 事件循环 + 输入处理
├── mod.rs          # 模块声明
├── setup.rs        # 首次设置向导
├── themes.rs       # Theme 定义 (dark/light)
└── widgets/
    ├── mod.rs      # 重导出
    ├── status.rs   # draw_status_bar
    ├── messages.rs # draw_messages
    └── input.rs    # draw_input + confirm dialog
```

**布局**:
```
┌──────────────────────────────────────────────┐
│  RustCC | provider | model     tokens | turns │  ← 状态栏
├──────────────────────────────────────────────┤
│  用户: 请帮我优化错误处理                      │  ← 消息区
│                                              │     (可滚动)
│  代理: 正在分析...                             │
│    ◌ read  src/main.rs                       │  ← 工具执行面板
│    ✓ write src/main.rs                       │
│                                              │
├──────────────────────────────────────────────┤
│  > 用户输入区域 _                              │  ← 输入区
└──────────────────────────────────────────────┘
```

**事件循环**:
- 非阻塞 drain agent events (try_recv)
- 50ms 轮询间隔 (处理中) / 200ms (正常)
- Ctrl+C 退出, Esc 取消, ↑↓ 滚动
- 斜杠命令在本地拦截处理，不发送给 Agent

**通信模型**:
```
TUI (主线程)  ←──event_rx──   Agent Task (后台)
              ──user_tx──→
```

### 3.5 服务层 (`src/services/`)

**模型路由器** (`model_router.rs`):
- 启发式复杂度分析: Simple / Medium / Complex / Code
- 4 级模型路由: Fast (本地) / Balanced / Reasoner / CodeGen
- `auto_route(query)`: 一键返回 (provider, model)

**RAG 引擎** (`rag.rs`):
- 代码分块: 50 行/chunk, 25 行重叠
- BM25 关键词检索 + Hash Embedding 语义检索
- 混合排序: 默认 0.5/0.5 权重
- 支持多语言: rust/python/typescript/go/java/c/cpp

**MCP 协议** (`mcp.rs`):
- JSON-RPC 2.0 over stdio
- 真实子进程管理 (tokio::process::Command)
- 生命周期: spawn → initialize → initialized 通知 → tools/list → tools/call
- 服务器配置持久化 (`~/.config/rustcc/mcp_servers.json`)
- 优雅关闭 (kill_on_drop)

**LSP 客户端** (`lsp.rs`):
- 完整 LSP 协议实现，通过 stdio 与语言服务器通信
- 支持: rust-analyzer, pyright, typescript-language-server, gopls
- 操作: textDocument/definition, textDocument/references, textDocument/hover, textDocument/documentSymbol
- LSP 头解析 (Content-Length) + JSON-RPC 请求/响应

### 3.6 配置与存储 (`src/storage/`)

**配置管理** (`config.rs`):
- TOML 格式配置文件
- 多 Provider API Key 管理
- 操作模式 + 预设配置文件

**设置系统** (`settings.rs`):
- JSON 格式持久化设置
- 双层合并: 项目 `.claude/settings.json` 覆盖用户 `~/.config/rustcc/settings.json`
- 权限白名单/黑名单持久化
- Hook 定义 (PreToolUse / PostToolUse / SessionStart / SessionStop)

**会话历史** (`history.rs`):
- JSON 格式会话持久化
- 支持保存/加载/导出/导入/删除

## 4. 关键设计模式

### 4.1 依赖方向
```
main → cli + session + storage
session → agent + llm + tools + settings
agent → llm + tools + planner
tui → agent (AgentEvent) + settings
tools → services (lsp, mcp)
```

### 4.2 异步模型
- `#[tokio::main]`: 主线程运行 tokio runtime
- `tokio::spawn`: 后台 Agent 任务
- `tokio::spawn_blocking`: Shell 命令执行
- `tokio::sync::mpsc`: TUI ↔ Agent 通信
- `tokio::sync::Mutex`: LSP 客户端共享状态
- `tokio::process::Command`: MCP/LSP 子进程管理

### 4.3 错误处理
- `anyhow::Error`: 应用层，灵活错误链
- `thiserror::Error`: 库层，LLMError / ConfigError
- `ToolResult { success, content, error }`: 工具层

## 5. 构建与测试

### 开发构建
```bash
cargo build                # Debug (opt-level=1)
cargo test                 # 运行全部 50 个测试
cargo clippy               # Lint 检查
```

### Release 构建
```toml
[profile.release]
opt-level = 3       # 最大优化
lto = true          # 链接时优化
codegen-units = 1   # 单代码单元
panic = "abort"     # 移除 panic unwind
strip = true        # 移除符号表
```

```bash
cargo build --release
# 二进制大小: ~15-30 MB
```

### 测试覆盖
| 模块 | 测试数 | 内容 |
|------|--------|------|
| model_router | 7 | 复杂度判定、模型路由 |
| rag | 8 | 索引、搜索、tokenization |
| permission | 8 | 允许/确认/拒绝/路径过滤 |
| context | 6 | 压缩、估算、系统消息保留 |
| mcp | 6 | 请求构建、响应解析 |
| settings | 4 | 合并、去重、路径 |
| task | 5 | CRUD 操作 |
| token_counter | 5 | 各模型估算 |
| **总计** | **50** | |

## 6. 扩展指南

### 添加新的 LLM Provider
1. 在 `src/llm/` 创建新文件
2. 实现 `LLMProvider` trait
3. 在 `mod.rs` 注册
4. 在 `session.rs` 的 `create_provider()` 添加分支
5. 在 `cli/mod.rs` 添加 ProviderType 变体

### 添加新的工具
1. 在 `src/tools/builtin/` 创建新文件
2. 实现 `Tool` trait (async_trait)
3. 在 `builtin/mod.rs` 注册
4. 如需确认，重写 `requires_approval()` 返回 true

### 添加新的 TUI 组件
1. 在 `src/tui/widgets/` 创建 widget 文件
2. 在 `src/tui/app.rs` 的 `draw_ui()` 中调用
3. 更新 `App` 状态结构

### 添加新的 LSP 语言支持
1. 在 `src/services/lsp.rs` 的 `get_handle()` 中添加语言匹配
2. 配置对应语言服务器命令和参数

## 7. 依赖说明

| 依赖 | 用途 |
|------|------|
| tokio | 异步运行时 + 进程管理 |
| ratatui + crossterm | TUI 界面 |
| reqwest | HTTP 客户端 |
| clap | CLI 参数解析 |
| serde + serde_json | 序列化 |
| toml | 配置文件格式 |
| async-trait | async trait 支持 |
| regex | 正则搜索 |
| glob | 文件匹配 |
| uuid | 会话/子代理/任务 ID |
| chrono | 时间戳 |
| dirs | 系统目录 |
| tracing | 结构化日志 |
| futures | Stream 处理 |
| bytes + tokio-util | 流式解析 |
| thiserror + anyhow | 错误处理 |
| colored | 终端着色 |
