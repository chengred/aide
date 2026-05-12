#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// A single step in an execution plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub step: u32,
    pub action: String,
    pub tool: Option<String>,
    pub expected_outcome: Option<String>,
    pub status: StepStatus,
    pub dependencies: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StepStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

/// A structured execution plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub title: String,
    pub steps: Vec<PlanStep>,
}

impl ExecutionPlan {
    pub fn new(title: impl Into<String>, steps: Vec<PlanStep>) -> Self {
        Self {
            title: title.into(),
            steps,
        }
    }

    /// Get the next pending step
    pub fn next_pending(&self) -> Option<&PlanStep> {
        self.steps.iter().find(|s| s.status == StepStatus::Pending)
    }

    /// Mark a step as completed
    pub fn mark_completed(&mut self, step_num: u32) {
        if let Some(step) = self.steps.iter_mut().find(|s| s.step == step_num) {
            step.status = StepStatus::Completed;
        }
    }

    /// Check if all steps are done
    pub fn is_complete(&self) -> bool {
        self.steps
            .iter()
            .all(|s| matches!(s.status, StepStatus::Completed | StepStatus::Skipped))
    }

    /// Format the plan as a readable string
    pub fn format(&self) -> String {
        let mut out = format!("Plan: {}\n", self.title);
        out.push_str("═══════════════════════════════\n");
        for step in &self.steps {
            let status_icon = match step.status {
                StepStatus::Pending => "[ ]",
                StepStatus::InProgress => "[~]",
                StepStatus::Completed => "[✓]",
                StepStatus::Failed => "[✗]",
                StepStatus::Skipped => "[-]",
            };
            out.push_str(&format!(
                "  {} Step {}: {}\n",
                status_icon, step.step, step.action
            ));
        }
        out
    }
}

/// The Planner is responsible for generating execution plans
/// from complex user requests. In Phase 2, this is a simple
/// template-based planner. Later phases can add LLM-based planning.
pub struct Planner;

impl Planner {
    pub fn new() -> Self {
        Self
    }

    /// Generate a plan from a task description
    /// This is a simple implementation — in later phases,
    /// the LLM itself generates plans via the PlanTool.
    pub fn generate_plan(&self, _task: &str) -> ExecutionPlan {
        ExecutionPlan::new(
            "Task Execution",
            vec![PlanStep {
                step: 1,
                action: "Analyze the request and identify required actions".into(),
                tool: None,
                expected_outcome: Some("Clear understanding of the task".into()),
                status: StepStatus::Pending,
                dependencies: vec![],
            }],
        )
    }

    /// Parse a plan from a PlanTool invocation's JSON arguments
    pub fn parse_from_tool_result(&self, args: &serde_json::Value) -> Option<ExecutionPlan> {
        let title = args.get("title")?.as_str()?.to_string();
        let steps_json = args.get("steps")?.as_array()?;

        let steps: Vec<PlanStep> = steps_json
            .iter()
            .enumerate()
            .map(|(i, step)| PlanStep {
                step: step["step"].as_u64().map(|n| n as u32).unwrap_or((i + 1) as u32),
                action: step["action"].as_str().unwrap_or("(no action)").to_string(),
                tool: step["tool"].as_str().map(String::from),
                expected_outcome: step["expected_outcome"].as_str().map(String::from),
                status: StepStatus::Pending,
                dependencies: Vec::new(),
            })
            .collect();

        Some(ExecutionPlan::new(title, steps))
    }

    /// Format the current plan status for inclusion in the system prompt
    pub fn format_plan_context(plan: &ExecutionPlan) -> String {
        let mut ctx = String::from("\n\n## Current Execution Plan\n\n");
        ctx.push_str(&format!("**Plan**: {}\n\n", plan.title));
        ctx.push_str("| Step | Status | Action |\n");
        ctx.push_str("|------|--------|--------|\n");
        for step in &plan.steps {
            let status = match step.status {
                StepStatus::Pending => "⬜",
                StepStatus::InProgress => "🔄",
                StepStatus::Completed => "✅",
                StepStatus::Failed => "❌",
                StepStatus::Skipped => "⏭️",
            };
            ctx.push_str(&format!(
                "| {} | {} | {} |\n",
                step.step, status, step.action
            ));
        }
        ctx.push_str("\nProceed through the plan one step at a time. Mark each step complete after it's done.\n");
        ctx
    }
}

impl Default for Planner {
    fn default() -> Self {
        Self::new()
    }
}
