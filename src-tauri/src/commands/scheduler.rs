//! Tauri commands for scheduled task management.

use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

use crate::ai_instances::AIInstanceManager;
use crate::database::init_database;
use crate::scheduler::{storage, SharedScheduler};

/// List all scheduled tasks for an instance.
#[tauri::command]
pub async fn list_scheduled_tasks(instance_id: String) -> Result<Vec<serde_json::Value>, String> {
    let db = init_database(&instance_id)
        .await
        .map_err(|e| format!("Failed to initialize database: {}", e))?;

    let tasks = storage::load_tasks(&db, &instance_id)
        .await
        .map_err(|e| format!("Failed to load tasks: {}", e))?;

    let result: Vec<serde_json::Value> = tasks
        .into_iter()
        .map(|t| serde_json::to_value(t).unwrap_or_default())
        .collect();

    Ok(result)
}

/// Delete a scheduled task.
#[tauri::command]
pub async fn delete_scheduled_task(
    instance_id: String,
    task_id: String,
    scheduler: State<'_, SharedScheduler>,
) -> Result<(), String> {
    let db = init_database(&instance_id)
        .await
        .map_err(|e| format!("Failed to initialize database: {}", e))?;

    // Remove from scheduler
    {
        let mut sched = scheduler.lock().await;
        sched
            .remove_job(&task_id)
            .await
            .map_err(|e| format!("Failed to remove job: {}", e))?;
    }

    // Delete from database
    storage::delete_task(&db, &task_id)
        .await
        .map_err(|e| format!("Failed to delete task: {}", e))?;

    Ok(())
}

/// Toggle a scheduled task's enabled state.
#[tauri::command]
pub async fn toggle_scheduled_task(
    instance_id: String,
    task_id: String,
    enabled: bool,
    scheduler: State<'_, SharedScheduler>,
    instance_manager: State<'_, Arc<Mutex<AIInstanceManager>>>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let db = init_database(&instance_id)
        .await
        .map_err(|e| format!("Failed to initialize database: {}", e))?;

    // Update database
    storage::set_task_enabled(&db, &task_id, enabled)
        .await
        .map_err(|e| format!("Failed to update task: {}", e))?;

    if enabled {
        // Re-register the job
        let task = storage::get_task(&db, &task_id)
            .await
            .map_err(|e| format!("Failed to get task: {}", e))?
            .ok_or_else(|| "Task not found".to_string())?;

        crate::scheduler::runner::register_task_job(
            &scheduler,
            task.id.clone(),
            &task.cron_expression,
            task.name,
            task.task_prompt,
            task.instance_id,
            instance_manager.inner().clone(),
            app_handle,
        )
        .await
        .map_err(|e| format!("Failed to register job: {}", e))?;
    } else {
        // Remove the job
        let mut sched = scheduler.lock().await;
        sched
            .remove_job(&task_id)
            .await
            .map_err(|e| format!("Failed to remove job: {}", e))?;
    }

    Ok(())
}
