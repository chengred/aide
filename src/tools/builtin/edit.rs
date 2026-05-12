use async_trait::async_trait;
use serde_json::json;

use super::super::{Tool, ToolResult};

pub struct EditTool;

impl EditTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        "Perform an exact string replacement in a file. Replaces the first occurrence of old_string with new_string."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to edit"
                },
                "old_string": {
                    "type": "string",
                    "description": "The exact text to find and replace"
                },
                "new_string": {
                    "type": "string",
                    "description": "The text to replace it with (must be different from old_string)"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "If true, replace all occurrences. Default: false (replace only first)"
                }
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> ToolResult {
        let file_path = match params["file_path"].as_str() {
            Some(p) => p,
            None => return ToolResult::err("Missing required parameter: file_path"),
        };
        let old_string = match params["old_string"].as_str() {
            Some(s) => s,
            None => return ToolResult::err("Missing required parameter: old_string"),
        };
        let new_string = match params["new_string"].as_str() {
            Some(s) => s,
            None => return ToolResult::err("Missing required parameter: new_string"),
        };
        let replace_all = params["replace_all"].as_bool().unwrap_or(false);

        if old_string == new_string {
            return ToolResult::err("old_string and new_string must be different");
        }

        let path = std::path::Path::new(file_path);
        if !path.exists() {
            return ToolResult::err(format!("File not found: {}", file_path));
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => return ToolResult::err(format!("Failed to read file: {}", e)),
        };

        let count = content.matches(old_string).count();
        if count == 0 {
            return ToolResult::err(format!(
                "old_string not found in {}. The file content may have changed.",
                file_path
            ));
        }

        let new_content = if replace_all {
            content.replace(old_string, new_string)
        } else {
            if count > 1 {
                return ToolResult::err(format!(
                    "Found {} occurrences of old_string in {}. Use replace_all: true to replace all, or provide more context to make old_string unique.",
                    count, file_path
                ));
            }
            content.replacen(old_string, new_string, 1)
        };

        match std::fs::write(path, &new_content) {
            Ok(_) => ToolResult::ok(format!(
                "Successfully edited {}. Replaced {} occurrence(s).",
                file_path,
                if replace_all { count } else { 1 }
            )),
            Err(e) => ToolResult::err(format!("Failed to write file: {}", e)),
        }
    }

    fn requires_approval(&self) -> bool {
        true
    }
}
