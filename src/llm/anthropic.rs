use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::Client;

use super::provider::*;

const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com/v1";
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    default_model: String,
}

impl AnthropicProvider {
    pub fn new(api_key: impl Into<String>, default_model: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            default_model: default_model.into(),
        }
    }
}

fn messages_to_anthropic(messages: &[Message], system: Option<&str>) -> (serde_json::Value, Vec<serde_json::Value>) {
    let mut system_msg = None;
    let mut converted = Vec::new();

    for m in messages {
        match m.role {
            Role::System => {
                system_msg = Some(m.content.clone());
            }
            Role::User => {
                converted.push(serde_json::json!({
                    "role": "user",
                    "content": m.content,
                }));
            }
            Role::Assistant => {
                let mut content: Vec<serde_json::Value> = Vec::new();
                if !m.content.is_empty() {
                    content.push(serde_json::json!({
                        "type": "text",
                        "text": m.content,
                    }));
                }
                if let Some(ref tool_calls) = m.tool_calls {
                    for tc in tool_calls {
                        content.push(serde_json::json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.function.name,
                            "input": serde_json::from_str::<serde_json::Value>(&tc.function.arguments).unwrap_or(serde_json::json!({})),
                        }));
                    }
                }
                converted.push(serde_json::json!({
                    "role": "assistant",
                    "content": content,
                }));
            }
            Role::Tool => {
                converted.push(serde_json::json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": m.tool_call_id,
                        "content": m.content,
                    }],
                }));
            }
        }
    }

    let system = system.or(system_msg.as_deref());
    let system_json = system.map(|s| serde_json::json!({"text": s}));

    (system_json.unwrap_or(serde_json::Value::Null), converted)
}

fn tools_to_anthropic(tools: &[ToolDefinition]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.function.name,
                "description": t.function.description,
                "input_schema": t.function.parameters,
            })
        })
        .collect()
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
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

        let (system, anthropic_messages) = messages_to_anthropic(messages, options.system.as_deref());

        let mut body = serde_json::json!({
            "model": model,
            "max_tokens": options.max_tokens.unwrap_or(4096),
            "messages": anthropic_messages,
        });

        if !system.is_null() {
            body["system"] = system;
        }
        if let Some(temp) = options.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(ref tools) = options.tools {
            if !tools.is_empty() {
                body["tools"] = serde_json::json!(tools_to_anthropic(tools));
            }
        }

        let resp = self
            .client
            .post(format!("{}/messages", ANTHROPIC_BASE_URL))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
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

        let mut content = String::new();
        let mut tool_calls: Option<Vec<ToolCall>> = None;

        for block in json["content"].as_array().unwrap_or(&vec![]) {
            match block["type"].as_str() {
                Some("text") => {
                    content.push_str(block["text"].as_str().unwrap_or(""));
                }
                Some("tool_use") => {
                    let tc = ToolCall {
                        id: block["id"].as_str().unwrap_or("").to_string(),
                        call_type: "function".to_string(),
                        function: FunctionCall {
                            name: block["name"].as_str().unwrap_or("").to_string(),
                            arguments: block["input"].to_string(),
                        },
                    };
                    tool_calls.get_or_insert_with(Vec::new).push(tc);
                }
                _ => {}
            }
        }

        let finish_reason = match json["stop_reason"].as_str() {
            Some("end_turn") => FinishReason::Stop,
            Some("tool_use") => FinishReason::ToolCalls,
            Some("max_tokens") => FinishReason::Length,
            Some("stop_sequence") => FinishReason::Stop,
            Some(other) => FinishReason::Unknown(other.to_string()),
            None => FinishReason::Stop,
        };

        let usage = Usage {
            prompt_tokens: json["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32,
            completion_tokens: json["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32,
            total_tokens: 0,
            cache_creation_input_tokens: json["usage"]["cache_creation_input_tokens"]
                .as_u64()
                .map(|v| v as u32),
            cache_read_input_tokens: json["usage"]["cache_read_input_tokens"]
                .as_u64()
                .map(|v| v as u32),
        };

        let total = usage.prompt_tokens + usage.completion_tokens;

        Ok(ChatResponse {
            content,
            tool_calls,
            finish_reason,
            usage: Usage {
                total_tokens: total,
                ..usage
            },
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

        let (system, anthropic_messages) = messages_to_anthropic(messages, options.system.as_deref());

        let mut body = serde_json::json!({
            "model": model,
            "max_tokens": options.max_tokens.unwrap_or(4096),
            "messages": anthropic_messages,
            "stream": true,
        });

        if !system.is_null() {
            body["system"] = system;
        }
        if let Some(temp) = options.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(ref tools) = options.tools {
            if !tools.is_empty() {
                body["tools"] = serde_json::json!(tools_to_anthropic(tools));
            }
        }

        let resp = self
            .client
            .post(format!("{}/messages", ANTHROPIC_BASE_URL))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
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
                    if line.is_empty() {
                        return futures::stream::iter(None);
                    }
                    let line = line.strip_prefix("data: ").unwrap_or(&line);
                    let json: serde_json::Value = match serde_json::from_str(line) {
                        Ok(j) => j,
                        Err(_) => return futures::stream::iter(None),
                    };

                    let event_type = json["type"].as_str().unwrap_or("");

                    match event_type {
                        "content_block_start" => {
                            let block = &json["content_block"];
                            if block["type"] == "tool_use" {
                                futures::stream::iter(Some(Ok(StreamChunk {
                                    content: None,
                                    tool_call: Some(ToolCallDelta {
                                        index: block["index"].as_u64().unwrap_or(0) as u32,
                                        id: block["id"].as_str().map(String::from),
                                        name: block["name"].as_str().map(String::from),
                                        arguments: Some(String::new()),
                                    }),
                                    finish_reason: None,
                                })))
                            } else {
                                futures::stream::iter(None)
                            }
                        }
                        "content_block_delta" => {
                            let delta = &json["delta"];
                            match delta["type"].as_str() {
                                Some("text_delta") => futures::stream::iter(Some(Ok(StreamChunk {
                                    content: delta["text"].as_str().map(String::from),
                                    tool_call: None,
                                    finish_reason: None,
                                }))),
                                Some("input_json_delta") => futures::stream::iter(Some(Ok(StreamChunk {
                                    content: None,
                                    tool_call: Some(ToolCallDelta {
                                        index: delta.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                                        id: None,
                                        name: None,
                                        arguments: delta["partial_json"].as_str().map(String::from),
                                    }),
                                    finish_reason: None,
                                }))),
                                _ => futures::stream::iter(None),
                            }
                        }
                        "message_stop" => futures::stream::iter(Some(Ok(StreamChunk {
                            content: None,
                            tool_call: None,
                            finish_reason: Some(FinishReason::Stop),
                        }))),
                        "error" => futures::stream::iter(Some(Err(LLMError::Provider {
                            status: 200,
                            body: json["error"]["message"]
                                .as_str()
                                .unwrap_or("unknown error")
                                .to_string(),
                        }))),
                        _ => futures::stream::iter(None),
                    }
                }
                Err(e) => futures::stream::iter(Some(Err(LLMError::Stream(e.to_string())))),
            }
        });

        Ok(Box::new(stream))
    }

    fn models(&self) -> Vec<String> {
        vec![
            "claude-sonnet-4-6".into(),
            "claude-opus-4-7".into(),
            "claude-haiku-4-5".into(),
        ]
    }

    fn supports_tools(&self) -> bool {
        true
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Anthropic
    }
}
