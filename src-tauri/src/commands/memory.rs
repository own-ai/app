use serde::Serialize;
use tauri::State;

use super::chat::AgentCache;
use crate::database::init_database;
use crate::memory::fact_extraction;

/// Memory statistics for debugging/monitoring
#[derive(Debug, Serialize)]
pub struct MemoryStats {
    pub working_memory_count: usize,
    pub working_memory_tokens: usize,
    pub working_memory_utilization: f32,
    pub long_term_memory_count: i64,
    pub summaries_count: i64,
}

/// Result of a memory search with similarity score
#[derive(Debug, Serialize)]
pub struct MemorySearchResult {
    pub id: String,
    pub content: String,
    pub entry_type: String,
    pub importance: f32,
    pub similarity: f32,
}

/// Get memory statistics for an AI instance
#[tauri::command]
pub async fn get_memory_stats(
    instance_id: String,
    agent_cache: State<'_, AgentCache>,
) -> Result<MemoryStats, String> {
    let cache = agent_cache.lock().await;

    // Get working memory stats from the agent (if it exists in cache)
    let (wm_count, wm_tokens, wm_utilization) = if let Some(agent) = cache.get(&instance_id) {
        let wm = agent.context_builder().working_memory();
        (wm.message_count(), wm.current_tokens(), wm.utilization())
    } else {
        (0, 0, 0.0)
    };

    drop(cache);

    // Get DB-based stats (summaries count, long-term memory count)
    let db = init_database(&instance_id)
        .await
        .map_err(|e| format!("Failed to initialize database: {}", e))?;

    let summaries_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM summaries")
        .fetch_one(&db)
        .await
        .unwrap_or(0);

    let long_term_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_entries")
        .fetch_one(&db)
        .await
        .unwrap_or(0);

    Ok(MemoryStats {
        working_memory_count: wm_count,
        working_memory_tokens: wm_tokens,
        working_memory_utilization: wm_utilization,
        long_term_memory_count: long_term_count,
        summaries_count,
    })
}

/// Search long-term memory semantically
#[tauri::command]
pub async fn search_memory(
    instance_id: String,
    query: String,
    limit: usize,
    agent_cache: State<'_, AgentCache>,
) -> Result<Vec<MemorySearchResult>, String> {
    let cache = agent_cache.lock().await;

    // Agent must be in cache (which contains the embedder in long-term memory)
    let agent = cache
        .get(&instance_id)
        .ok_or_else(|| "Agent not in cache - please send a message first".to_string())?;

    // Perform semantic search via shared long-term memory
    let long_term_memory = agent.context_builder().long_term_memory().clone();
    drop(cache); // Release cache lock before awaiting the memory lock
    let mut mem = long_term_memory.lock().await;
    let memories = mem
        .recall(&query, limit, 0.0) // min_importance = 0.0 to include all
        .await
        .map_err(|e| format!("Failed to search memory: {}", e))?;

    // Convert to search results (we don't have access to similarity scores from recall)
    // So we'll just return importance as a proxy
    let results: Vec<MemorySearchResult> = memories
        .into_iter()
        .map(|mem| MemorySearchResult {
            id: mem.id,
            content: mem.content,
            entry_type: format!("{:?}", mem.entry_type),
            importance: mem.importance,
            similarity: mem.importance, // Proxy - actual similarity not exposed by recall
        })
        .collect();

    Ok(results)
}

/// Manually add a memory entry to long-term memory
#[tauri::command]
pub async fn add_memory_entry(
    instance_id: String,
    content: String,
    entry_type: String,
    importance: f32,
    agent_cache: State<'_, AgentCache>,
) -> Result<String, String> {
    let cache = agent_cache.lock().await;

    // Agent must be in cache
    let agent = cache
        .get(&instance_id)
        .ok_or_else(|| "Agent not in cache - please send a message first".to_string())?;

    // Parse entry type
    let memory_type = fact_extraction::parse_memory_type(&entry_type);

    // Create memory entry
    let entry = crate::memory::MemoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        content,
        entry_type: memory_type,
        importance: importance.clamp(0.0, 1.0),
        created_at: chrono::Utc::now(),
        last_accessed: chrono::Utc::now(),
        access_count: 0,
        tags: Vec::new(),
        source_message_ids: Vec::new(),
    };

    let entry_id = entry.id.clone();

    // Store in long-term memory via shared reference
    let long_term_memory = agent.context_builder().long_term_memory().clone();
    drop(cache); // Release cache lock before awaiting the memory lock
    let mut mem = long_term_memory.lock().await;
    mem.store(entry)
        .await
        .map_err(|e| format!("Failed to store memory entry: {}", e))?;

    tracing::info!("Manually added memory entry: {}", entry_id);

    Ok(entry_id)
}

/// Delete a memory entry from long-term memory
#[tauri::command]
pub async fn delete_memory_entry(
    instance_id: String,
    entry_id: String,
    agent_cache: State<'_, AgentCache>,
) -> Result<(), String> {
    let cache = agent_cache.lock().await;

    // Agent must be in cache
    let agent = cache
        .get(&instance_id)
        .ok_or_else(|| "Agent not in cache - please send a message first".to_string())?;

    // Delete from long-term memory via shared reference
    let long_term_memory = agent.context_builder().long_term_memory().clone();
    drop(cache); // Release cache lock before awaiting the memory lock
    let mem = long_term_memory.lock().await;
    mem.delete(&entry_id)
        .await
        .map_err(|e| format!("Failed to delete memory entry: {}", e))?;

    tracing::info!("Deleted memory entry: {}", entry_id);

    Ok(())
}
