use async_trait::async_trait;
use serde_json::json;

use super::{Tool, ToolResult};

/// The PlanTool is a "no-op" context engineering tool.
/// It doesn't execute anything - it exists solely to let the model
/// output structured plans, which helps the agent maintain direction
/// during complex, long-running tasks.
pub struct PlanTool;

impl PlanTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for PlanTool {
    fn name(&self) -> &str {
        "plan"
    }

    fn description(&self) -> &str {
        "Create a structured plan for a complex task. This is a context engineering tool \
         that helps the agent organize its approach before executing individual steps. \
         Each step should be concrete and actionable."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "A brief title for the overall plan"
                },
                "steps": {
                    "type": "array",
                    "description": "Ordered list of steps to execute",
                    "items": {
                        "type": "object",
                        "properties": {
                            "step": {
                                "type": "integer",
                                "description": "Step number"
                            },
                            "action": {
                                "type": "string",
                                "description": "What to do in this step"
                            },
                            "tool": {
                                "type": "string",
                                "description": "The tool to use for this step"
                            },
                            "expected_outcome": {
                                "type": "string",
                                "description": "What should happen after this step"
                            }
                        },
                        "required": ["step", "action"]
                    }
                },
                "dependencies": {
                    "type": "array",
                    "description": "Any dependencies between steps",
                    "items": {
                        "type": "object",
                        "properties": {
                            "step": { "type": "integer" },
                            "depends_on": { "type": "integer" },
                            "reason": { "type": "string" }
                        }
                    }
                }
            },
            "required": ["title", "steps"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> ToolResult {
        let title = params["title"].as_str().unwrap_or("Untitled Plan");
        let steps = params["steps"].as_array();

        let mut output = format!("Plan recorded: {}\n", title);
        output.push_str("═══════════════════════════════\n");

        if let Some(steps) = steps {
            for step in steps {
                let num = step["step"].as_u64().map(|n| n.to_string()).unwrap_or_else(|| "?".into());
                let action = step["action"].as_str().unwrap_or("(no action)");
                let tool = step["tool"].as_str().unwrap_or("manual");
                let expected = step["expected_outcome"].as_str().unwrap_or("-");

                output.push_str(&format!(
                    "  Step {}: {}\n    Tool: {}\n    Expected: {}\n\n",
                    num, action, tool, expected
                ));
            }
        }

        output.push_str("The agent will now proceed to execute this plan step by step.");

        ToolResult::ok(output)
    }

    fn requires_approval(&self) -> bool {
        false
    }
}
