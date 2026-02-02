use crate::ai_instances::{AIInstance, AIInstanceManager, CreateInstanceRequest};
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

/// Create a new AI instance
#[tauri::command]
pub async fn create_ai_instance(
    request: CreateInstanceRequest,
    manager: State<'_, Arc<Mutex<AIInstanceManager>>>,
) -> Result<AIInstance, String> {
    let mut manager = manager.lock().await;
    manager
        .create_instance(request.name)
        .map_err(|e| e.to_string())
}

/// List all AI instances
#[tauri::command]
pub async fn list_ai_instances(
    manager: State<'_, Arc<Mutex<AIInstanceManager>>>,
) -> Result<Vec<AIInstance>, String> {
    let manager = manager.lock().await;
    Ok(manager.list_instances())
}

/// Set the active AI instance
#[tauri::command]
pub async fn set_active_instance(
    instance_id: String,
    manager: State<'_, Arc<Mutex<AIInstanceManager>>>,
) -> Result<(), String> {
    let mut manager = manager.lock().await;
    manager.set_active(instance_id).map_err(|e| e.to_string())
}

/// Get the currently active AI instance
#[tauri::command]
pub async fn get_active_instance(
    manager: State<'_, Arc<Mutex<AIInstanceManager>>>,
) -> Result<Option<AIInstance>, String> {
    let manager = manager.lock().await;
    Ok(manager.get_active_instance().cloned())
}

/// Delete an AI instance
#[tauri::command]
pub async fn delete_ai_instance(
    instance_id: String,
    manager: State<'_, Arc<Mutex<AIInstanceManager>>>,
) -> Result<(), String> {
    let mut manager = manager.lock().await;
    manager
        .delete_instance(&instance_id)
        .map_err(|e| e.to_string())
}
