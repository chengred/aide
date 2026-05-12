use async_trait::async_trait;
use serde_json::json;

use super::super::{Tool, ToolResult};

pub struct WriteTool;

impl WriteTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }

    fn description(&self) -> &str {
        "Write content to a file, creating it if it doesn't exist or overwriting if it does."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "The content to write to the file"
                }
            },
            "required": ["file_path", "content"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> ToolResult {
        let file_path = match params["file_path"].as_str() {
            Some(p) => p,
            None => return ToolResult::err("Missing required parameter: file_path"),
        };
        let content = match params["content"].as_str() {
            Some(c) => c,
            None => return ToolResult::err("Missing required parameter: content"),
        };

        let path = std::path::Path::new(file_path);
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return ToolResult::err(format!("Failed to create parent directories: {}", e));
            }
        }

        match std::fs::write(path, content) {
            Ok(_) => ToolResult::ok(format!(
                "Successfully wrote {} bytes to {}",
                content.len(),
                file_path
            )),
            Err(e) => ToolResult::err(format!("Failed to write file: {}", e)),
        }
    }

    fn requires_approval(&self) -> bool {
        true
    }
}
