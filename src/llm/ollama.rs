use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::Client;

use super::provider::*;

pub struct OllamaProvider {
    client: Client,
    base_url: String,
    default_model: String,
}

impl OllamaProvider {
    pub fn new(base_url: impl Into<String>, default_model: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into(),
            default_model: default_model.into(),
        }
    }

    /// List models available in the Ollama instance
    #[allow(dead_code)]
    pub async fn list_models(&self) -> Result<Vec<String>, LLMError> {
        let resp = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await?;
        let json: serde_json::Value = resp.json().await?;
        let models = json["models"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m["name"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        Ok(models)
    }
}

#[async_trait]
impl LLMProvider for OllamaProvider {
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

        let body = serde_json::json!({
            "model": model,
            "messages": messages_to_ollama(messages),
            "stream": false,
            "options": {
                "temperature": options.temperature.unwrap_or(0.7),
                "num_predict": options.max_tokens.unwrap_or(4096),
            }
        });

        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
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

        let json: serde_json::Value = resp.json().await?;

        let content = json["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let eval_count = json["eval_count"].as_u64().unwrap_or(0) as u32;
        let prompt_eval_count = json["prompt_eval_count"].as_u64().unwrap_or(0) as u32;

        Ok(ChatResponse {
            content,
            tool_calls: None,
            finish_reason: FinishReason::Stop,
            usage: Usage {
                prompt_tokens: prompt_eval_count,
                completion_tokens: eval_count,
                total_tokens: prompt_eval_count + eval_count,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
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

        let body = serde_json::json!({
            "model": model,
            "messages": messages_to_ollama(messages),
            "stream": true,
            "options": {
                "temperature": options.temperature.unwrap_or(0.7),
                "num_predict": options.max_tokens.unwrap_or(4096),
            }
        });

        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
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
            let result = match line {
                Ok(line) => {
                    if line.is_empty() {
                        return futures::stream::iter(None);
                    }
                    let json: serde_json::Value = match serde_json::from_str(&line) {
                        Ok(j) => j,
                        Err(_) => return futures::stream::iter(None),
                    };
                    let content = json["message"]["content"].as_str().map(String::from);
                    let done = json["done"].as_bool().unwrap_or(false);
                    let finish_reason = if done { Some(FinishReason::Stop) } else { None };
                    futures::stream::iter(Some(Ok(StreamChunk { content, tool_call: None, finish_reason })))
                }
                Err(e) => futures::stream::iter(Some(Err(LLMError::Stream(e.to_string())))),
            };
            result
        });

        Ok(Box::new(stream))
    }

    fn models(&self) -> Vec<String> {
        vec![self.default_model.clone()]
    }

    fn supports_tools(&self) -> bool {
        true
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Ollama
    }
}

fn messages_to_ollama(messages: &[Message]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": m.role.to_string(),
                "content": m.content,
            })
        })
        .collect()
}
