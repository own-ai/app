use serde::Serialize;
use tauri::State;

use super::chat::AgentCache;
use crate::database::init_database;

/// Memory statistics for debugging/monitoring
#[derive(Debug, Serialize)]
pub struct MemoryStats {
    pub working_memory_count: usize,
    pub working_memory_tokens: usize,
    pub working_memory_utilization: f32,
    pub long_term_memory_count: i64,
    pub summaries_count: i64,
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
        (
            wm.message_count(),
            wm.current_tokens(),
            wm.utilization(),
        )
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
