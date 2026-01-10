//! TodoWrite tool for task list management
//!
//! Manages a structured task list for tracking progress during coding sessions.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::base::Tool;
use crate::mcp::registry::{ToolContext, ToolResult};

/// Todo item status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

impl TodoStatus {
    fn from_str(s: &str) -> Self {
        match s {
            "in_progress" => Self::InProgress,
            "completed" => Self::Completed,
            _ => Self::Pending,
        }
    }

    #[allow(dead_code)]
    fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }

    fn symbol(&self) -> &'static str {
        match self {
            Self::Pending => "○",
            Self::InProgress => "◐",
            Self::Completed => "●",
        }
    }
}

/// A single todo item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    /// The task description
    pub content: String,
    /// Current status
    pub status: String,
    /// Active form of the description (shown when in progress)
    #[serde(rename = "activeForm")]
    pub active_form: String,
}

/// Input parameters for TodoWrite
#[derive(Debug, Deserialize)]
struct TodoWriteInput {
    /// The updated todo list
    todos: Vec<TodoItem>,
}

/// Shared todo list state
#[derive(Debug, Default)]
pub struct TodoList {
    items: RwLock<Vec<TodoItem>>,
}

impl TodoList {
    pub fn new() -> Self {
        Self {
            items: RwLock::new(Vec::new()),
        }
    }

    pub async fn update(&self, items: Vec<TodoItem>) {
        let mut guard = self.items.write().await;
        *guard = items;
    }

    pub async fn get_all(&self) -> Vec<TodoItem> {
        self.items.read().await.clone()
    }

    pub async fn format(&self) -> String {
        let items = self.items.read().await;
        if items.is_empty() {
            return "No todos".to_string();
        }

        let mut output = String::new();
        for (i, item) in items.iter().enumerate() {
            let status = TodoStatus::from_str(&item.status);
            let display_text = if status == TodoStatus::InProgress {
                &item.active_form
            } else {
                &item.content
            };
            output.push_str(&format!(
                "{}. {} {}\n",
                i + 1,
                status.symbol(),
                display_text
            ));
        }
        output
    }
}

/// TodoWrite tool for task list management
#[derive(Debug)]
pub struct TodoWriteTool {
    /// Shared todo list
    todo_list: Arc<TodoList>,
}

impl Default for TodoWriteTool {
    fn default() -> Self {
        Self::new()
    }
}

impl TodoWriteTool {
    /// Create a new TodoWrite tool with its own list
    pub fn new() -> Self {
        Self {
            todo_list: Arc::new(TodoList::new()),
        }
    }

    /// Create a TodoWrite tool with a shared list
    pub fn with_shared_list(list: Arc<TodoList>) -> Self {
        Self { todo_list: list }
    }

    /// Get the shared todo list
    pub fn todo_list(&self) -> Arc<TodoList> {
        self.todo_list.clone()
    }
}

#[async_trait]
impl Tool for TodoWriteTool {
    fn name(&self) -> &str {
        "TodoWrite"
    }

    fn description(&self) -> &str {
        "Manages a structured task list for tracking progress. Use this to plan tasks, \
         track progress, and demonstrate thoroughness. Each todo has content, status \
         (pending/in_progress/completed), and activeForm (shown when in progress)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["todos"],
            "properties": {
                "todos": {
                    "type": "array",
                    "description": "The updated todo list",
                    "items": {
                        "type": "object",
                        "required": ["content", "status", "activeForm"],
                        "properties": {
                            "content": {
                                "type": "string",
                                "minLength": 1,
                                "description": "The task description (imperative form)"
                            },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed"],
                                "description": "Current status of the task"
                            },
                            "activeForm": {
                                "type": "string",
                                "minLength": 1,
                                "description": "Present continuous form shown during execution"
                            }
                        }
                    }
                }
            }
        })
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> ToolResult {
        // Parse input
        let params: TodoWriteInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid input: {}", e)),
        };

        // Validate todos
        for (i, todo) in params.todos.iter().enumerate() {
            if todo.content.trim().is_empty() {
                return ToolResult::error(format!("Todo {} has empty content", i + 1));
            }
            if todo.active_form.trim().is_empty() {
                return ToolResult::error(format!("Todo {} has empty activeForm", i + 1));
            }
            // Validate status
            let valid_statuses = ["pending", "in_progress", "completed"];
            if !valid_statuses.contains(&todo.status.as_str()) {
                return ToolResult::error(format!(
                    "Todo {} has invalid status '{}'. Must be one of: {:?}",
                    i + 1,
                    todo.status,
                    valid_statuses
                ));
            }
        }

        // Count statuses
        let pending = params
            .todos
            .iter()
            .filter(|t| t.status == "pending")
            .count();
        let in_progress = params
            .todos
            .iter()
            .filter(|t| t.status == "in_progress")
            .count();
        let completed = params
            .todos
            .iter()
            .filter(|t| t.status == "completed")
            .count();

        // Update the todo list
        self.todo_list.update(params.todos.clone()).await;

        // Format output
        let formatted = self.todo_list.format().await;

        let output = format!(
            "Todos updated successfully.\n\n{}\n\nSummary: {} pending, {} in progress, {} completed",
            formatted, pending, in_progress, completed
        );

        ToolResult::success(output).with_metadata(json!({
            "total": params.todos.len(),
            "pending": pending,
            "in_progress": in_progress,
            "completed": completed
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_todo_write_tool_properties() {
        let tool = TodoWriteTool::new();
        assert_eq!(tool.name(), "TodoWrite");
        assert!(tool.description().contains("task"));
    }

    #[test]
    fn test_todo_write_input_schema() {
        let tool = TodoWriteTool::new();
        let schema = tool.input_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["todos"].is_object());
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!("todos"))
        );
    }

    #[tokio::test]
    async fn test_todo_write_create_list() {
        let temp_dir = TempDir::new().unwrap();
        let tool = TodoWriteTool::new();
        let context = ToolContext::new("test", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "todos": [
                        {
                            "content": "Implement feature",
                            "status": "in_progress",
                            "activeForm": "Implementing feature"
                        },
                        {
                            "content": "Write tests",
                            "status": "pending",
                            "activeForm": "Writing tests"
                        }
                    ]
                }),
                &context,
            )
            .await;

        assert!(!result.is_error);
        assert!(result.content.contains("Todos updated"));
        assert!(result.content.contains("1 pending"));
        assert!(result.content.contains("1 in progress"));
    }

    #[tokio::test]
    async fn test_todo_write_update_status() {
        let temp_dir = TempDir::new().unwrap();
        let tool = TodoWriteTool::new();
        let context = ToolContext::new("test", temp_dir.path());

        // First create
        tool.execute(
            json!({
                "todos": [
                    {
                        "content": "Task 1",
                        "status": "pending",
                        "activeForm": "Doing task 1"
                    }
                ]
            }),
            &context,
        )
        .await;

        // Then update to completed
        let result = tool
            .execute(
                json!({
                    "todos": [
                        {
                            "content": "Task 1",
                            "status": "completed",
                            "activeForm": "Doing task 1"
                        }
                    ]
                }),
                &context,
            )
            .await;

        assert!(!result.is_error);
        assert!(result.content.contains("1 completed"));
    }

    #[tokio::test]
    async fn test_todo_write_invalid_status() {
        let temp_dir = TempDir::new().unwrap();
        let tool = TodoWriteTool::new();
        let context = ToolContext::new("test", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "todos": [
                        {
                            "content": "Task",
                            "status": "invalid_status",
                            "activeForm": "Doing task"
                        }
                    ]
                }),
                &context,
            )
            .await;

        assert!(result.is_error);
        assert!(result.content.contains("invalid status"));
    }

    #[tokio::test]
    async fn test_todo_write_empty_content() {
        let temp_dir = TempDir::new().unwrap();
        let tool = TodoWriteTool::new();
        let context = ToolContext::new("test", temp_dir.path());

        let result = tool
            .execute(
                json!({
                    "todos": [
                        {
                            "content": "",
                            "status": "pending",
                            "activeForm": "Doing task"
                        }
                    ]
                }),
                &context,
            )
            .await;

        assert!(result.is_error);
        assert!(result.content.contains("empty content"));
    }

    #[test]
    fn test_todo_status() {
        assert_eq!(TodoStatus::from_str("pending"), TodoStatus::Pending);
        assert_eq!(TodoStatus::from_str("in_progress"), TodoStatus::InProgress);
        assert_eq!(TodoStatus::from_str("completed"), TodoStatus::Completed);
        assert_eq!(TodoStatus::from_str("unknown"), TodoStatus::Pending);

        assert_eq!(TodoStatus::Pending.as_str(), "pending");
        assert_eq!(TodoStatus::InProgress.symbol(), "◐");
    }

    #[tokio::test]
    async fn test_shared_todo_list() {
        let shared_list = Arc::new(TodoList::new());
        let tool1 = TodoWriteTool::with_shared_list(shared_list.clone());
        let tool2 = TodoWriteTool::with_shared_list(shared_list.clone());

        let temp_dir = TempDir::new().unwrap();
        let context = ToolContext::new("test", temp_dir.path());

        // Update via tool1
        tool1
            .execute(
                json!({
                    "todos": [
                        {
                            "content": "Shared task",
                            "status": "pending",
                            "activeForm": "Doing shared task"
                        }
                    ]
                }),
                &context,
            )
            .await;

        // Verify via tool2's shared list
        let items = tool2.todo_list().get_all().await;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].content, "Shared task");
    }
}
