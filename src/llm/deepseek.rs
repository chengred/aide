use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::Client;

use super::openai::messages_to_openai;
use super::provider::*;

/// DeepSeek provider - uses OpenAI-compatible API format
pub struct DeepSeekProvider {
    client: Client,
    api_key: String,
    base_url: String,
    default_model: String,
}

impl DeepSeekProvider {
    pub fn new(api_key: impl Into<String>, default_model: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: "https://api.deepseek.com/v1".into(),
            default_model: default_model.into(),
        }
    }
}

#[async_trait]
impl LLMProvider for DeepSeekProvider {
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

        let finish_reason = match choice["finish_reason"].as_str() {
            Some("stop") => FinishReason::Stop,
            Some("tool_calls") => FinishReason::ToolCalls,
            Some("length") => FinishReason::Length,
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
            cache_read_input_tokens: json["usage"]["prompt_cache_hit_tokens"]
                .as_u64()
                .map(|v| v as u32),
        };

        Ok(ChatResponse {
            content,
            tool_calls: None,
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
        });

        if let Some(temp) = options.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(max_tok) = options.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tok);
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
                        Some("length") => Some(FinishReason::Length),
                        Some(_) => None,
                        None => None,
                    };

                    futures::stream::iter(Some(Ok(StreamChunk { content, tool_call: None, finish_reason })))
                }
                Err(e) => futures::stream::iter(Some(Err(LLMError::Stream(e.to_string())))),
            }
        });

        Ok(Box::new(stream))
    }

    fn models(&self) -> Vec<String> {
        vec![
            "deepseek-chat".into(),
            "deepseek-reasoner".into(),
        ]
    }

    fn supports_tools(&self) -> bool {
        true
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::DeepSeek
    }
}
