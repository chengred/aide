use serde::{Deserialize, Serialize};

/// The role of a message in a conversation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::System => write!(f, "system"),
            Role::User => write!(f, "user"),
            Role::Assistant => write!(f, "assistant"),
            Role::Tool => write!(f, "tool"),
        }
    }
}

/// A single message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    #[allow(dead_code)]
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
            name: None,
        }
    }
}

/// A tool call requested by the model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// A tool definition provided to the model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub def_type: String,
    pub function: FunctionDef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

impl ToolDefinition {
    pub fn new(name: &str, description: &str, parameters: serde_json::Value) -> Self {
        Self {
            def_type: "function".into(),
            function: FunctionDef {
                name: name.into(),
                description: description.into(),
                parameters,
            },
        }
    }
}

/// Options for a chat completion request
#[derive(Debug, Clone)]
pub struct ChatOptions {
    pub model: String,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub tools: Option<Vec<ToolDefinition>>,
    pub system: Option<String>,
}

impl Default for ChatOptions {
    fn default() -> Self {
        Self {
            model: "gpt-4o".into(),
            temperature: None,
            max_tokens: None,
            tools: None,
            system: None,
        }
    }
}

/// The response from a chat completion
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub finish_reason: FinishReason,
    pub usage: Usage,
}

/// Why the model stopped generating
#[derive(Debug, Clone, PartialEq)]
pub enum FinishReason {
    Stop,
    ToolCalls,
    Length,
    ContentFilter,
    Unknown(String),
}

/// Token usage information
#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    #[allow(dead_code)]
    pub cache_creation_input_tokens: Option<u32>,
    #[allow(dead_code)]
    pub cache_read_input_tokens: Option<u32>,
}

/// A chunk of streaming response
#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub content: Option<String>,
    pub tool_call: Option<ToolCallDelta>,
    pub finish_reason: Option<FinishReason>,
}

#[derive(Debug, Clone)]
pub struct ToolCallDelta {
    pub index: u32,
    pub id: Option<String>,
    pub name: Option<String>,
    pub arguments: Option<String>,
}

/// Supported provider types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderType {
    #[serde(rename = "openai")]
    OpenAI,
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "deepseek")]
    DeepSeek,
    #[serde(rename = "ollama")]
    Ollama,
    #[serde(rename = "candle")]
    Candle,
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderType::OpenAI => write!(f, "openai"),
            ProviderType::Anthropic => write!(f, "anthropic"),
            ProviderType::DeepSeek => write!(f, "deepseek"),
            ProviderType::Ollama => write!(f, "ollama"),
            ProviderType::Candle => write!(f, "candle"),
        }
    }
}

impl std::str::FromStr for ProviderType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "openai" => Ok(ProviderType::OpenAI),
            "anthropic" => Ok(ProviderType::Anthropic),
            "deepseek" => Ok(ProviderType::DeepSeek),
            "ollama" => Ok(ProviderType::Ollama),
            "candle" => Ok(ProviderType::Candle),
            _ => Err(format!("Unknown provider: {}. Valid options: openai, anthropic, deepseek, ollama, candle", s)),
        }
    }
}

/// The unified LLM Provider trait
#[async_trait::async_trait]
pub trait LLMProvider: Send + Sync {
    /// Send a chat completion request
    async fn chat(
        &self,
        messages: &[Message],
        options: &ChatOptions,
    ) -> Result<ChatResponse, LLMError>;

    /// Stream a chat completion
    async fn stream(
        &self,
        messages: &[Message],
        options: &ChatOptions,
    ) -> Result<Box<dyn futures::Stream<Item = Result<StreamChunk, LLMError>> + Send + Unpin>, LLMError>;

    /// List available models for this provider
    fn models(&self) -> Vec<String>;

    /// Whether this provider supports tool calling
    #[allow(dead_code)]
    fn supports_tools(&self) -> bool;

    /// The provider type
    #[allow(dead_code)]
    fn provider_type(&self) -> ProviderType;
}

/// Errors that can occur during LLM operations
#[derive(Debug, thiserror::Error)]
pub enum LLMError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Provider error: {status}, body: {body}")]
    Provider { status: u16, body: String },

    #[error("Stream error: {0}")]
    Stream(String),

    #[error("Configuration error: {0}")]
    #[allow(dead_code)]
    Config(String),

    #[error("Rate limited. Retry after {retry_after:?} seconds")]
    RateLimit { retry_after: Option<u64> },

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("{0}")]
    #[allow(dead_code)]
    Other(String),
}
