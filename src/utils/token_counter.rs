#![allow(dead_code)]

use crate::llm::Message;

/// Supported tokenizer types
#[derive(Debug, Clone, Copy)]
pub enum TokenizerType {
    /// OpenAI models (GPT-4, GPT-3.5, etc.)
    OpenAI,
    /// Anthropic Claude models
    Claude,
    /// DeepSeek models
    DeepSeek,
    /// Generic estimation (conservative)
    Generic,
}

/// Token counter with model-aware estimation
pub struct TokenCounter {
    tokenizer_type: TokenizerType,
}

impl TokenCounter {
    /// Create a token counter for a specific model family
    pub fn new(tokenizer_type: TokenizerType) -> Self {
        Self { tokenizer_type }
    }

    /// Create from a model name
    pub fn from_model(model: &str) -> Self {
        let lower = model.to_lowercase();
        let tokenizer_type = if lower.starts_with("gpt-") || lower.starts_with("o1") || lower.starts_with("o3") {
            TokenizerType::OpenAI
        } else if lower.starts_with("claude-") {
            TokenizerType::Claude
        } else if lower.starts_with("deepseek") {
            TokenizerType::DeepSeek
        } else {
            TokenizerType::Generic
        };
        Self { tokenizer_type }
    }

    /// Estimate token count for a single string
    pub fn count(&self, text: &str) -> usize {
        // Average chars per token varies by model family
        // Based on empirical measurements
        let chars_per_token = match self.tokenizer_type {
            TokenizerType::OpenAI => 3.5, // GPT tokenizer is more efficient on English text
            TokenizerType::Claude => 3.8, // Claude tokenizer
            TokenizerType::DeepSeek => 3.5, // Similar to OpenAI
            TokenizerType::Generic => 3.5, // Conservative default
        };

        // Adjust for code content (code is more token-dense)
        let code_ratio = Self::code_ratio(text);

        (text.len() as f64 / chars_per_token * code_ratio) as usize
    }

    /// Estimate tokens for a slice of messages
    pub fn count_messages(&self, messages: &[Message]) -> usize {
        // Base tokens per message (role + formatting overhead)
        const TOKENS_PER_MESSAGE: usize = 4;

        messages
            .iter()
            .map(|m| {
                TOKENS_PER_MESSAGE + self.count(&m.content)
                    + m.tool_calls.as_ref().map(|tcs| {
                        tcs.iter().map(|tc| self.count(&tc.function.arguments) + self.count(&tc.function.name) + 8).sum::<usize>()
                    }).unwrap_or(0)
            })
            .sum()
    }

    /// Estimate token count for tool definitions
    pub fn count_tool_definitions(&self, tools: &[crate::llm::ToolDefinition]) -> usize {
        tools
            .iter()
            .map(|t| {
                self.count(&t.function.name)
                    + self.count(&t.function.description)
                    + self.count(&t.function.parameters.to_string())
                    + 10
            })
            .sum()
    }

    /// Estimate token count for a complete request
    pub fn estimate_request_tokens(
        &self,
        messages: &[Message],
        tools: Option<&[crate::llm::ToolDefinition]>,
        system_prompt: Option<&str>,
    ) -> usize {
        let mut total = self.count_messages(messages);

        if let Some(tools) = tools {
            total += self.count_tool_definitions(tools);
        }

        if let Some(sys) = system_prompt {
            total += self.count(sys) + 8; // system message overhead
        }

        // Add overhead for JSON formatting, stop sequences, etc.
        total + 20
    }

    /// Read the tokenizer type
    pub fn tokenizer_type(&self) -> TokenizerType {
        self.tokenizer_type
    }

    /// Determine the ratio of code to natural language in text
    /// Code is more token-dense (~2.5 chars/token)
    fn code_ratio(text: &str) -> f64 {
        let total = text.len() as f64;
        if total == 0.0 {
            return 1.0;
        }

        let code_chars = text
            .chars()
            .filter(|c| matches!(c, '{' | '}' | '(' | ')' | '[' | ']' | ';' | ':' | '=' | '<' | '>' | '/' | '\\' | '|' | '&' | '!' | '.' | ',' | '\'' | '"' | '`'))
            .count() as f64;

        let code_ratio = code_chars / total;

        // Blend: if 30%+ special chars, it's likely code
        if code_ratio > 0.3 {
            // Code tokens are denser: ~2.5 chars/token vs ~3.5 for prose
            1.0 + (code_ratio - 0.3) * 1.5
        } else {
            1.0
        }
    }
}

impl Default for TokenCounter {
    fn default() -> Self {
        Self::new(TokenizerType::Generic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_english() {
        let counter = TokenCounter::new(TokenizerType::OpenAI);
        let text = "Hello, how are you today?";
        let tokens = counter.count(text);
        assert!(tokens > 0 && tokens <= text.len());
    }

    #[test]
    fn test_count_code() {
        let counter = TokenCounter::new(TokenizerType::Generic);
        let text = "fn main() { println!(\"hello\"); }";
        let tokens = counter.count(text);
        assert!(tokens > 0);
    }

    #[test]
    fn test_count_empty() {
        let counter = TokenCounter::default();
        assert_eq!(counter.count(""), 0);
    }

    #[test]
    fn test_count_messages() {
        let counter = TokenCounter::new(TokenizerType::Claude);
        let messages = vec![
            Message::system("You are helpful."),
            Message::user("Hello!"),
        ];
        let tokens = counter.count_messages(&messages);
        assert!(tokens > 0);
    }

    #[test]
    fn test_from_model() {
        let c1 = TokenCounter::from_model("gpt-4o");
        assert!(matches!(c1.tokenizer_type(), TokenizerType::OpenAI));

        let c2 = TokenCounter::from_model("claude-sonnet-4-6");
        assert!(matches!(c2.tokenizer_type(), TokenizerType::Claude));

        let c3 = TokenCounter::from_model("unknown-model");
        assert!(matches!(c3.tokenizer_type(), TokenizerType::Generic));
    }
}
