use anyhow::{Context, Result};
use candle_core::{DType, Device};
use chrono::{DateTime, Utc};
use fastembed::Qwen3TextEmbedding;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Row, Sqlite};
use std::sync::Arc;
use tokio::sync::Mutex;

/// A shared reference to long-term memory, safe for concurrent access from tools.
pub type SharedLongTermMemory = Arc<Mutex<LongTermMemory>>;

/// Types of memories
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    Fact,       // "User lives in Berlin"
    Preference, // "Prefers concise answers"
    Skill,      // "Knows Python"
    Context,    // "Working on Project X"
    ToolUsage,  // "Successfully used weather API"
}

/// A memory entry with embeddings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub entry_type: MemoryType,
    pub importance: f32, // 0.0 - 1.0
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub access_count: u32,
    pub tags: Vec<String>,
    pub source_message_ids: Vec<String>,
}

/// Long-term memory with vector search using fastembed
pub struct LongTermMemory {
    embedder: Qwen3TextEmbedding,
    db: Pool<Sqlite>,
}

impl LongTermMemory {
    /// Initialize long-term memory with fastembed embeddings
    pub async fn new(db: Pool<Sqlite>) -> Result<Self> {
        tracing::info!("Initializing fastembed model...");

        // Initialize fastembed
        let device = Device::Cpu;
        let embedder =
            Qwen3TextEmbedding::from_hf("Qwen/Qwen3-Embedding-0.6B", &device, DType::F32, 512)
                .map_err(|e| {
                    tracing::error!("Fastembed initialization failed with error: {:#}", e);
                    e
                })
                .context("Failed to initialize fastembed model")?;

        tracing::info!("Fastembed model loaded successfully");

        // Ensure memory_entries table exists
        Self::create_table(&db).await?;

        Ok(Self { embedder, db })
    }

    /// Create memory_entries table with vector storage
    async fn create_table(db: &Pool<Sqlite>) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS memory_entries (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                embedding BLOB NOT NULL,
                entry_type TEXT NOT NULL,
                importance REAL NOT NULL DEFAULT 0.5,
                created_at DATETIME NOT NULL,
                last_accessed DATETIME NOT NULL,
                access_count INTEGER NOT NULL DEFAULT 0,
                tags TEXT,  -- JSON array
                source_message_ids TEXT  -- JSON array
            )
            "#,
        )
        .execute(db)
        .await?;

        // Create indices
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_memory_type ON memory_entries(entry_type)")
            .execute(db)
            .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_memory_importance ON memory_entries(importance DESC)",
        )
        .execute(db)
        .await?;

        Ok(())
    }

    /// Store a memory entry with its embedding
    pub async fn store(&mut self, entry: MemoryEntry) -> Result<()> {
        // Generate embedding
        let embeddings = self
            .embedder
            .embed(std::slice::from_ref(&entry.content))
            .context("Failed to generate embedding")?;

        let embedding_vec = &embeddings[0];
        let embedding_bytes = Self::vec_to_bytes(embedding_vec);

        // Store in database
        sqlx::query(
            r#"
            INSERT INTO memory_entries 
            (id, content, embedding, entry_type, importance, created_at, last_accessed, 
             access_count, tags, source_message_ids)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&entry.id)
        .bind(&entry.content)
        .bind(&embedding_bytes)
        .bind(serde_json::to_string(&entry.entry_type)?)
        .bind(entry.importance)
        .bind(entry.created_at)
        .bind(entry.last_accessed)
        .bind(entry.access_count as i32)
        .bind(serde_json::to_string(&entry.tags)?)
        .bind(serde_json::to_string(&entry.source_message_ids)?)
        .execute(&self.db)
        .await?;

        tracing::info!(
            "Stored memory: {} (type: {:?}, importance: {})",
            entry.id,
            entry.entry_type,
            entry.importance
        );

        Ok(())
    }

    /// Recall memories using semantic search
    pub async fn recall(
        &mut self,
        query: &str,
        limit: usize,
        min_importance: f32,
    ) -> Result<Vec<MemoryEntry>> {
        // Generate query embedding
        let query_embeddings = self
            .embedder
            .embed(&[query.to_string()])
            .context("Failed to generate query embedding")?;

        let query_vec = &query_embeddings[0];

        // Fetch all memories above importance threshold
        let rows = sqlx::query(
            r#"
            SELECT id, content, embedding, entry_type, importance, created_at, 
                   last_accessed, access_count, tags, source_message_ids
            FROM memory_entries
            WHERE importance >= ?
            "#,
        )
        .bind(min_importance)
        .fetch_all(&self.db)
        .await?;

        // Calculate cosine similarity and sort
        let mut scored_memories: Vec<(f32, MemoryEntry)> = rows
            .into_iter()
            .filter_map(|row| {
                let embedding_bytes: Vec<u8> = row.get("embedding");
                let embedding = Self::bytes_to_vec(&embedding_bytes);
                let similarity = Self::cosine_similarity(query_vec, &embedding);

                // Parse memory entry
                let entry = MemoryEntry {
                    id: row.get("id"),
                    content: row.get("content"),
                    entry_type: serde_json::from_str(row.get("entry_type")).ok()?,
                    importance: row.get("importance"),
                    created_at: row.get("created_at"),
                    last_accessed: row.get("last_accessed"),
                    access_count: row.get::<i32, _>("access_count") as u32,
                    tags: serde_json::from_str(row.get("tags")).ok()?,
                    source_message_ids: serde_json::from_str(row.get("source_message_ids")).ok()?,
                };

                Some((similarity, entry))
            })
            .collect();

        // Sort by similarity (descending)
        scored_memories.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Take top N and update access tracking
        let mut results = Vec::new();
        for (similarity, mut memory) in scored_memories.into_iter().take(limit) {
            memory.last_accessed = Utc::now();
            memory.access_count += 1;

            // Update in database
            sqlx::query(
                "UPDATE memory_entries SET last_accessed = ?, access_count = ? WHERE id = ?",
            )
            .bind(memory.last_accessed)
            .bind(memory.access_count as i32)
            .bind(&memory.id)
            .execute(&self.db)
            .await?;

            tracing::debug!(
                "Recalled memory: {} (similarity: {:.3})",
                memory.id,
                similarity
            );
            results.push(memory);
        }

        Ok(results)
    }

    /// Calculate cosine similarity between two vectors
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }

        dot_product / (norm_a * norm_b)
    }

    /// Convert vector to bytes for storage
    fn vec_to_bytes(vec: &[f32]) -> Vec<u8> {
        vec.iter().flat_map(|f| f.to_le_bytes()).collect()
    }

    /// Convert bytes back to vector
    fn bytes_to_vec(bytes: &[u8]) -> Vec<f32> {
        bytes
            .chunks(4)
            .map(|chunk| {
                let arr: [u8; 4] = chunk.try_into().unwrap();
                f32::from_le_bytes(arr)
            })
            .collect()
    }

    /// Delete a memory entry by ID
    pub async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM memory_entries WHERE id = ?")
            .bind(id)
            .execute(&self.db)
            .await?;

        tracing::info!("Deleted memory entry: {}", id);
        Ok(())
    }

    /// Search memories by type
    pub async fn search_by_type(
        &self,
        entry_type: &MemoryType,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>> {
        let rows = sqlx::query(
            r#"
            SELECT id, content, entry_type, importance, created_at,
                   last_accessed, access_count, tags, source_message_ids
            FROM memory_entries
            WHERE entry_type = ?
            ORDER BY importance DESC, created_at DESC
            LIMIT ?
            "#,
        )
        .bind(serde_json::to_string(entry_type)?)
        .bind(limit as i32)
        .fetch_all(&self.db)
        .await?;

        let entries: Vec<MemoryEntry> = rows
            .into_iter()
            .filter_map(|row| {
                Some(MemoryEntry {
                    id: row.get("id"),
                    content: row.get("content"),
                    entry_type: serde_json::from_str(row.get("entry_type")).ok()?,
                    importance: row.get("importance"),
                    created_at: row.get("created_at"),
                    last_accessed: row.get("last_accessed"),
                    access_count: row.get::<i32, _>("access_count") as u32,
                    tags: serde_json::from_str(row.get("tags")).ok()?,
                    source_message_ids: serde_json::from_str(row.get("source_message_ids")).ok()?,
                })
            })
            .collect();

        tracing::debug!("Found {} memories of type {:?}", entries.len(), entry_type);

        Ok(entries)
    }

    /// Count total number of memory entries
    pub async fn count(&self) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_entries")
            .fetch_one(&self.db)
            .await?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create an in-memory SQLite database for testing
    async fn setup_test_db() -> Pool<Sqlite> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory database");

        pool
    }

    /// Helper: create a test memory entry
    fn create_test_entry(id: &str, content: &str, entry_type: MemoryType) -> MemoryEntry {
        MemoryEntry {
            id: id.to_string(),
            content: content.to_string(),
            entry_type,
            importance: 0.7,
            created_at: Utc::now(),
            last_accessed: Utc::now(),
            access_count: 0,
            tags: vec![],
            source_message_ids: vec![],
        }
    }

    #[tokio::test]
    #[ignore] // Requires fastembed model download (slow, ~1GB)
    async fn test_delete_memory_entry() {
        let db = setup_test_db().await;
        let mut memory = LongTermMemory::new(db)
            .await
            .expect("Failed to create LongTermMemory");

        // Store a memory entry
        let entry = create_test_entry("test_id_1", "User likes pizza", MemoryType::Preference);
        memory.store(entry).await.expect("Failed to store entry");

        // Verify it exists
        let count_before = memory.count().await.expect("Failed to count");
        assert_eq!(count_before, 1);

        // Delete it
        memory.delete("test_id_1").await.expect("Failed to delete");

        // Verify it's gone
        let count_after = memory.count().await.expect("Failed to count");
        assert_eq!(count_after, 0);
    }

    #[tokio::test]
    #[ignore] // Requires fastembed model download (slow, ~1GB)
    async fn test_search_by_type() {
        let db = setup_test_db().await;
        let mut memory = LongTermMemory::new(db)
            .await
            .expect("Failed to create LongTermMemory");

        // Store multiple entries of different types
        let entries = vec![
            create_test_entry("e1", "User knows Rust", MemoryType::Skill),
            create_test_entry("e2", "User lives in Berlin", MemoryType::Fact),
            create_test_entry("e3", "User prefers dark mode", MemoryType::Preference),
            create_test_entry("e4", "User knows Python", MemoryType::Skill),
        ];

        for entry in entries {
            memory.store(entry).await.expect("Failed to store");
        }

        // Search for skills only
        let skills = memory
            .search_by_type(&MemoryType::Skill, 10)
            .await
            .expect("Failed to search by type");

        assert_eq!(skills.len(), 2);
        assert!(skills.iter().all(|e| e.entry_type == MemoryType::Skill));

        // Search for preferences
        let prefs = memory
            .search_by_type(&MemoryType::Preference, 10)
            .await
            .expect("Failed to search by type");

        assert_eq!(prefs.len(), 1);
        assert_eq!(prefs[0].content, "User prefers dark mode");
    }

    #[tokio::test]
    #[ignore] // Requires fastembed model download (slow, ~1GB)
    async fn test_search_by_type_respects_limit() {
        let db = setup_test_db().await;
        let mut memory = LongTermMemory::new(db)
            .await
            .expect("Failed to create LongTermMemory");

        // Store 5 facts
        for i in 0..5 {
            let entry = create_test_entry(
                &format!("fact_{}", i),
                &format!("Fact number {}", i),
                MemoryType::Fact,
            );
            memory.store(entry).await.expect("Failed to store");
        }

        // Request only 3
        let facts = memory
            .search_by_type(&MemoryType::Fact, 3)
            .await
            .expect("Failed to search");

        assert_eq!(facts.len(), 3);
    }

    #[tokio::test]
    #[ignore] // Requires fastembed model download (slow, ~1GB)
    async fn test_count() {
        let db = setup_test_db().await;
        let mut memory = LongTermMemory::new(db)
            .await
            .expect("Failed to create LongTermMemory");

        // Initially empty
        assert_eq!(memory.count().await.expect("Failed to count"), 0);

        // Add entries
        for i in 0..3 {
            let entry = create_test_entry(
                &format!("entry_{}", i),
                &format!("Content {}", i),
                MemoryType::Fact,
            );
            memory.store(entry).await.expect("Failed to store");
        }

        assert_eq!(memory.count().await.expect("Failed to count"), 3);

        // Delete one
        memory.delete("entry_1").await.expect("Failed to delete");

        assert_eq!(memory.count().await.expect("Failed to count"), 2);
    }

    #[tokio::test]
    #[ignore] // Requires fastembed model download (slow, ~1GB)
    async fn test_search_by_type_empty_result() {
        let db = setup_test_db().await;
        let memory = LongTermMemory::new(db)
            .await
            .expect("Failed to create LongTermMemory");

        // Search when no entries exist
        let results = memory
            .search_by_type(&MemoryType::Context, 10)
            .await
            .expect("Failed to search");

        assert_eq!(results.len(), 0);
    }
}
