use async_trait::async_trait;
use serde_json::json;
use std::process::Command;

use super::super::{Tool, ToolResult};

pub struct BashTool;

impl BashTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute a shell command and return its output. Use with caution."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in milliseconds (max 600000). Default: 120000."
                },
                "working_dir": {
                    "type": "string",
                    "description": "Working directory for the command"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> ToolResult {
        let cmd_str = match params["command"].as_str() {
            Some(c) => c,
            None => return ToolResult::err("Missing required parameter: command"),
        };

        let timeout_ms = params["timeout"].as_u64().unwrap_or(120_000);
        let timeout = std::time::Duration::from_millis(timeout_ms.min(600_000));

        let mut cmd = if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.args(["/C", cmd_str]);
            c
        } else {
            let mut c = Command::new("bash");
            c.args(["-c", cmd_str]);
            c
        };

        if let Some(dir) = params["working_dir"].as_str() {
            cmd.current_dir(dir);
        }

        // Use tokio's spawn_blocking with timeout
        let result = tokio::time::timeout(timeout, tokio::task::spawn_blocking(move || {
            match cmd.output() {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                    let mut result = String::new();
                    if !stdout.is_empty() {
                        result.push_str(&stdout);
                    }
                    if !stderr.is_empty() {
                        if !result.is_empty() {
                            result.push('\n');
                        }
                        result.push_str("[stderr]\n");
                        result.push_str(&stderr);
                    }
                    if result.is_empty() {
                        result = format!(
                            "Command completed with exit code: {}",
                            output.status.code().unwrap_or(-1)
                        );
                    }
                    ToolResult::ok(result)
                }
                Err(e) => ToolResult::err(format!("Failed to execute command: {}", e)),
            }
        }))
        .await;

        match result {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => ToolResult::err(format!("Task join error: {}", e)),
            Err(_) => ToolResult::err(format!("Command timed out after {}ms", timeout_ms)),
        }
    }

    fn requires_approval(&self) -> bool {
        true
    }
}
