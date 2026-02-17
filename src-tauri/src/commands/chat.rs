use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{Emitter, State};
use tokio::sync::Mutex;

use crate::agent::OwnAIAgent;
use crate::ai_instances::AIInstanceManager;
use crate::database::init_database;

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

/// Agent cache to avoid recreating agents for each message
pub type AgentCache = Arc<Mutex<HashMap<String, OwnAIAgent>>>;

/// Send a message and get AI response (non-streaming)
#[tauri::command]
pub async fn send_message(
    request: SendMessageRequest,
    instance_manager: State<'_, Arc<Mutex<AIInstanceManager>>>,
    agent_cache: State<'_, AgentCache>,
) -> Result<Message, String> {
    // 1. Get instance
    let manager = instance_manager.lock().await;
    let instance = manager
        .get_instance(&request.instance_id)
        .ok_or_else(|| format!("Instance not found: {}", request.instance_id))?
        .clone();
    drop(manager);

    // 2. Get or create agent
    let mut cache = agent_cache.lock().await;

    if !cache.contains_key(&request.instance_id) {
        // Initialize database for this instance
        let db = init_database(&request.instance_id)
            .await
            .map_err(|e| format!("Failed to initialize database: {}", e))?;

        // Create new agent
        let agent = OwnAIAgent::new(&instance, db, None)
            .await
            .map_err(|e| format!("Failed to create agent: {}", e))?;

        cache.insert(request.instance_id.clone(), agent);
    }

    let agent = cache
        .get_mut(&request.instance_id)
        .ok_or("Agent not found in cache")?;

    // 3. Chat with agent
    let response_content = agent
        .chat(&request.content)
        .await
        .map_err(|e| format!("Agent error: {}", e))?;

    // 4. Return response message
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
) -> Result<(), String> {
    // 1. Get instance
    let manager = instance_manager.lock().await;
    let instance = manager
        .get_instance(&request.instance_id)
        .ok_or_else(|| format!("Instance not found: {}", request.instance_id))?
        .clone();
    drop(manager);

    // 2. Get or create agent
    let mut cache = agent_cache.lock().await;

    if !cache.contains_key(&request.instance_id) {
        // Initialize database for this instance
        let db = init_database(&request.instance_id)
            .await
            .map_err(|e| format!("Failed to initialize database: {}", e))?;

        // Create new agent
        let agent = OwnAIAgent::new(&instance, db, None)
            .await
            .map_err(|e| format!("Failed to create agent: {}", e))?;

        cache.insert(request.instance_id.clone(), agent);
    }

    let agent = cache
        .get_mut(&request.instance_id)
        .ok_or("Agent not found in cache")?;

    // 3. Stream chat with agent
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
) -> Result<Vec<Message>, String> {
    let pool = init_database(&instance_id)
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
    let mut cache = agent_cache.lock().await;
    cache.remove(&instance_id);

    tracing::info!("Agent cache cleared for instance: {}", instance_id);

    Ok(())
}
