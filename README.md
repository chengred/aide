# Aide — AI Agent CLI

A high-performance, modular AI Agent CLI tool built in Rust. Supports multiple LLM backends, interactive TUI, and comprehensive tool-based agent execution for software engineering tasks.

## Features

- **Multi-Provider LLM**: OpenAI, Anthropic Claude, DeepSeek, and Ollama local models
- **Interactive TUI**: Full Ratatui-based terminal interface with streaming output and tool visualization
- **Agent Loop**: plan → action → observe cycle with automatic tool selection and execution
- **Built-in Tools**: read, write, edit, grep, glob, and bash with permission control
- **Model Router**: Automatic complexity analysis and routing to optimal model tier
- **RAG Code Search**: Hybrid BM25 + semantic retrieval for codebase understanding
- **Sub-agents**: Isolated parallel agents for complex task decomposition
- **MCP Protocol**: JSON-RPC Model Context Protocol server support
- **Session Persistence**: Save, load, export, and manage conversation history
- **Privacy Modes**: Local-only, cloud, and hybrid operation modes

## Quick Start

### Prerequisites

- Rust toolchain 1.70+ (`rustup default stable`)
- Optional: Ollama for local models

### Install

```bash
# Clone and build
git clone https://github.com/your-org/aide.git
cd aide
cargo build --release

# The binary is at target/release/aide
```

### Configure

```bash
# Initialize default configuration
aide config init

# Config is at:
#   Linux:   ~/.config/aide/config.toml
#   macOS:   ~/Library/Application Support/aide/config.toml
#   Windows: %APPDATA%/aide/config.toml
```

Edit the config to add your API keys:

```toml
[general]
default_provider = "openai"
default_model = "gpt-4o"

[providers.openai]
api_key = "sk-..."
model = "gpt-4o"

[providers.anthropic]
api_key = "sk-ant-..."
model = "claude-sonnet-4-6"

[providers.ollama]
base_url = "http://localhost:11434"
model = "codellama"
```

Pre-built profiles are available:

```bash
# Edit config.toml and set:
profile = "privacy-first"   # Local only, zero cloud
profile = "balanced"         # Hybrid local + cloud
profile = "cloud-max"        # Maximum cloud capability
```

### Usage

```bash
# Start interactive TUI session (default)
aide

# Start with specific provider and model
aide -p anthropic -m claude-sonnet-4-6 chat

# Single-shot query
aide run "explain the error handling in src/main.rs"

# Index code for RAG search
aide rag index

# View available tools
aide tools

# Manage session history
aide history

# List MCP servers
aide mcp list
```

### TUI Controls

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Shift+Enter` | New line |
| `Ctrl+C` | Quit |
| `Esc` | Cancel agent |
| `↑` / `↓` | Scroll history |
| `PgUp` / `PgDn` | Page scroll |
| `Home` / `End` | Line start/end |

### Slash Commands (in TUI)

| Command | Description |
|---------|-------------|
| `/help` | Show help |
| `/model <name>` | Switch model |
| `/provider <p>` | Switch provider |
| `/tools` | List available tools |
| `/clear` | Reset context |
| `/save` | Save session |

## Architecture

```
src/
├── agent/       # Agent engine, planner, context, memory, subagents
├── cli/         # Clap CLI definitions
├── llm/         # LLM abstraction (OpenAI, Anthropic, DeepSeek, Ollama)
├── services/    # Model router, RAG engine, MCP protocol
├── session.rs   # Session manager (TUI + REPL)
├── storage/     # Config management, session history
├── tools/       # Tool registry + 6 builtin tools + permissions
└── tui/         # Ratatui terminal UI
```

### Design Patterns

- **Agent Loop**: plan → action → observe, inspired by Claude Code's architecture
- **Tool System**: Trait-based async tool registry with JSON Schema generation
- **Permissions**: Three-tier (Allow/Confirm/Deny) with path-based filtering
- **Context Management**: Sliding window + summary compression
- **Three-tier Memory**: Working (session), Short-term (summaries), Long-term (RAG)

## Running Tests

```bash
cargo test                    # All tests
cargo test -- --nocapture     # With output
cargo test -p aide agent    # Specific module
```

## Release Build

```bash
cargo build --release         # Optimized with LTO, single codegen unit
```

Release profile is configured in Cargo.toml with:
- LTO (Link-Time Optimization)
- Single codegen unit
- Stripped symbols
- Abort on panic

## License

MIT
