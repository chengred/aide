#![allow(dead_code)]

use crate::llm::Message;
use crate::llm::Role;
use crate::utils::token_counter::TokenCounter;

/// Manages conversation context with sliding window and compression
pub struct ContextManager {
    /// Maximum number of messages to keep in full context
    max_messages: usize,
    /// Token threshold that triggers compression
    token_threshold: usize,
    /// Number of recent turns to preserve in full after compression
    recent_turns_to_keep: usize,
}

impl ContextManager {
    pub fn new(max_messages: usize, token_threshold: usize, recent_turns_to_keep: usize) -> Self {
        Self {
            max_messages,
            token_threshold,
            recent_turns_to_keep,
        }
    }

    /// Check if the context needs compression
    pub fn needs_compression(&self, messages: &[Message]) -> bool {
        let counter = TokenCounter::default();
        let estimated_tokens = counter.count_messages(messages);

        estimated_tokens > self.token_threshold || messages.len() > self.max_messages
    }

    /// Compress the context by summarizing older messages
    pub fn compress(&self, messages: &mut Vec<Message>) {
        if messages.len() <= self.recent_turns_to_keep * 2 + 2 {
            return; // Not enough messages to compress
        }

        // Count turns (pairs of user + assistant)
        let mut turn_count = 0;
        let mut split_idx = 0;
        for (i, msg) in messages.iter().enumerate() {
            if matches!(msg.role, Role::User) {
                turn_count += 1;
            }
            if turn_count > self.recent_turns_to_keep {
                split_idx = i;
            }
        }

        if split_idx <= 1 {
            return;
        }

        // Summarize old messages
        let old_messages: Vec<&Message> = messages[..split_idx].iter().collect();
        let summary = self.summarize(&old_messages);

        // Replace old messages with a single system summary
        let recent: Vec<Message> = messages.drain(split_idx..).collect();
        messages.clear();

        // Keep the original system message if it exists
        if let Some(first) = messages.first() {
            if matches!(first.role, Role::System) {
                messages.push(first.clone());
            }
        }

        messages.push(Message::system(format!(
            "[Context Summary] Previous conversation summary: {}",
            summary
        )));
        messages.extend(recent);
    }

    /// Generate a simple summary of messages
    fn summarize(&self, messages: &[&Message]) -> String {
        let user_messages: Vec<&str> = messages
            .iter()
            .filter(|m| matches!(m.role, Role::User))
            .map(|m| m.content.as_str())
            .collect();

        if user_messages.is_empty() {
            return "No previous user messages.".into();
        }

        let mut summary = format!(
            "The conversation covered {} user queries. ",
            user_messages.len()
        );

        // Include truncated versions of user queries
        let previews: Vec<String> = user_messages
            .iter()
            .take(5)
            .map(|m| {
                let truncated: String = m.chars().take(100).collect();
                if m.len() > 100 {
                    format!("\"{}\"...", truncated)
                } else {
                    format!("\"{}\"", truncated)
                }
            })
            .collect();

        summary.push_str("Key topics: ");
        summary.push_str(&previews.join("; "));
        summary
    }

    /// Estimate token count for a set of messages
    pub fn estimate_tokens(messages: &[Message]) -> usize {
        let counter = TokenCounter::default();
        counter.count_messages(messages)
    }

    /// Get the current token threshold
    pub fn token_threshold(&self) -> usize {
        self.token_threshold
    }
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::new(500, 100_000, 10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{Message, Role};

    fn make_messages(count: usize) -> Vec<Message> {
        let mut msgs = Vec::new();
        msgs.push(Message::system("test system"));
        for i in 0..count {
            msgs.push(Message::user(format!("user message {}", i)));
            msgs.push(Message::assistant(format!("assistant response {}", i)));
        }
        msgs
    }

    #[test]
    fn test_estimate_tokens() {
        let msgs = vec![
            Message::user("hello world"),
            Message::assistant("hi there"),
        ];
        let tokens = ContextManager::estimate_tokens(&msgs);
        assert!(tokens > 0);
        // Now uses TokenCounter, result depends on model-aware estimation
        assert!(tokens >= 6, "expected at least 6 tokens, got {}", tokens);
    }

    #[test]
    fn test_needs_compression_false_for_small_context() {
        let cm = ContextManager::default();
        let msgs = make_messages(5);
        assert!(!cm.needs_compression(&msgs));
    }

    #[test]
    fn test_needs_compression_true_for_large_context() {
        let cm = ContextManager::new(10, 50, 3);
        let msgs = make_messages(20);
        assert!(cm.needs_compression(&msgs));
    }

    #[test]
    fn test_compress_reduces_messages() {
        let cm = ContextManager::new(50, 1000, 2);
        let mut msgs = make_messages(10);
        let original_len = msgs.len();
        cm.compress(&mut msgs);
        assert!(msgs.len() < original_len);
    }

    #[test]
    fn test_compress_preserves_recent() {
        let cm = ContextManager::new(50, 1000, 3);
        let mut msgs = make_messages(10);
        let last_msg = msgs.last().unwrap().content.clone();
        cm.compress(&mut msgs);
        assert_eq!(msgs.last().unwrap().content, last_msg);
    }

    #[test]
    fn test_compress_keeps_system_message() {
        let cm = ContextManager::new(50, 1000, 2);
        let mut msgs = make_messages(8);
        cm.compress(&mut msgs);
        assert_eq!(msgs[0].role, Role::System);
    }
}

