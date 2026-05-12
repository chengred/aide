pub mod loop_;
pub mod planner;
pub mod context;
pub mod memory;
pub mod subagent;

use tokio::sync::mpsc::UnboundedSender;

use crate::agent::memory::MemoryStore;
use crate::llm::{LLMProvider, Message};

/// Details about an operation needing confirmation
#[derive(Debug)]
pub struct ConfirmationDetails {
    pub tool_name: String,
    /// Short summary line
    pub summary: String,
    /// File path (for file operations)
    #[allow(dead_code)]
    pub file_path: Option<String>,
    /// Old content / before state
    pub old_content: Option<String>,
    /// New content / after state
    pub new_content: Option<String>,
    /// Operation type hint
    pub operation: ConfirmationType,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConfirmationType {
    WriteFile,
    EditFile,
    RunCommand,
    WebFetch,
    Generic,
}

/// Events emitted by the agent during streaming execution (for TUI)
#[derive(Debug)]
#[allow(dead_code)]
pub enum AgentEvent {
    TextDelta(String),
    ToolCallStart { id: String, name: String, args: String },
    ToolCallEnd { id: String, name: String, result: String, success: bool },
    AgentDone { content: String, turns: u32, total_tokens: u32 },
    AgentError(String),
    /// Permission confirmation request — TUI must respond via the oneshot sender
    ConfirmRequest {
        details: ConfirmationDetails,
        response_tx: tokio::sync::oneshot::Sender<bool>,
    },
}
use crate::tools::permission::PermissionManager;
use crate::tools::ToolRegistry;

/// Configuration for the Agent
#[derive(Clone)]
pub struct AgentConfig {
    /// Maximum turns (LLM round-trips) before forcing a stop
    pub max_turns: u32,
    /// System prompt for the agent
    #[allow(dead_code)]
    pub system_prompt: String,
    /// Temperature for LLM calls
    pub temperature: f64,
    /// Maximum tokens per response
    pub max_tokens: u32,
    /// Whether to include the planning tool
    #[allow(dead_code)]
    pub enable_planning: bool,
    /// Whether to show tool calls in output
    pub show_tool_calls: bool,
    /// Whether to auto-load memory into system prompt
    pub enable_memory: bool,
    /// Memory store for persistent memory
    pub memory_store: Option<MemoryStore>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_turns: 100,
            system_prompt: "You are a helpful AI coding assistant. You help users with software engineering tasks. \
                You have access to tools for reading, writing, editing, and searching files, as well as executing shell commands. \
                Use tools when appropriate and explain your reasoning clearly."
                .into(),
            temperature: 0.7,
            max_tokens: 4096,
            enable_planning: true,
            show_tool_calls: true,
            enable_memory: true,
            memory_store: MemoryStore::open().ok(),
        }
    }
}

/// Result of an agent run
#[derive(Debug)]
pub struct AgentResult {
    /// The final text response
    pub content: String,
    /// Total turns taken
    #[allow(dead_code)]
    pub turns: u32,
    /// Total tokens used
    #[allow(dead_code)]
    pub total_tokens: u32,
    /// Tools that were called during execution
    pub tools_called: Vec<String>,
    /// Why the agent stopped
    #[allow(dead_code)]
    pub stop_reason: StopReason,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StopReason {
    /// Agent completed normally (end_turn)
    Completed,
    /// Agent requested tool calls
    #[allow(dead_code)]
    ToolUse,
    /// Hit the maximum turn limit
    MaxTurns,
    /// Exceeded token budget
    MaxTokens,
    /// Content was refused/filtered
    Refusal,
    /// An error occurred
    Error(String),
}

/// The main Agent struct
pub struct Agent {
    config: AgentConfig,
    tool_registry: ToolRegistry,
    permission_manager: PermissionManager,
}

impl Agent {
    pub fn new(
        config: AgentConfig,
        tool_registry: ToolRegistry,
        permission_manager: PermissionManager,
    ) -> Self {
        Self {
            config,
            tool_registry,
            permission_manager,
        }
    }

    pub fn tool_registry(&self) -> &ToolRegistry {
        &self.tool_registry
    }

    pub fn permission_manager(&self) -> &PermissionManager {
        &self.permission_manager
    }

    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    /// Run the agent loop with the given messages and provider
    pub async fn run(
        &self,
        provider: &dyn LLMProvider,
        messages: &[Message],
        model: &str,
    ) -> AgentResult {
        loop_::run_agent_loop(
            provider,
            messages,
            model,
            &self.config,
            &self.tool_registry,
            &self.permission_manager,
            None,
        )
        .await
    }

    /// Run the agent loop with streaming events sent to the TUI
    pub async fn run_streaming(
        &self,
        provider: &dyn LLMProvider,
        messages: &[Message],
        model: &str,
        event_tx: UnboundedSender<AgentEvent>,
    ) -> AgentResult {
        loop_::run_agent_loop(
            provider,
            messages,
            model,
            &self.config,
            &self.tool_registry,
            &self.permission_manager,
            Some(event_tx),
        )
        .await
    }
}
