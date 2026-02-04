use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Row, Sqlite};

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
    embedder: TextEmbedding,
    db: Pool<Sqlite>,
}

impl LongTermMemory {
    /// Initialize long-term memory with fastembed embeddings
    pub async fn new(db: Pool<Sqlite>) -> Result<Self> {
        tracing::info!("Initializing fastembed model...");
        
        // Initialize fastembed with default model (all-MiniLM-L6-v2)
        let embedder = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::AllMiniLML6V2).with_show_download_progress(true),
        )
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
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_memory_type ON memory_entries(entry_type)",
        )
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
            .embed(vec![entry.content.clone()], None)
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
            .embed(vec![query.to_string()], None)
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
            
            tracing::debug!("Recalled memory: {} (similarity: {:.3})", memory.id, similarity);
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
        vec.iter()
            .flat_map(|f| f.to_le_bytes())
            .collect()
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
}
