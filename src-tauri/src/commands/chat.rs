use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{Emitter, Manager, State};
use tokio::sync::{Mutex, RwLock};

use crate::agent::OwnAIAgent;
use crate::ai_instances::AIInstanceManager;
use crate::database::{get_or_init_db, DbCache};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub instance_id: String,
    pub content: String,
}

/// Agent cache to avoid recreating agents for each message.
///
/// Uses an outer `RwLock` on the HashMap (locked briefly to look up / insert
/// entries) and a per-instance `Mutex<OwnAIAgent>` so that long-running
/// operations (streaming, tool calls) only block the **same** instance
/// instead of the entire cache.
pub type AgentCache = Arc<RwLock<HashMap<String, Arc<Mutex<OwnAIAgent>>>>>;

/// Helper: get an existing agent from cache, or create a new one.
///
/// The outer `RwLock` is held only briefly (read to look up, write to insert).
/// Returns an `Arc<Mutex<OwnAIAgent>>` that callers lock independently,
/// so the cache itself is free for other instances / commands.
pub async fn get_or_create_agent(
    instance_id: &str,
    instance_manager: &Arc<Mutex<AIInstanceManager>>,
    agent_cache: &AgentCache,
    db_cache: &DbCache,
    app_handle: &tauri::AppHandle,
) -> Result<Arc<Mutex<OwnAIAgent>>, String> {
    // Fast path: read-lock to check if agent already exists
    {
        let cache = agent_cache.read().await;
        if let Some(agent_arc) = cache.get(instance_id) {
            return Ok(agent_arc.clone());
        }
    }

    // Slow path: agent does not exist yet, create it
    let manager = instance_manager.lock().await;
    let instance = manager
        .get_instance(instance_id)
        .ok_or_else(|| format!("Instance not found: {}", instance_id))?
        .clone();
    drop(manager);

    let db = get_or_init_db(db_cache, instance_id)
        .await
        .map_err(|e| format!("Failed to initialize database: {}", e))?;

    let agent = OwnAIAgent::new(&instance, db, None, Some(app_handle.clone()))
        .await
        .map_err(|e| format!("Failed to create agent: {}", e))?;

    let agent_arc = Arc::new(Mutex::new(agent));

    // Write-lock to insert; use entry API to handle race (another task
    // may have inserted the same instance in the meantime).
    let mut cache = agent_cache.write().await;
    let entry = cache
        .entry(instance_id.to_string())
        .or_insert_with(|| agent_arc.clone());
    Ok(entry.clone())
}

/// Send a message and get AI response (non-streaming)
#[tauri::command]
pub async fn send_message(
    request: SendMessageRequest,
    app_handle: tauri::AppHandle,
    instance_manager: State<'_, Arc<Mutex<AIInstanceManager>>>,
    agent_cache: State<'_, AgentCache>,
    db_cache: State<'_, DbCache>,
) -> Result<Message, String> {
    // 1. Get or create agent (cache lock released immediately)
    let agent_arc = get_or_create_agent(
        &request.instance_id,
        instance_manager.inner(),
        agent_cache.inner(),
        db_cache.inner(),
        &app_handle,
    )
    .await?;

    // 2. Lock only this instance's agent for the chat call
    let mut agent = agent_arc.lock().await;

    let response_content = agent
        .chat(&request.content)
        .await
        .map_err(|e| format!("Agent error: {}", e))?;

    // 3. Return response message
    let response = Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: "agent".to_string(),
        content: response_content,
        timestamp: Utc::now().to_rfc3339(),
        metadata: None,
    };

    tracing::info!(
        "Message processed for instance: {} (length: {})",
        request.instance_id,
        response.content.len()
    );

    Ok(response)
}

/// Stream a message and get AI response chunk by chunk
#[tauri::command]
pub async fn stream_message(
    request: SendMessageRequest,
    window: tauri::Window,
    instance_manager: State<'_, Arc<Mutex<AIInstanceManager>>>,
    agent_cache: State<'_, AgentCache>,
    db_cache: State<'_, DbCache>,
) -> Result<(), String> {
    // 1. Get or create agent (cache lock released immediately)
    let agent_arc = get_or_create_agent(
        &request.instance_id,
        instance_manager.inner(),
        agent_cache.inner(),
        db_cache.inner(),
        window.app_handle(),
    )
    .await?;

    // 2. Lock only this instance's agent for streaming
    let mut agent = agent_arc.lock().await;

    let window_clone = window.clone();
    let instance_id = request.instance_id.clone();

    agent
        .stream_chat(&request.content, move |chunk| {
            // Emit each chunk to the frontend
            if let Err(e) = window_clone.emit("agent:token", chunk) {
                tracing::error!("Failed to emit token: {}", e);
            }
        })
        .await
        .map_err(|e| format!("Streaming error: {}", e))?;

    tracing::info!("Streaming completed for instance: {}", instance_id);

    Ok(())
}

/// Load messages from the database
#[tauri::command]
pub async fn load_messages(
    instance_id: String,
    limit: i32,
    offset: i32,
    db_cache: State<'_, DbCache>,
) -> Result<Vec<Message>, String> {
    let pool = get_or_init_db(&db_cache, &instance_id)
        .await
        .map_err(|e| e.to_string())?;

    let messages = sqlx::query_as::<_, (String, String, String, String, Option<String>)>(
        r#"
        SELECT id, role, content, timestamp, metadata
        FROM messages
        ORDER BY timestamp ASC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await
    .map_err(|e| format!("Failed to load messages: {}", e))?;

    let messages: Vec<Message> = messages
        .into_iter()
        .map(|(id, role, content, timestamp, metadata)| Message {
            id,
            role,
            content,
            timestamp,
            metadata: metadata.and_then(|m| serde_json::from_str(&m).ok()),
        })
        .collect();

    tracing::debug!(
        "Loaded {} messages for instance: {}",
        messages.len(),
        instance_id
    );

    Ok(messages)
}

/// Clear agent cache for an instance (useful when switching models/settings)
#[tauri::command]
pub async fn clear_agent_cache(
    instance_id: String,
    agent_cache: State<'_, AgentCache>,
) -> Result<(), String> {
    let mut cache = agent_cache.write().await;
    cache.remove(&instance_id);

    tracing::info!("Agent cache cleared for instance: {}", instance_id);

    Ok(())
}
