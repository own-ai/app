use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

use crate::ai_instances::AIInstanceManager;
use crate::canvas::storage;
use crate::canvas::ProgramMetadata;
use crate::database::init_database;
use crate::utils::paths;

/// List all Canvas programs for an instance.
#[tauri::command]
pub async fn list_programs(instance_id: String) -> Result<Vec<ProgramMetadata>, String> {
    let pool = init_database(&instance_id)
        .await
        .map_err(|e| e.to_string())?;

    storage::list_programs_from_db(&pool, &instance_id)
        .await
        .map_err(|e| format!("Failed to list programs: {}", e))
}

/// Delete a Canvas program by name.
#[tauri::command]
pub async fn delete_program(instance_id: String, program_name: String) -> Result<(), String> {
    let pool = init_database(&instance_id)
        .await
        .map_err(|e| e.to_string())?;

    let programs_root = paths::get_instance_programs_path(&instance_id)
        .map_err(|e| format!("Failed to get programs path: {}", e))?;

    storage::delete_program_from_db(&pool, &instance_id, &program_name, &programs_root)
        .await
        .map_err(|e| format!("Failed to delete program: {}", e))
}

/// Get the custom protocol URL for a program (used by frontend to load in iframe).
#[tauri::command]
pub async fn get_program_url(
    instance_id: String,
    program_name: String,
    instance_manager: State<'_, Arc<Mutex<AIInstanceManager>>>,
) -> Result<String, String> {
    // Verify instance exists
    let manager = instance_manager.lock().await;
    manager
        .get_instance(&instance_id)
        .ok_or_else(|| format!("Instance not found: {}", instance_id))?;
    drop(manager);

    // Verify program exists
    let pool = init_database(&instance_id)
        .await
        .map_err(|e| e.to_string())?;

    storage::get_program_by_name(&pool, &instance_id, &program_name)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| format!("Program '{}' not found", program_name))?;

    Ok(format!(
        "ownai-program://localhost/{}/{}/index.html",
        instance_id, program_name
    ))
}
