pub mod builtin;
pub mod permission;
pub mod planning;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::llm::ToolDefinition;

/// Result of executing a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub content: String,
    pub error: Option<String>,
}

impl ToolResult {
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            success: true,
            content: content.into(),
            error: None,
        }
    }

    pub fn err(error: impl Into<String>) -> Self {
        Self {
            success: false,
            content: String::new(),
            error: Some(error.into()),
        }
    }
}

/// The core Tool trait
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique name of the tool
    fn name(&self) -> &str;

    /// Human-readable description
    fn description(&self) -> &str;

    /// JSON Schema for the tool's parameters
    fn parameters(&self) -> serde_json::Value;

    /// Execute the tool with the given parameters
    async fn execute(&self, params: serde_json::Value) -> ToolResult;

    /// Convert to an LLM-compatible ToolDefinition
    fn to_definition(&self) -> ToolDefinition {
        ToolDefinition::new(self.name(), self.description(), self.parameters())
    }

    /// Whether this tool requires user approval before execution
    fn requires_approval(&self) -> bool {
        false
    }
}

/// Registry of all available tools
#[derive(Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool
    pub fn register(&mut self, tool: impl Tool + 'static) {
        let name = tool.name().to_string();
        self.tools.insert(name, Arc::new(tool));
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// List all registered tool names
    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// Get all tools as LLM-compatible definitions
    pub fn to_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.to_definition()).collect()
    }

    /// Register a tool that is already Arc-wrapped
    #[allow(dead_code)]
    pub fn register_arc(&mut self, name: String, tool: Arc<dyn Tool>) {
        self.tools.insert(name, tool);
    }

    /// Check if a tool requires approval
    pub fn requires_approval(&self, name: &str) -> bool {
        self.tools
            .get(name)
            .map(|t| t.requires_approval())
            .unwrap_or(false)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
