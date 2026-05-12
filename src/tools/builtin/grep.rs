use async_trait::async_trait;
use serde_json::json;

use super::super::{Tool, ToolResult};

pub struct GrepTool;

impl GrepTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search for a regex pattern in files. Returns matching lines with file paths and line numbers."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The regular expression pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search in. Defaults to current directory."
                },
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g. '*.rs', '**/*.toml')"
                },
                "output_mode": {
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count"],
                    "description": "Output mode: content shows matching lines, files_with_matches shows file paths, count shows match counts"
                },
                "case_insensitive": {
                    "type": "boolean",
                    "description": "If true, perform case-insensitive search"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return. Default: 250"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> ToolResult {
        let pattern = match params["pattern"].as_str() {
            Some(p) => p,
            None => return ToolResult::err("Missing required parameter: pattern"),
        };

        let search_path = params["path"].as_str().unwrap_or(".");
        let glob_filter = params["glob"].as_str();
        let output_mode = params["output_mode"].as_str().unwrap_or("content");
        let case_insensitive = params["case_insensitive"].as_bool().unwrap_or(false);
        let max_results = params["max_results"].as_u64().unwrap_or(250) as usize;

        let mut builder = regex::RegexBuilder::new(pattern);
        builder.multi_line(true);
        if case_insensitive {
            builder.case_insensitive(true);
        }
        let re = match builder.build() {
            Ok(r) => r,
            Err(e) => return ToolResult::err(format!("Invalid regex pattern: {}", e)),
        };

        let path = std::path::Path::new(search_path);
        let mut results: Vec<String> = Vec::new();
        let mut total_matches = 0;

        if path.is_file() {
            search_file(path, &re, output_mode, &mut results, &mut total_matches, max_results);
        } else if path.is_dir() {
            search_dir(path, &re, glob_filter, output_mode, &mut results, &mut total_matches, max_results);
        } else {
            return ToolResult::err(format!("Path not found: {}", search_path));
        }

        if results.is_empty() {
            return ToolResult::ok(format!("No matches found for pattern: {}", pattern));
        }

        let summary = format!("Found {} matches. ", total_matches);
        ToolResult::ok(summary + &results.join("\n"))
    }
}

fn search_file(
    path: &std::path::Path,
    re: &regex::Regex,
    output_mode: &str,
    results: &mut Vec<String>,
    total: &mut usize,
    max: usize,
) {
    if results.len() >= max {
        return;
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let path_str = path.display().to_string();
    match output_mode {
        "files_with_matches" => {
            if re.is_match(&content) {
                results.push(path_str);
                *total += 1;
            }
        }
        "count" => {
            let count = re.find_iter(&content).count();
            if count > 0 {
                results.push(format!("{}: {}", path_str, count));
                *total += count;
            }
        }
        _ => {
            for (line_num, line) in content.lines().enumerate() {
                if results.len() >= max {
                    break;
                }
                if re.is_match(line) {
                    results.push(format!("{}:{}: {}", path_str, line_num + 1, line));
                    *total += 1;
                }
            }
        }
    }
}

fn search_dir(
    dir: &std::path::Path,
    re: &regex::Regex,
    glob_filter: Option<&str>,
    output_mode: &str,
    results: &mut Vec<String>,
    total: &mut usize,
    max: usize,
) {
    if results.len() >= max {
        return;
    }
    let glob_pattern = glob_filter
        .map(|g| glob::Pattern::new(g))
        .transpose()
        .ok()
        .flatten();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if results.len() >= max {
                break;
            }
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with('.') || name == "target" || name == "node_modules" {
                        continue;
                    }
                }
                search_dir(&path, re, glob_filter, output_mode, results, total, max);
            } else if path.is_file() {
                if let Some(ref pat) = glob_pattern {
                    let rel = path.strip_prefix(dir).unwrap_or(&path);
                    if !pat.matches_path(rel) {
                        continue;
                    }
                }
                search_file(&path, re, output_mode, results, total, max);
            }
        }
    }
}
