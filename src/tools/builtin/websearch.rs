use async_trait::async_trait;
use serde_json::json;

use super::super::{Tool, ToolResult};

pub struct WebSearchTool;

impl WebSearchTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "websearch"
    }

    fn description(&self) -> &str {
        "Search the web for information. Returns search results with titles and URLs. \
         Use for finding documentation, current events, and technical references."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> ToolResult {
        let query = match params["query"].as_str() {
            Some(q) => q,
            None => return ToolResult::err("Missing required parameter: query"),
        };

        // Use DuckDuckGo's HTML search (no API key required)
        let url = format!(
            "https://html.duckduckgo.com/html/?q={}",
            urlencoding(query)
        );

        let client = match reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (compatible; rustcc/0.1)")
            .timeout(std::time::Duration::from_secs(15))
            .build()
        {
            Ok(c) => c,
            Err(e) => return ToolResult::err(format!("Failed to create HTTP client: {}", e)),
        };

        let resp = match client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => return ToolResult::err(format!("Search request failed: {}", e)),
        };

        if !resp.status().is_success() {
            return ToolResult::err(format!(
                "Search returned HTTP {}",
                resp.status().as_u16()
            ));
        }

        let html = match resp.text().await {
            Ok(h) => h,
            Err(e) => return ToolResult::err(format!("Failed to read response: {}", e)),
        };

        let results = parse_ddg_results(&html);

        if results.is_empty() {
            return ToolResult::ok(format!("No search results found for: {}", query));
        }

        let mut output = format!("Search results for: {}\n\n", query);
        for (i, result) in results.iter().enumerate() {
            output.push_str(&format!(
                "{}. {}\n   {}\n   {}\n\n",
                i + 1,
                result.title,
                result.snippet,
                result.url
            ));
        }

        output.push_str(&format!(
            "Sources: {}",
            results
                .iter()
                .map(|r| format!("- [{}]({})", r.title, r.url))
                .collect::<Vec<_>>()
                .join("\n")
        ));

        ToolResult::ok(output)
    }

    fn requires_approval(&self) -> bool {
        true
    }
}

struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' || c == ' ' {
                if c == ' ' {
                    '+'.to_string()
                } else {
                    c.to_string()
                }
            } else {
                format!("%{:02X}", c as u8)
            }
        })
        .collect()
}

fn parse_ddg_results(html: &str) -> Vec<SearchResult> {
    let mut results = Vec::new();

    // Parse DuckDuckGo HTML results
    // Look for result__title and result__snippet classes
    let mut titles: Vec<String> = Vec::new();
    let mut snippets: Vec<String> = Vec::new();
    let mut urls: Vec<String> = Vec::new();

    // Extract titles
    let mut search = html;
    while let Some(start) = search.find("result__title") {
        search = &search[start..];
        if let Some(link_start) = search.find("href=\"") {
            let after_href = &search[link_start + 6..];
            if let Some(link_end) = after_href.find('"') {
                let href = &after_href[..link_end];
                let clean_url = unescape_html(href);
                if !clean_url.is_empty() && clean_url.starts_with("http") {
                    urls.push(clean_url);
                }
            }
            // Find title text
            if let Some(tag_end) = search.find('>') {
                let after_open = &search[tag_end + 1..];
                if let Some(title_end) = after_open.find("</a>") {
                    let title = unescape_html(&after_open[..title_end]);
                    let title = title.trim().to_string();
                    if !title.is_empty() {
                        titles.push(title);
                    }
                }
            }
        }
        search = &search[1..];
    }

    // Extract snippets
    search = html;
    while let Some(start) = search.find("result__snippet") {
        search = &search[start..];
        if let Some(tag_end) = search.find('>') {
            let after_open = &search[tag_end + 1..];
            if let Some(snip_end) = after_open.find("</") {
                let snippet = unescape_html(&after_open[..snip_end]);
                let snippet = snippet.trim().to_string();
                if !snippet.is_empty() {
                    snippets.push(snippet);
                }
            }
        }
        search = &search[1..];
    }

    // Combine
    let count = titles.len().min(snippets.len()).min(urls.len());
    for i in 0..count.min(10) {
        results.push(SearchResult {
            title: titles[i].clone(),
            url: urls[i].clone(),
            snippet: snippets[i].clone(),
        });
    }

    results
}

fn unescape_html(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
}
