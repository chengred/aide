use async_trait::async_trait;
use serde_json::json;

use super::super::{Tool, ToolResult};

pub struct GlobTool;

impl GlobTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Find files matching a glob pattern. Returns file paths sorted by modification time."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The glob pattern to match files against (e.g. '**/*.rs', 'src/**/*.ts')"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. Defaults to current working directory."
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> ToolResult {
        let pattern_str = match params["pattern"].as_str() {
            Some(p) => p,
            None => return ToolResult::err("Missing required parameter: pattern"),
        };
        let base_path = params["path"].as_str().unwrap_or(".");

        let base = std::path::Path::new(base_path);
        if !base.is_dir() {
            return ToolResult::err(format!("Not a directory: {}", base_path));
        }

        // Build the full glob pattern
        let full_pattern = if base_path == "." {
            pattern_str.to_string()
        } else {
            format!("{}/{}", base_path.trim_end_matches('/'), pattern_str.trim_start_matches('/'))
        };

        let mut paths: Vec<String> = match glob::glob_with(
            &full_pattern,
            glob::MatchOptions {
                case_sensitive: true,
                require_literal_separator: false,
                require_literal_leading_dot: false,
            },
        ) {
            Ok(paths) => paths.filter_map(|p| p.ok().map(|p| p.display().to_string())).collect(),
            Err(e) => return ToolResult::err(format!("Invalid glob pattern: {}", e)),
        };

        if paths.is_empty() {
            return ToolResult::ok("No files found matching the pattern.");
        }

        // Sort by modification time (newest first)
        paths.sort_by(|a, b| {
            let ta = std::fs::metadata(a).and_then(|m| m.modified()).ok();
            let tb = std::fs::metadata(b).and_then(|m| m.modified()).ok();
            tb.cmp(&ta)
        });

        // Limit to 100 results
        paths.truncate(100);

        let output = paths.join("\n");
        ToolResult::ok(format!("Found {} files:\n{}", paths.len(), output))
    }
}
