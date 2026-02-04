use crate::memory::{MemoryEntry, MemoryType};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::State;

/// Memory statistics for debugging/monitoring
#[derive(Debug, Serialize)]
pub struct MemoryStats {
    pub working_memory_count: usize,
    pub working_memory_tokens: usize,
    pub working_memory_utilization: f32,
    pub long_term_memory_count: usize,
    pub summaries_count: usize,
}

/// Test command to store a test memory entry
#[tauri::command]
pub async fn test_store_memory(
    instance_id: String,
    content: String,
    entry_type: String,
) -> Result<String, String> {
    // This is a test command for Phase 2.5
    // In Phase 3, this will be integrated into the actual agent
    
    tracing::info!(
        "Test: Storing memory for instance {} - {}",
        instance_id,
        content
    );
    
    Ok(format!(
        "Memory stored (test): {} - Type: {}",
        content, entry_type
    ))
}

/// Test command to recall memories
#[tauri::command]
pub async fn test_recall_memories(
    instance_id: String,
    query: String,
) -> Result<Vec<String>, String> {
    // This is a test command for Phase 2.5
    // In Phase 3, this will be integrated into the actual agent
    
    tracing::info!(
        "Test: Recalling memories for instance {} - Query: {}",
        instance_id,
        query
    );
    
    // Return mock results for now
    Ok(vec![
        format!("Fact: User prefers {} (similarity: 0.85)", query),
        format!("Context: Previously discussed {} (similarity: 0.72)", query),
    ])
}

/// Get memory statistics for an AI instance
#[tauri::command]
pub async fn get_memory_stats(instance_id: String) -> Result<MemoryStats, String> {
    // This is a test command for Phase 2.5
    // In Phase 3, this will be integrated into the actual agent
    
    tracing::info!("Getting memory stats for instance {}", instance_id);
    
    // Return mock stats for now
    Ok(MemoryStats {
        working_memory_count: 15,
        working_memory_tokens: 3500,
        working_memory_utilization: 7.0, // 7% of 50k tokens
        long_term_memory_count: 42,
        summaries_count: 3,
    })
}
