use async_trait::async_trait;
use serde_json::json;

use super::super::{Tool, ToolResult};

pub struct ReadTool;

impl ReadTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }

    fn description(&self) -> &str {
        "Read the contents of a file at the given path. Returns the file content with line numbers."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to read"
                },
                "offset": {
                    "type": "integer",
                    "description": "Line number to start reading from (0-indexed)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to read"
                }
            },
            "required": ["file_path"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> ToolResult {
        let file_path = match params["file_path"].as_str() {
            Some(p) => p,
            None => return ToolResult::err("Missing required parameter: file_path"),
        };

        let path = std::path::Path::new(file_path);
        if !path.exists() {
            return ToolResult::err(format!("File not found: {}", file_path));
        }
        if !path.is_file() {
            return ToolResult::err(format!("Not a file: {}", file_path));
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => return ToolResult::err(format!("Failed to read file: {}", e)),
        };

        let offset = params["offset"].as_u64().unwrap_or(0) as usize;
        let limit = params["limit"].as_u64().map(|v| v as usize);

        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();

        let start = offset.min(total);
        let end = limit.map(|l| (start + l).min(total)).unwrap_or(total);
        let snippet: Vec<String> = lines[start..end]
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:>6}\t{}", start + i + 1, line))
            .collect();

        let output = if snippet.is_empty() {
            "(file is empty)".to_string()
        } else {
            format!(
                "File: {} (lines {}-{} of {})\n{}",
                file_path,
                start + 1,
                end,
                total,
                snippet.join("\n")
            )
        };

        ToolResult::ok(output)
    }
}
