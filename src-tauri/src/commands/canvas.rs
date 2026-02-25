use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

use crate::ai_instances::AIInstanceManager;
use crate::canvas::bridge::{self, BridgeResponse};
use crate::canvas::storage;
use crate::canvas::ProgramMetadata;
use crate::commands::chat::AgentCache;
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

/// Handle a Bridge API request from a Canvas program iframe.
///
/// The frontend forwards postMessage requests from the iframe to this command.
/// Dispatches to the appropriate bridge handler based on the method.
#[tauri::command]
pub async fn bridge_request(
    instance_id: String,
    program_name: String,
    method: String,
    params: serde_json::Value,
    app_handle: tauri::AppHandle,
    instance_manager: State<'_, Arc<Mutex<AIInstanceManager>>>,
    agent_cache: State<'_, AgentCache>,
) -> Result<BridgeResponse, String> {
    let pool = init_database(&instance_id)
        .await
        .map_err(|e| e.to_string())?;

    let workspace = paths::get_instance_workspace_path(&instance_id)
        .map_err(|e| format!("Failed to get workspace path: {}", e))?;

    match method.as_str() {
        "chat" => {
            let prompt = params
                .get("prompt")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'prompt' parameter")?
                .to_string();

            // Get instance
            let manager = instance_manager.lock().await;
            let instance = manager
                .get_instance(&instance_id)
                .ok_or_else(|| format!("Instance not found: {}", instance_id))?
                .clone();
            drop(manager);

            // Get or create agent
            let mut cache = agent_cache.lock().await;

            if !cache.contains_key(&instance_id) {
                let db = init_database(&instance_id)
                    .await
                    .map_err(|e| format!("Failed to initialize database: {}", e))?;

                let agent =
                    crate::agent::OwnAIAgent::new(&instance, db, None, Some(app_handle.clone()))
                        .await
                        .map_err(|e| format!("Failed to create agent: {}", e))?;

                cache.insert(instance_id.clone(), agent);
            }

            let agent = cache
                .get_mut(&instance_id)
                .ok_or("Agent not found in cache")?;

            match agent.chat(&prompt).await {
                Ok(response) => Ok(BridgeResponse::ok(serde_json::Value::String(response))),
                Err(e) => Ok(BridgeResponse::err(format!("Chat error: {}", e))),
            }
        }

        "storeData" => {
            let key = params
                .get("key")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'key' parameter")?;
            let value = params.get("value").unwrap_or(&serde_json::Value::Null);

            Ok(bridge::handle_store_data(&pool, &program_name, key, value).await)
        }

        "loadData" => {
            let key = params
                .get("key")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'key' parameter")?;

            Ok(bridge::handle_load_data(&pool, &program_name, key).await)
        }

        "notify" => {
            let message = params
                .get("message")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'message' parameter")?;
            let delay_ms = params.get("delay_ms").and_then(|v| v.as_u64());

            // Resolve instance name for notification title
            let manager = instance_manager.lock().await;
            let instance_name = manager
                .get_instance(&instance_id)
                .map(|i| i.name.clone())
                .unwrap_or_else(|| "ownAI".to_string());
            drop(manager);

            Ok(bridge::handle_notify(Some(&app_handle), &instance_name, message, delay_ms).await)
        }

        "readFile" => {
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'path' parameter")?;

            Ok(bridge::handle_read_file(&workspace, path).await)
        }

        "writeFile" => {
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'path' parameter")?;
            let content = params
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'content' parameter")?;

            Ok(bridge::handle_write_file(&workspace, path, content).await)
        }

        _ => Ok(BridgeResponse::err(format!("Unknown method: {}", method))),
    }
}
