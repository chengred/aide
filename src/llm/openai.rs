use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::Client;

use super::provider::*;

pub struct OpenAIProvider {
    client: Client,
    base_url: String,
    api_key: String,
    default_model: String,
}

impl OpenAIProvider {
    pub fn new(
        api_key: impl Into<String>,
        base_url: Option<String>,
        default_model: impl Into<String>,
    ) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com/v1".into()),
            api_key: api_key.into(),
            default_model: default_model.into(),
        }
    }
}

/// Serialize messages to OpenAI-compatible JSON format
pub fn messages_to_openai(messages: &[Message]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .map(|m| {
            let mut obj = serde_json::json!({
                "role": m.role.to_string(),
                "content": m.content,
            });

            if let Some(ref tool_calls) = m.tool_calls {
                let calls: Vec<_> = tool_calls
                    .iter()
                    .map(|tc| {
                        serde_json::json!({
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.function.name,
                                "arguments": tc.function.arguments,
                            }
                        })
                    })
                    .collect();
                obj["tool_calls"] = serde_json::json!(calls);
            }

            if let Some(ref tool_call_id) = m.tool_call_id {
                obj["tool_call_id"] = tool_call_id.as_str().into();
            }

            if let Some(ref name) = m.name {
                obj["name"] = name.as_str().into();
            }

            obj
        })
        .collect()
}

/// Serialize tool definitions to OpenAI-compatible JSON format
pub fn tools_to_openai(tools: &[ToolDefinition]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.function.name,
                    "description": t.function.description,
                    "parameters": t.function.parameters,
                }
            })
        })
        .collect()
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    async fn chat(
        &self,
        messages: &[Message],
        options: &ChatOptions,
    ) -> Result<ChatResponse, LLMError> {
        let model = if options.model.is_empty() {
            &self.default_model
        } else {
            &options.model
        };

        let mut body = serde_json::json!({
            "model": model,
            "messages": messages_to_openai(messages),
            "stream": false,
        });

        if let Some(temp) = options.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(max_tok) = options.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tok);
        }
        if let Some(ref tools) = options.tools {
            if !tools.is_empty() {
                body["tools"] = serde_json::json!(tools_to_openai(tools));
            }
        }

        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body_text = resp.text().await.unwrap_or_default();

            if status == 401 || status == 403 {
                return Err(LLMError::Auth(body_text));
            }
            if status == 429 {
                return Err(LLMError::RateLimit { retry_after: None });
            }

            return Err(LLMError::Provider {
                status,
                body: body_text,
            });
        }

        let json: serde_json::Value = resp.json().await?;

        let choice = &json["choices"][0];
        let message = &choice["message"];

        let content = message["content"].as_str().unwrap_or("").to_string();

        let tool_calls = message["tool_calls"].as_array().map(|arr| {
            arr.iter()
                .map(|tc| ToolCall {
                    id: tc["id"].as_str().unwrap_or("").to_string(),
                    call_type: tc["type"].as_str().unwrap_or("function").to_string(),
                    function: FunctionCall {
                        name: tc["function"]["name"].as_str().unwrap_or("").to_string(),
                        arguments: tc["function"]["arguments"].as_str().unwrap_or("{}").to_string(),
                    },
                })
                .collect()
        });

        let finish_reason = match choice["finish_reason"].as_str() {
            Some("stop") => FinishReason::Stop,
            Some("tool_calls") => FinishReason::ToolCalls,
            Some("length") => FinishReason::Length,
            Some("content_filter") => FinishReason::ContentFilter,
            Some(other) => FinishReason::Unknown(other.to_string()),
            None => FinishReason::Stop,
        };

        let usage = Usage {
            prompt_tokens: json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            completion_tokens: json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32,
            total_tokens: json["usage"]["total_tokens"].as_u64().unwrap_or(0) as u32,
            cache_creation_input_tokens: json["usage"]["prompt_tokens_details"]["cached_tokens"]
                .as_u64()
                .map(|v| v as u32),
            cache_read_input_tokens: json["usage"]["prompt_tokens_details"]["cached_tokens"]
                .as_u64()
                .map(|v| v as u32),
        };

        Ok(ChatResponse {
            content,
            tool_calls,
            finish_reason,
            usage,
        })
    }

    async fn stream(
        &self,
        messages: &[Message],
        options: &ChatOptions,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunk, LLMError>> + Send + Unpin>, LLMError> {
        let model = if options.model.is_empty() {
            &self.default_model
        } else {
            &options.model
        };

        let mut body = serde_json::json!({
            "model": model,
            "messages": messages_to_openai(messages),
            "stream": true,
            "stream_options": { "include_usage": true }
        });

        if let Some(temp) = options.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(max_tok) = options.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tok);
        }
        if let Some(ref tools) = options.tools {
            if !tools.is_empty() {
                body["tools"] = serde_json::json!(tools_to_openai(tools));
            }
        }

        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body_text = resp.text().await.unwrap_or_default();
            return Err(LLMError::Provider {
                status,
                body: body_text,
            });
        }

        use tokio_stream::wrappers::LinesStream;
        use tokio::io::AsyncBufReadExt;
        use std::io;

        let byte_stream = resp
            .bytes_stream()
            .map(|r| r.map_err(|e| io::Error::new(io::ErrorKind::Other, e)));
        let stream_reader = tokio_util::io::StreamReader::new(byte_stream);
        let lines = LinesStream::new(tokio::io::BufReader::new(stream_reader).lines());

        let stream = lines.flat_map(|line| {
            match line {
                Ok(line) => {
                    if line.is_empty() || line == "data: [DONE]" {
                        return futures::stream::iter(None);
                    }
                    let line = line.strip_prefix("data: ").unwrap_or(&line);
                    let json: serde_json::Value = match serde_json::from_str(line) {
                        Ok(j) => j,
                        Err(_) => return futures::stream::iter(None),
                    };

                    let choice = &json["choices"][0];
                    let delta = &choice["delta"];

                    let content = delta["content"].as_str().map(String::from);

                    let finish_reason = match choice["finish_reason"].as_str() {
                        Some("stop") => Some(FinishReason::Stop),
                        Some("tool_calls") => Some(FinishReason::ToolCalls),
                        Some("length") => Some(FinishReason::Length),
                        Some("content_filter") => Some(FinishReason::ContentFilter),
                        Some(_) => None,
                        None => None,
                    };

                    let tool_call = delta["tool_calls"].as_array().and_then(|arr| {
                        arr.first().map(|tc| ToolCallDelta {
                            index: tc["index"].as_u64().unwrap_or(0) as u32,
                            id: tc["id"].as_str().map(String::from),
                            name: tc["function"]["name"].as_str().map(String::from),
                            arguments: tc["function"]["arguments"].as_str().map(String::from),
                        })
                    });

                    futures::stream::iter(Some(Ok(StreamChunk { content, tool_call, finish_reason })))
                }
                Err(e) => futures::stream::iter(Some(Err(LLMError::Stream(e.to_string())))),
            }
        });

        Ok(Box::new(stream))
    }

    fn models(&self) -> Vec<String> {
        vec![
            "gpt-4o".into(),
            "gpt-4o-mini".into(),
            "gpt-4-turbo".into(),
            "gpt-3.5-turbo".into(),
        ]
    }

    fn supports_tools(&self) -> bool {
        true
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::OpenAI
    }
}
