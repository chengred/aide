#![allow(dead_code)]

use std::sync::Arc;
use tokio::task::JoinHandle;

use crate::llm::{LLMProvider, Message};
use crate::tools::permission::PermissionManager;
use crate::tools::ToolRegistry;

use super::{Agent, AgentConfig};

/// A sub-agent task definition
#[derive(Clone)]
pub struct SubAgentTask {
    /// Task description for the sub-agent
    pub description: String,
    /// Specific tools the sub-agent may use (if empty, inherits all)
    pub allowed_tools: Vec<String>,
    /// System prompt override
    pub system_prompt: Option<String>,
}

/// Result from a sub-agent execution
#[derive(Debug, Clone)]
pub struct SubAgentResult {
    pub task_id: String,
    pub description: String,
    pub content: String,
    pub tools_called: Vec<String>,
    pub turns: u32,
    pub success: bool,
    pub error: Option<String>,
}

/// Manages sub-agent lifecycle
pub struct SubAgentManager {
    max_parallel: usize,
    active_tasks: Vec<JoinHandle<SubAgentResult>>,
}

impl SubAgentManager {
    pub fn new(max_parallel: usize) -> Self {
        Self {
            max_parallel,
            active_tasks: Vec::new(),
        }
    }

    /// Spawn a single sub-agent to handle a specific task
    pub async fn spawn(
        provider: Arc<dyn LLMProvider>,
        config: &AgentConfig,
        tool_registry: &ToolRegistry,
        permission_manager: &PermissionManager,
        task: SubAgentTask,
    ) -> SubAgentResult {
        let task_id = uuid::Uuid::new_v4().to_string();

        // Build a restricted tool registry if needed
        let sub_tool_registry = if task.allowed_tools.is_empty() {
            tool_registry.clone()
        } else {
            let mut restricted = ToolRegistry::new();
            for name in &task.allowed_tools {
                if let Some(tool) = tool_registry.get(name) {
                    // Need to register by name - we'll use a workaround
                    restricted.register_arc(name.clone(), tool);
                }
            }
            restricted
        };

        // Build sub-agent messages
        let system_prompt = task
            .system_prompt
            .unwrap_or_else(|| config.system_prompt.clone());
        let messages = vec![
            Message::system(format!(
                "{}\n\nYou are a sub-agent handling a specific sub-task. Focus only on your assigned task. \
                 When you complete it, return your findings clearly and concisely. Do not ask follow-up questions.",
                system_prompt
            )),
            Message::user(format!("Sub-task: {}", task.description)),
        ];

        let agent = Agent::new(config.clone(), sub_tool_registry, permission_manager.clone());
        let model = config.system_prompt.clone(); // Use default model from config
        let _ = model; // model is from config

        let result = agent.run(provider.as_ref(), &messages, "gpt-4o").await;

        SubAgentResult {
            task_id,
            description: task.description,
            content: result.content,
            tools_called: result.tools_called,
            turns: result.turns,
            success: result.stop_reason.is_completed(),
            error: if result.stop_reason.is_completed() {
                None
            } else {
                Some(format!("{:?}", result.stop_reason))
            },
        }
    }

    /// Run multiple sub-agent tasks in parallel
    pub async fn run_parallel(
        provider: Arc<dyn LLMProvider>,
        config: &AgentConfig,
        tool_registry: &ToolRegistry,
        permission_manager: &PermissionManager,
        tasks: Vec<SubAgentTask>,
    ) -> Vec<SubAgentResult> {
        let handles: Vec<_> = tasks
            .into_iter()
            .map(|task| {
                let p = provider.clone();
                let cfg = config.clone();
                let tr = tool_registry.clone();
                let pm = permission_manager.clone();
                tokio::spawn(async move {
                    Self::spawn(p, &cfg, &tr, &pm, task).await
                })
            })
            .collect();

        let mut results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => results.push(SubAgentResult {
                    task_id: String::new(),
                    description: String::new(),
                    content: String::new(),
                    tools_called: Vec::new(),
                    turns: 0,
                    success: false,
                    error: Some(format!("Sub-agent panicked: {}", e)),
                }),
            }
        }
        results
    }
}

impl Default for SubAgentManager {
    fn default() -> Self {
        Self::new(4)
    }
}

impl super::StopReason {
    fn is_completed(&self) -> bool {
        matches!(self, super::StopReason::Completed)
    }
}

