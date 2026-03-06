use serde::Serialize;
use tauri::State;

use super::chat::AgentCache;
use crate::database::{get_or_init_db, DbCache};
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
    db_cache: State<'_, DbCache>,
) -> Result<MemoryStats, String> {
    // Read-lock the cache briefly to clone the Arc, then release it
    // before locking the agent (avoids holding cache lock during agent wait).
    let (wm_count, wm_tokens, wm_utilization) = {
        let agent_arc = {
            let cache = agent_cache.read().await;
            cache.get(&instance_id).cloned()
        };
        if let Some(agent_arc) = agent_arc {
            let agent = agent_arc.lock().await;
            let wm = agent.context_builder().working_memory();
            (wm.message_count(), wm.current_tokens(), wm.utilization())
        } else {
            (0, 0, 0.0)
        }
    };

    // Get DB-based stats (summaries count, long-term memory count)
    let db = get_or_init_db(&db_cache, &instance_id)
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
    // Read-lock cache briefly to get the agent Arc, then lock agent briefly
    // to clone the shared long-term memory reference.
    let long_term_memory = {
        let cache = agent_cache.read().await;
        let agent_arc = cache
            .get(&instance_id)
            .ok_or_else(|| "Agent not in cache - please send a message first".to_string())?
            .clone();
        drop(cache); // Release cache lock

        let agent = agent_arc.lock().await;
        agent.context_builder().long_term_memory().clone()
        // agent lock dropped here
    };

    // Perform semantic search (no cache or agent lock held)
    let mut mem = long_term_memory.lock().await;
    let memories = mem
        .recall(&query, limit, 0.0) // min_importance = 0.0 to include all
        .await
        .map_err(|e| format!("Failed to search memory: {}", e))?;

    let results: Vec<MemorySearchResult> = memories
        .into_iter()
        .map(|(similarity, mem)| MemorySearchResult {
            id: mem.id,
            content: mem.content,
            entry_type: format!("{:?}", mem.entry_type),
            importance: mem.importance,
            similarity,
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
    // Read-lock cache briefly, then lock agent briefly to clone shared ref
    let long_term_memory = {
        let cache = agent_cache.read().await;
        let agent_arc = cache
            .get(&instance_id)
            .ok_or_else(|| "Agent not in cache - please send a message first".to_string())?
            .clone();
        drop(cache);

        let agent = agent_arc.lock().await;
        agent.context_builder().long_term_memory().clone()
    };

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
        collection_id: None,
    };

    let entry_id = entry.id.clone();

    // Store in long-term memory (no cache or agent lock held)
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
    // Read-lock cache briefly, then lock agent briefly to clone shared ref
    let long_term_memory = {
        let cache = agent_cache.read().await;
        let agent_arc = cache
            .get(&instance_id)
            .ok_or_else(|| "Agent not in cache - please send a message first".to_string())?
            .clone();
        drop(cache);

        let agent = agent_arc.lock().await;
        agent.context_builder().long_term_memory().clone()
    };

    // Delete from long-term memory (no cache or agent lock held)
    let mem = long_term_memory.lock().await;
    mem.delete(&entry_id)
        .await
        .map_err(|e| format!("Failed to delete memory entry: {}", e))?;

    tracing::info!("Deleted memory entry: {}", entry_id);

    Ok(())
}
