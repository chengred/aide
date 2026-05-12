# RustCC 开发文档

## 1. 项目概述

RustCC 是一个用 Rust 实现的高性能、模块化 AI Agent CLI 工具。借鉴 Claude Code 的核心设计模式，支持多种 LLM 后端，提供交互式 TUI 终端体验和完整的 Agent 工具调用系统。

**技术栈**: Rust 1.95 + tokio + ratatui + reqwest + clap
**代码规模**: 30 个源文件，约 8000 行代码，20 个依赖

## 2. 架构总览

```
┌──────────────────────────────────────────────────────┐
│                    用户界面层 (TUI)                    │
│  Ratatui 终端界面 | 流式输出 | 工具可视化 | 按键处理  │
└────────────────────────┬─────────────────────────────┘
                         │ mpsc channel
┌────────────────────────▼─────────────────────────────┐
│                  控制器层 (Session)                    │
│  Session Manager | Agent Loop | Context Mgmt         │
└────────────────────────┬─────────────────────────────┘
                         │
┌────────────────────────▼─────────────────────────────┐
│                   服务层 (Services)                    │
│  ModelRouter | RagEngine | McpProtocol               │
└────────────────────────┬─────────────────────────────┘
                         │
┌────────────────────────▼─────────────────────────────┐
│              基础设施层 (Infrastructure)               │
│  LLM Providers | Tool System | File I/O | Shell      │
│  OpenAI/Anthropic/DeepSeek/Ollama                    │
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
1. 构建系统提示 + 消息上下文 + 工具定义
2. 发送 LLM 推理请求
3. 解析响应：
   - 文本内容 → 流式输出
   - 工具调用 → 权限检查 → 执行工具 → 结果回传
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
}
```

**子代理系统** (`subagent.rs`):
- `SubAgentManager::spawn()`: 创建隔离子代理，独立上下文 + 受限工具集
- `SubAgentManager::run_parallel()`: 并行执行多个子任务
- 结果收集和整合

**上下文管理** (`context.rs`):
- 滑动窗口: 保留最近 N 轮完整对话
- 摘要压缩: 旧消息生成摘要附加到系统提示
- Token 估算: 4 chars ≈ 1 token (粗略估计)

**规划器** (`planner.rs`):
- `ExecutionPlan`: 结构化执行计划
- `PlanStep`: 单个步骤 (状态跟踪 + 依赖管理)
- `PlanTool`: 上下文工程的 no-op 工具

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

**内置工具 (6个)**:
| 工具 | 功能 | 需确认 |
|------|------|--------|
| read | 读取文件 (支持分页/行号) | 否 |
| write | 写入文件 (自动创建目录) | 是 |
| edit | 精确字符串替换 | 是 |
| grep | 正则搜索 (递归目录) | 否 |
| glob | 文件模式匹配 | 否 |
| bash | Shell命令执行 (超时控制) | 是 |

**权限系统** (`permission.rs`):
三层防护:
1. **Allow**: 白名单工具 + 路径过滤
2. **Confirm**: 高影响操作需用户确认
3. **Deny**: 黑名单工具 + 路径排除

**规划工具** (`planning.rs`):
PlanTool 是一个"空操作"工具，不执行实际动作。它的作用是让 LLM 输出结构化计划，作为上下文工程策略，帮助 Agent 在复杂长期任务中保持方向感。

### 3.4 TUI 界面 (`src/tui/`)

基于 Ratatui 的全功能终端界面。

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
- 方法: initialize / tools/list / tools/call
- 服务器配置持久化

## 4. 关键设计模式

### 4.1 依赖方向
```
main → cli + session + storage
session → agent + llm + tools
agent → llm + tools
tui → agent (AgentEvent)
tools → llm (ToolDefinition)
```

### 4.2 异步模型
- `#[tokio::main]`: 主线程运行 tokio runtime
- `tokio::spawn`: 后台 Agent 任务
- `tokio::spawn_blocking`: Shell 命令执行
- `tokio::sync::mpsc`: TUI ↔ Agent 通信

### 4.3 错误处理
- `anyhow::Error`: 应用层，灵活错误链
- `thiserror::Error`: 库层，LLMError / ConfigError
- `ToolResult { success, content, error }`: 工具层

## 5. 构建与测试

### 开发构建
```bash
cargo build                # Debug (opt-level=1)
cargo test                 # 运行全部 30 个测试
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
| 模块 | 测试数 |
|------|--------|
| model_router | 7 |
| rag | 8 |
| permission | 8 |
| context | 6 |
| config | 1 |
| **总计** | **30** |

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

## 7. 依赖说明

| 依赖 | 用途 |
|------|------|
| tokio | 异步运行时 |
| ratatui + crossterm | TUI 界面 |
| reqwest | HTTP 客户端 |
| clap | CLI 参数解析 |
| serde + serde_json | 序列化 |
| toml | 配置文件格式 |
| async-trait | async trait 支持 |
| regex | 正则搜索 |
| glob | 文件匹配 |
| uuid | 会话/子代理 ID |
| chrono | 时间戳 |
| dirs | 系统目录 |
| tracing | 结构化日志 |
| futures | Stream 处理 |
| bytes + tokio-util | 流式解析 |
| thiserror + anyhow | 错误处理 |
| colored | 终端着色 |
