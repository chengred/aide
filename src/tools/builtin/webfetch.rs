use async_trait::async_trait;
use serde_json::json;

use super::super::{Tool, ToolResult};

pub struct WebFetchTool;

impl WebFetchTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "webfetch"
    }

    fn description(&self) -> &str {
        "Fetch content from a URL and process into markdown. Use for reading documentation, \
         API references, and web pages. Do NOT use for authenticated/private URLs."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch content from"
                },
                "prompt": {
                    "type": "string",
                    "description": "What information to extract from the page"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> ToolResult {
        let url = match params["url"].as_str() {
            Some(u) => u,
            None => return ToolResult::err("Missing required parameter: url"),
        };

        let client = reqwest::Client::builder()
            .user_agent("aide/0.1")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e));

        let client = match client {
            Ok(c) => c,
            Err(e) => return ToolResult::err(e),
        };

        let resp = match client.get(url).send().await {
            Ok(r) => r,
            Err(e) => return ToolResult::err(format!("HTTP request failed: {}", e)),
        };

        if !resp.status().is_success() {
            return ToolResult::err(format!(
                "HTTP {}: {}",
                resp.status().as_u16(),
                resp.status().canonical_reason().unwrap_or("unknown")
            ));
        }

        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("text/plain");

        // Handle HTML → text conversion
        let text = if content_type.contains("text/html") {
            let html = match resp.text().await {
                Ok(h) => h,
                Err(e) => return ToolResult::err(format!("Failed to read response: {}", e)),
            };
            html_to_text(&html)
        } else {
            match resp.text().await {
                Ok(t) => t,
                Err(e) => return ToolResult::err(format!("Failed to read response: {}", e)),
            }
        };

        // Truncate very long responses
        let max_len = 50_000;
        let truncated = if text.len() > max_len {
            format!("{}...\n\n[Content truncated at {} characters]", &text[..max_len], max_len)
        } else {
            text
        };

        let prompt = params["prompt"].as_str();
        let output = if let Some(p) = prompt {
            format!("URL: {}\nExtraction prompt: {}\n\nContent:\n{}", url, p, truncated)
        } else {
            format!("URL: {}\n\nContent:\n{}", url, truncated)
        };

        ToolResult::ok(output)
    }

    fn requires_approval(&self) -> bool {
        true
    }
}

/// Simple HTML to plain text converter
fn html_to_text(html: &str) -> String {
    // Remove script and style sections
    let mut text = String::new();
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let mut tag_name = String::new();
    let mut last_was_newline = false;
    let mut last_was_space = false;

    let lower = html.to_lowercase();
    let chars: Vec<char> = html.chars().collect();
    let _lower_chars: Vec<char> = lower.chars().collect();

    for i in 0..chars.len() {
        let ch = chars[i];

        if ch == '<' {
            in_tag = true;
            tag_name.clear();
            continue;
        }

        if in_tag {
            if ch == '>' {
                in_tag = false;
                let tag = tag_name.trim().to_lowercase();
                if tag == "script" || tag == "style" {
                    in_script = tag == "script";
                    in_style = tag == "style";
                }
                if tag.starts_with("/script") {
                    in_script = false;
                }
                if tag.starts_with("/style") {
                    in_style = false;
                }
                // Block-level tags → newline
                if matches!(
                    tag.as_str(),
                    "br" | "p" | "div" | "li" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                        | "tr" | "article" | "section" | "blockquote"
                ) || tag.starts_with("/")
                {
                    if !last_was_newline {
                        text.push('\n');
                        last_was_newline = true;
                    }
                }
                tag_name.clear();
            } else {
                tag_name.push(ch);
            }
            continue;
        }

        if in_script || in_style {
            continue;
        }

        // Handle common HTML entities
        if ch == '&' {
            let mut entity = String::new();
            let mut j = i;
            while j < chars.len() && chars[j] != ';' && (j - i) < 10 {
                entity.push(chars[j]);
                j += 1;
            }
            entity.push(';');
            match entity.as_str() {
                "&amp;" => text.push('&'),
                "&lt;" => text.push('<'),
                "&gt;" => text.push('>'),
                "&quot;" => text.push('"'),
                "&#39;" | "&apos;" => text.push('\''),
                "&nbsp;" => {
                    if !last_was_space {
                        text.push(' ');
                        last_was_space = true;
                    }
                }
                _ => {}
            }
            continue;
        }

        if ch.is_whitespace() {
            if !last_was_space && !last_was_newline {
                text.push(' ');
                last_was_space = true;
            }
        } else {
            text.push(ch);
            last_was_newline = false;
            last_was_space = false;
        }
    }

    // Clean up: collapse multiple newlines
    let mut result = String::new();
    let mut blank_count = 0;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            blank_count += 1;
            if blank_count <= 1 {
                result.push('\n');
            }
        } else {
            blank_count = 0;
            result.push_str(trimmed);
            result.push('\n');
        }
    }

    result.trim().to_string()
}
