//! rig Tools for the agent to manage scheduled tasks.
//!
//! Provides three tools:
//! - `CreateScheduledTaskTool` -- create a new recurring task
//! - `ListScheduledTasksTool` -- list all scheduled tasks
//! - `DeleteScheduledTaskTool` -- delete a scheduled task

use chrono::Utc;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Pool, Sqlite};
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::Mutex;

use crate::ai_instances::AIInstanceManager;

use super::runner::register_task_job;
use super::storage;
use super::{validate_cron_expression, ScheduledTask, SharedScheduler};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct SchedulerToolError(String);

// ---------------------------------------------------------------------------
// CreateScheduledTaskTool
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateScheduledTaskArgs {
    /// A short, descriptive name for the task (e.g. "morning-reminder", "weekly-report").
    name: String,
    /// Cron expression defining when the task runs (e.g. "0 8 * * *" for every day at 8:00).
    cron_expression: String,
    /// The prompt that will be sent to a temporary agent each time the task fires.
    task_prompt: String,
    /// Whether to send OS notifications and show results in the chat when the task completes.
    /// Defaults to true. Set to false for silent background tasks.
    #[serde(default = "default_notify")]
    notify: bool,
}

fn default_notify() -> bool {
    true
}

/// rig Tool that allows the agent to create scheduled tasks.
#[derive(Clone, Serialize, Deserialize)]
pub struct CreateScheduledTaskTool {
    #[serde(skip)]
    db: Option<Pool<Sqlite>>,
    #[serde(skip, default)]
    instance_id: String,
    #[serde(skip)]
    scheduler: Option<SharedScheduler>,
    #[serde(skip)]
    manager: Option<Arc<Mutex<AIInstanceManager>>>,
    #[serde(skip)]
    app_handle: Option<AppHandle>,
}

impl CreateScheduledTaskTool {
    pub fn new(
        db: Pool<Sqlite>,
        instance_id: String,
        scheduler: SharedScheduler,
        manager: Arc<Mutex<AIInstanceManager>>,
        app_handle: Option<AppHandle>,
    ) -> Self {
        Self {
            db: Some(db),
            instance_id,
            scheduler: Some(scheduler),
            manager: Some(manager),
            app_handle,
        }
    }
}

impl Tool for CreateScheduledTaskTool {
    const NAME: &'static str = "create_scheduled_task";
    type Error = SchedulerToolError;
    type Args = CreateScheduledTaskArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "create_scheduled_task".to_string(),
            description: "Create a recurring scheduled task. The task will run automatically \
                according to the cron expression. Each time it fires, a temporary agent \
                executes the task prompt with access to all tools.\n\n\
                Common cron patterns:\n\
                - \"0 8 * * *\" -- every day at 8:00\n\
                - \"0 9 * * 1\" -- every Monday at 9:00\n\
                - \"*/30 * * * *\" -- every 30 minutes\n\
                - \"0 0 1 * *\" -- first day of every month at midnight\n\
                - \"0 18 * * 1-5\" -- weekdays at 18:00"
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "A short name for the task (e.g. 'morning-reminder', 'weekly-report')"
                    },
                    "cron_expression": {
                        "type": "string",
                        "description": "Cron expression (5 or 6 fields). Examples: '0 8 * * *' (daily at 8:00), '*/30 * * * *' (every 30 min)"
                    },
                    "task_prompt": {
                        "type": "string",
                        "description": "The prompt that a temporary agent will execute each time the task fires"
                    },
                    "notify": {
                        "type": "boolean",
                        "description": "Whether to send OS notifications and show results in the chat when the task completes. Defaults to true. Set to false for silent background tasks."
                    }
                },
                "required": ["name", "cron_expression", "task_prompt"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| SchedulerToolError("Database not initialized".to_string()))?;
        let scheduler = self
            .scheduler
            .as_ref()
            .ok_or_else(|| SchedulerToolError("Scheduler not initialized".to_string()))?;
        let manager = self
            .manager
            .as_ref()
            .ok_or_else(|| SchedulerToolError("Manager not initialized".to_string()))?;

        // Validate cron expression
        validate_cron_expression(&args.cron_expression).map_err(SchedulerToolError)?;

        // Create task
        let task = ScheduledTask {
            id: uuid::Uuid::new_v4().to_string(),
            instance_id: self.instance_id.clone(),
            name: args.name.clone(),
            cron_expression: args.cron_expression.clone(),
            task_prompt: args.task_prompt.clone(),
            enabled: true,
            notify: args.notify,
            last_run: None,
            last_result: None,
            created_at: Utc::now(),
        };

        // Save to database
        storage::save_task(db, &task)
            .await
            .map_err(|e| SchedulerToolError(format!("Failed to save task: {}", e)))?;

        // Register with scheduler (if app_handle available)
        if let Some(app_handle) = &self.app_handle {
            register_task_job(
                scheduler,
                task.id.clone(),
                &task.cron_expression,
                task.name.clone(),
                task.task_prompt.clone(),
                task.instance_id.clone(),
                task.notify,
                manager.clone(),
                app_handle.clone(),
            )
            .await
            .map_err(|e| SchedulerToolError(format!("Failed to register job: {}", e)))?;
        }

        Ok(format!(
            "Scheduled task '{}' created (id: {}, cron: '{}').\n\
             The task will run automatically according to the schedule. \
             A temporary agent will execute the prompt each time it fires.",
            args.name, task.id, args.cron_expression
        ))
    }
}

// ---------------------------------------------------------------------------
// ListScheduledTasksTool
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListScheduledTasksArgs {}

/// rig Tool that lists all scheduled tasks for the current instance.
#[derive(Clone, Serialize, Deserialize)]
pub struct ListScheduledTasksTool {
    #[serde(skip)]
    db: Option<Pool<Sqlite>>,
    #[serde(skip, default)]
    instance_id: String,
}

impl ListScheduledTasksTool {
    pub fn new(db: Pool<Sqlite>, instance_id: String) -> Self {
        Self {
            db: Some(db),
            instance_id,
        }
    }
}

impl Tool for ListScheduledTasksTool {
    const NAME: &'static str = "list_scheduled_tasks";
    type Error = SchedulerToolError;
    type Args = ListScheduledTasksArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "list_scheduled_tasks".to_string(),
            description: "List all scheduled tasks for the current AI instance. \
                Shows task name, cron expression, enabled status, and last run info."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| SchedulerToolError("Database not initialized".to_string()))?;

        let tasks = storage::load_tasks(db, &self.instance_id)
            .await
            .map_err(|e| SchedulerToolError(format!("Failed to load tasks: {}", e)))?;

        if tasks.is_empty() {
            return Ok("No scheduled tasks found.".to_string());
        }

        let mut output = format!("Found {} scheduled task(s):\n\n", tasks.len());
        for task in &tasks {
            output.push_str(&format!(
                "- **{}** (id: {})\n  Cron: `{}`\n  Status: {}\n  Prompt: {}\n",
                task.name,
                task.id,
                task.cron_expression,
                if task.enabled { "enabled" } else { "disabled" },
                if task.task_prompt.len() > 100 {
                    format!("{}...", &task.task_prompt[..100])
                } else {
                    task.task_prompt.clone()
                },
            ));
            if let Some(last_run) = &task.last_run {
                output.push_str(&format!("  Last run: {}\n", last_run));
            }
            if let Some(last_result) = &task.last_result {
                let truncated = if last_result.len() > 200 {
                    format!("{}...", &last_result[..200])
                } else {
                    last_result.clone()
                };
                output.push_str(&format!("  Last result: {}\n", truncated));
            }
            output.push('\n');
        }

        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// DeleteScheduledTaskTool
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DeleteScheduledTaskArgs {
    /// The ID of the task to delete.
    task_id: String,
}

/// rig Tool that deletes a scheduled task.
#[derive(Clone, Serialize, Deserialize)]
pub struct DeleteScheduledTaskTool {
    #[serde(skip)]
    db: Option<Pool<Sqlite>>,
    #[serde(skip)]
    scheduler: Option<SharedScheduler>,
}

impl DeleteScheduledTaskTool {
    pub fn new(db: Pool<Sqlite>, scheduler: SharedScheduler) -> Self {
        Self {
            db: Some(db),
            scheduler: Some(scheduler),
        }
    }
}

impl Tool for DeleteScheduledTaskTool {
    const NAME: &'static str = "delete_scheduled_task";
    type Error = SchedulerToolError;
    type Args = DeleteScheduledTaskArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "delete_scheduled_task".to_string(),
            description: "Delete a scheduled task by its ID. The task will be removed \
                from both the database and the active scheduler. Use list_scheduled_tasks \
                to find task IDs."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task_id": {
                        "type": "string",
                        "description": "The ID of the scheduled task to delete"
                    }
                },
                "required": ["task_id"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| SchedulerToolError("Database not initialized".to_string()))?;
        let scheduler = self
            .scheduler
            .as_ref()
            .ok_or_else(|| SchedulerToolError("Scheduler not initialized".to_string()))?;

        // Remove from scheduler
        {
            let mut sched = scheduler.lock().await;
            sched
                .remove_job(&args.task_id)
                .await
                .map_err(|e| SchedulerToolError(format!("Failed to remove job: {}", e)))?;
        }

        // Delete from database
        storage::delete_task(db, &args.task_id)
            .await
            .map_err(|e| SchedulerToolError(format!("Failed to delete task: {}", e)))?;

        Ok(format!(
            "Scheduled task '{}' has been deleted.",
            args.task_id
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_scheduled_task_tool_name() {
        assert_eq!(CreateScheduledTaskTool::NAME, "create_scheduled_task");
    }

    #[test]
    fn test_list_scheduled_tasks_tool_name() {
        assert_eq!(ListScheduledTasksTool::NAME, "list_scheduled_tasks");
    }

    #[test]
    fn test_delete_scheduled_task_tool_name() {
        assert_eq!(DeleteScheduledTaskTool::NAME, "delete_scheduled_task");
    }

    #[tokio::test]
    async fn test_create_definition() {
        let tool = CreateScheduledTaskTool {
            db: None,
            instance_id: String::new(),
            scheduler: None,
            manager: None,
            app_handle: None,
        };
        let def = Tool::definition(&tool, "test".to_string()).await;
        assert_eq!(def.name, "create_scheduled_task");
        assert!(def.description.contains("cron"));
        assert!(def.description.contains("every day at 8:00"));
    }

    #[tokio::test]
    async fn test_list_definition() {
        let tool = ListScheduledTasksTool {
            db: None,
            instance_id: String::new(),
        };
        let def = Tool::definition(&tool, "test".to_string()).await;
        assert_eq!(def.name, "list_scheduled_tasks");
        assert!(def.description.contains("List all scheduled tasks"));
    }

    #[tokio::test]
    async fn test_delete_definition() {
        let tool = DeleteScheduledTaskTool {
            db: None,
            scheduler: None,
        };
        let def = Tool::definition(&tool, "test".to_string()).await;
        assert_eq!(def.name, "delete_scheduled_task");
        assert!(def.description.contains("Delete a scheduled task"));
    }

    #[tokio::test]
    async fn test_create_no_db_fails() {
        let tool = CreateScheduledTaskTool {
            db: None,
            instance_id: String::new(),
            scheduler: None,
            manager: None,
            app_handle: None,
        };
        let result = Tool::call(
            &tool,
            CreateScheduledTaskArgs {
                name: "test".to_string(),
                cron_expression: "0 8 * * *".to_string(),
                task_prompt: "test prompt".to_string(),
                notify: true,
            },
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not initialized"));
    }

    #[tokio::test]
    async fn test_list_no_db_fails() {
        let tool = ListScheduledTasksTool {
            db: None,
            instance_id: String::new(),
        };
        let result = Tool::call(&tool, ListScheduledTasksArgs {}).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_no_db_fails() {
        let tool = DeleteScheduledTaskTool {
            db: None,
            scheduler: None,
        };
        let result = Tool::call(
            &tool,
            DeleteScheduledTaskArgs {
                task_id: "test".to_string(),
            },
        )
        .await;
        assert!(result.is_err());
    }
}
