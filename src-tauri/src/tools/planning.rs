use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub description: String,
    pub status: TodoStatus,
    pub priority: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
    Cancelled,
}

impl std::fmt::Display for TodoStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TodoStatus::Pending => write!(f, "[ ]"),
            TodoStatus::InProgress => write!(f, "[~]"),
            TodoStatus::Completed => write!(f, "[x]"),
            TodoStatus::Blocked => write!(f, "[!]"),
            TodoStatus::Cancelled => write!(f, "[-]"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoList {
    pub context: String,
    pub items: Vec<TodoItem>,
}

impl TodoList {
    pub fn new(context: String) -> Self {
        Self {
            context,
            items: Vec::new(),
        }
    }

    pub fn to_markdown(&self) -> String {
        let mut md = format!("## TODO: {}\n\n", self.context);

        for item in &self.items {
            md.push_str(&format!(
                "- {} [P{}] {}\n",
                item.status, item.priority, item.description
            ));
        }

        if self.items.is_empty() {
            md.push_str("(no items)\n");
        }

        md
    }
}

/// Shared state for the current TODO list
pub type SharedTodoList = Arc<RwLock<Option<TodoList>>>;

pub fn create_shared_todo_list() -> SharedTodoList {
    Arc::new(RwLock::new(None))
}

// ---------------------------------------------------------------------------
// Tool error
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct PlanningError(String);

// ---------------------------------------------------------------------------
// WriteTodos Tool
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct WriteTodosArgs {
    context: String,
    #[serde(default)]
    todos: Vec<NewTodoItem>,
    #[serde(default)]
    updates: Vec<TodoUpdate>,
}

#[derive(Debug, Deserialize)]
pub struct NewTodoItem {
    description: String,
    #[serde(default = "default_priority")]
    priority: u8,
}

fn default_priority() -> u8 {
    3
}

#[derive(Debug, Deserialize)]
pub struct TodoUpdate {
    id: String,
    status: TodoStatus,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct WriteTodosTool {
    #[serde(skip)]
    current_list: Option<SharedTodoList>,
}

impl WriteTodosTool {
    pub fn new(shared_list: SharedTodoList) -> Self {
        Self {
            current_list: Some(shared_list),
        }
    }
}

impl Tool for WriteTodosTool {
    const NAME: &'static str = "write_todos";
    type Error = PlanningError;
    type Args = WriteTodosArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "write_todos".to_string(),
            description: "Create or update a TODO list for tracking multi-step tasks. \
                Use this to break down complex tasks into steps, track progress, \
                and adapt your plan as new information emerges. \
                Each TODO has a description, priority (1=low, 5=critical), and status."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "context": {
                        "type": "string",
                        "description": "Brief description of the overall task you are working on"
                    },
                    "todos": {
                        "type": "array",
                        "description": "New TODO items to add",
                        "items": {
                            "type": "object",
                            "properties": {
                                "description": {
                                    "type": "string",
                                    "description": "What needs to be done"
                                },
                                "priority": {
                                    "type": "number",
                                    "description": "Priority 1-5 (1=low, 5=critical)",
                                    "minimum": 1,
                                    "maximum": 5
                                }
                            },
                            "required": ["description"]
                        }
                    },
                    "updates": {
                        "type": "array",
                        "description": "Status updates for existing TODO items",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": {
                                    "type": "string",
                                    "description": "ID of the TODO item to update"
                                },
                                "status": {
                                    "type": "string",
                                    "enum": ["pending", "in_progress", "completed", "blocked", "cancelled"],
                                    "description": "New status for the item"
                                }
                            },
                            "required": ["id", "status"]
                        }
                    }
                },
                "required": ["context"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let shared = self
            .current_list
            .as_ref()
            .ok_or_else(|| PlanningError("TODO list not initialized".to_string()))?;

        let mut list_guard = shared.write().await;

        let list = list_guard.get_or_insert_with(|| TodoList::new(args.context.clone()));

        // Update context if changed
        if list.context != args.context {
            list.context = args.context;
        }

        // Add new TODOs
        for todo in args.todos {
            let id = format!("todo-{}", list.items.len() + 1);
            list.items.push(TodoItem {
                id,
                description: todo.description,
                status: TodoStatus::Pending,
                priority: todo.priority.clamp(1, 5),
            });
        }

        // Process status updates
        for update in args.updates {
            if let Some(item) = list.items.iter_mut().find(|i| i.id == update.id) {
                item.status = update.status;
            } else {
                return Err(PlanningError(format!("TODO item not found: {}", update.id)));
            }
        }

        Ok(format!("TODO list updated:\n\n{}", list.to_markdown()))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_todo_list_markdown() {
        let mut list = TodoList::new("Test task".to_string());
        list.items.push(TodoItem {
            id: "todo-1".to_string(),
            description: "First step".to_string(),
            status: TodoStatus::Completed,
            priority: 5,
        });
        list.items.push(TodoItem {
            id: "todo-2".to_string(),
            description: "Second step".to_string(),
            status: TodoStatus::Pending,
            priority: 3,
        });

        let md = list.to_markdown();
        assert!(md.contains("[x]"));
        assert!(md.contains("[ ]"));
        assert!(md.contains("First step"));
        assert!(md.contains("Second step"));
    }

    #[test]
    fn test_todo_status_display() {
        assert_eq!(format!("{}", TodoStatus::Pending), "[ ]");
        assert_eq!(format!("{}", TodoStatus::Completed), "[x]");
        assert_eq!(format!("{}", TodoStatus::InProgress), "[~]");
        assert_eq!(format!("{}", TodoStatus::Blocked), "[!]");
        assert_eq!(format!("{}", TodoStatus::Cancelled), "[-]");
    }
}
