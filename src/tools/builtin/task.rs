use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use serde_json::json;

use super::super::{Tool, ToolResult};

/// A single task in the task list
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Task {
    pub id: String,
    pub subject: String,
    pub description: String,
    pub status: TaskStatus,
    pub blocks: Vec<String>,
    pub blocked_by: Vec<String>,
    pub owner: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    #[serde(rename = "in_progress")]
    InProgress,
    Completed,
    Deleted,
}

/// Shared task store accessible by all task tools
pub type TaskStore = Arc<RwLock<Vec<Task>>>;

pub fn new_task_store() -> TaskStore {
    Arc::new(RwLock::new(Vec::new()))
}

// ── TaskCreate ──

pub struct TaskCreateTool {
    store: TaskStore,
}

impl TaskCreateTool {
    pub fn new(store: TaskStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for TaskCreateTool {
    fn name(&self) -> &str {
        "task_create"
    }

    fn description(&self) -> &str {
        "Create a new task for tracking work progress. Use this to break complex tasks into manageable steps."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "subject": {
                    "type": "string",
                    "description": "Brief title for the task in imperative form (e.g., 'Fix authentication bug')"
                },
                "description": {
                    "type": "string",
                    "description": "What needs to be done"
                },
                "depends_on": {
                    "type": "array",
                    "description": "Task IDs that must complete before this one",
                    "items": { "type": "string" }
                }
            },
            "required": ["subject", "description"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> ToolResult {
        let subject = params["subject"].as_str().unwrap_or("Untitled");
        let description = params["description"].as_str().unwrap_or("");
        let depends_on: Vec<String> = params["depends_on"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let id = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let task = Task {
            id: id.clone(),
            subject: subject.to_string(),
            description: description.to_string(),
            status: TaskStatus::Pending,
            blocks: Vec::new(),
            blocked_by: depends_on,
            owner: None,
        };

        let mut store = self.store.write().unwrap();
        store.push(task);

        ToolResult::ok(format!("Task created: [{}] {}", id, subject))
    }

    fn requires_approval(&self) -> bool {
        false
    }
}

// ── TaskUpdate ──

pub struct TaskUpdateTool {
    store: TaskStore,
}

impl TaskUpdateTool {
    pub fn new(store: TaskStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for TaskUpdateTool {
    fn name(&self) -> &str {
        "task_update"
    }

    fn description(&self) -> &str {
        "Update a task's status or details. Use 'in_progress' when starting work, 'completed' when done."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The task ID to update"
                },
                "status": {
                    "type": "string",
                    "enum": ["pending", "in_progress", "completed", "deleted"],
                    "description": "New status for the task"
                },
                "subject": {
                    "type": "string",
                    "description": "New subject (optional)"
                },
                "add_blocks": {
                    "type": "array",
                    "description": "Task IDs that this task blocks",
                    "items": { "type": "string" }
                }
            },
            "required": ["task_id", "status"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> ToolResult {
        let task_id = params["task_id"].as_str().unwrap_or("");
        let status_str = params["status"].as_str().unwrap_or("pending");
        let new_subject = params["subject"].as_str();
        let add_blocks: Vec<String> = params["add_blocks"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let status = match status_str {
            "pending" => TaskStatus::Pending,
            "in_progress" => TaskStatus::InProgress,
            "completed" => TaskStatus::Completed,
            "deleted" => TaskStatus::Deleted,
            _ => return ToolResult::err(format!("Unknown status: {}", status_str)),
        };

        let mut store = self.store.write().unwrap();

        // Find by index to avoid borrow issues
        let task_idx = store.iter().position(|t| t.id == task_id);
        let task_idx = match task_idx {
            Some(i) => i,
            None => return ToolResult::err(format!("Task not found: {}", task_id)),
        };

        let old_status = store[task_idx].status.clone();
        store[task_idx].status = status.clone();

        if let Some(subj) = new_subject {
            store[task_idx].subject = subj.to_string();
        }

        // Add block relationships
        for blocked_id in &add_blocks {
            if !store[task_idx].blocks.contains(blocked_id) {
                store[task_idx].blocks.push(blocked_id.clone());
            }
        }

        // Update blocked_by on dependent tasks
        for blocked_id in &add_blocks {
            if let Some(blocked_idx) = store.iter().position(|t| t.id == *blocked_id) {
                if !store[blocked_idx].blocked_by.contains(&task_id.to_string()) {
                    store[blocked_idx].blocked_by.push(task_id.to_string());
                }
            }
        }

        let verb = match status {
            TaskStatus::InProgress => "Started",
            TaskStatus::Completed => "Completed",
            TaskStatus::Deleted => "Deleted",
            _ => "Updated",
        };

        ToolResult::ok(format!("{} task [{}] {} (was {:?})", verb, task_id, store[task_idx].subject, old_status))
    }

    fn requires_approval(&self) -> bool {
        false
    }
}

// ── TaskList ──

pub struct TaskListTool {
    store: TaskStore,
}

impl TaskListTool {
    pub fn new(store: TaskStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for TaskListTool {
    fn name(&self) -> &str {
        "task_list"
    }

    fn description(&self) -> &str {
        "List all tasks and their current status. Use this to check progress and find the next task to work on."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "status_filter": {
                    "type": "string",
                    "enum": ["pending", "in_progress", "completed", "all"],
                    "description": "Filter tasks by status"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value) -> ToolResult {
        let filter = params["status_filter"].as_str().unwrap_or("all");
        let store = self.store.read().unwrap();

        let filtered: Vec<&Task> = match filter {
            "pending" => store.iter().filter(|t| t.status == TaskStatus::Pending).collect(),
            "in_progress" => store.iter().filter(|t| t.status == TaskStatus::InProgress).collect(),
            "completed" => store.iter().filter(|t| t.status == TaskStatus::Completed).collect(),
            _ => store.iter().filter(|t| t.status != TaskStatus::Deleted).collect(),
        };

        if filtered.is_empty() {
            return ToolResult::ok("No tasks found.".to_string());
        }

        let mut output = format!("Tasks ({}) :\n", filtered.len());
        output.push_str("──────┬──────────┬──────────────────────────\n");
        output.push_str(" ID   │ Status   │ Subject\n");
        output.push_str("──────┼──────────┼──────────────────────────\n");

        for task in &filtered {
            let status_icon = match task.status {
                TaskStatus::Pending => "⬜",
                TaskStatus::InProgress => "🔄",
                TaskStatus::Completed => "✅",
                TaskStatus::Deleted => "❌",
            };
            let blocked = if !task.blocked_by.is_empty() {
                format!(" [blocked by: {}]", task.blocked_by.join(", "))
            } else {
                String::new()
            };
            output.push_str(&format!(
                " {} │ {} {} │ {}{}\n",
                task.id,
                status_icon,
                format!("{:?}", task.status).to_lowercase().replace('"', ""),
                task.subject,
                blocked
            ));
        }

        ToolResult::ok(output)
    }

    fn requires_approval(&self) -> bool {
        false
    }
}

// ── TaskGet ──

pub struct TaskGetTool {
    store: TaskStore,
}

impl TaskGetTool {
    pub fn new(store: TaskStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for TaskGetTool {
    fn name(&self) -> &str {
        "task_get"
    }

    fn description(&self) -> &str {
        "Get full details about a specific task including its description and dependencies."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The task ID to retrieve"
                }
            },
            "required": ["task_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> ToolResult {
        let task_id = params["task_id"].as_str().unwrap_or("");
        let store = self.store.read().unwrap();

        let task = match store.iter().find(|t| t.id == task_id) {
            Some(t) => t,
            None => return ToolResult::err(format!("Task not found: {}", task_id)),
        };

        let mut output = format!("Task: [{}] {}\n", task.id, task.subject);
        output.push_str(&format!("Status: {:?}\n", task.status));
        output.push_str(&format!("Description: {}\n", task.description));
        if !task.blocked_by.is_empty() {
            output.push_str(&format!("Blocked by: {}\n", task.blocked_by.join(", ")));
        }
        if !task.blocks.is_empty() {
            output.push_str(&format!("Blocks: {}\n", task.blocks.join(", ")));
        }
        output.push_str(&format!("Owner: {}\n", task.owner.as_deref().unwrap_or("unassigned")));

        ToolResult::ok(output)
    }

    fn requires_approval(&self) -> bool {
        false
    }
}

/// Register all task tools with the given registry
pub fn register_all(registry: &mut super::super::ToolRegistry) {
    let store = new_task_store();
    registry.register(TaskCreateTool::new(store.clone()));
    registry.register(TaskUpdateTool::new(store.clone()));
    registry.register(TaskListTool::new(store.clone()));
    registry.register(TaskGetTool::new(store));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Runtime::new().unwrap().block_on(f)
    }

    fn make_store() -> TaskStore {
        let store = new_task_store();
        store.write().unwrap().push(Task {
            id: "test-1".into(),
            subject: "Test task".into(),
            description: "A test task".into(),
            status: TaskStatus::Pending,
            blocks: vec![],
            blocked_by: vec![],
            owner: None,
        });
        store
    }

    #[test]
    fn test_create_task() {
        let store = new_task_store();
        let tool = TaskCreateTool::new(store.clone());
        let params = json!({"subject": "Fix bug", "description": "Fix the login bug"});
        let result = block_on(tool.execute(params));
        assert!(result.success);
        assert!(result.content.contains("Fix bug"));
        assert_eq!(store.read().unwrap().len(), 1);
    }

    #[test]
    fn test_update_task_status() {
        let store = make_store();
        let tool = TaskUpdateTool::new(store.clone());
        let params = json!({"task_id": "test-1", "status": "in_progress"});
        let result = block_on(tool.execute(params));
        assert!(result.success);
        assert_eq!(store.read().unwrap()[0].status, TaskStatus::InProgress);
    }

    #[test]
    fn test_list_tasks() {
        let store = make_store();
        let tool = TaskListTool::new(store.clone());
        let params = json!({"status_filter": "pending"});
        let result = block_on(tool.execute(params));
        assert!(result.success);
        assert!(result.content.contains("Test task"));
    }

    #[test]
    fn test_get_task() {
        let store = make_store();
        let tool = TaskGetTool::new(store.clone());
        let params = json!({"task_id": "test-1"});
        let result = block_on(tool.execute(params));
        assert!(result.success);
        assert!(result.content.contains("Test task"));
        assert!(result.content.contains("A test task"));
    }

    #[test]
    fn test_get_nonexistent_task() {
        let store = make_store();
        let tool = TaskGetTool::new(store);
        let params = json!({"task_id": "nonexistent"});
        let result = block_on(tool.execute(params));
        assert!(!result.success);
    }
}
