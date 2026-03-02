//! Document ingestion pipeline.
//!
//! Combines document parsing, text chunking, and memory storage
//! to ingest documents into Knowledge Collections.

use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::{Pool, Sqlite};
use std::path::Path;

use super::chunking::{chunk_text, ChunkingConfig};
use super::collections::{
    clear_collection_entries, find_or_create_collection, update_collection_count,
};
use super::document_parser;
use super::long_term::LongTermMemory;
use super::MemoryEntry;
use super::MemoryType;

/// Result of a document ingestion operation.
#[derive(Debug)]
pub struct IngestResult {
    /// ID of the collection the document was ingested into.
    pub collection_id: String,
    /// Name of the collection.
    pub collection_name: String,
    /// Number of chunks created and stored.
    pub chunks_created: usize,
    /// Total characters in the extracted text.
    pub total_chars: usize,
}

/// Ingest a document into a Knowledge Collection.
///
/// This function:
/// 1. Extracts text from the file (supports PDF, DOCX, Text, Markdown)
/// 2. Chunks the text into overlapping segments
/// 3. Creates or finds the target collection
/// 4. Optionally clears existing entries (if `replace_existing` is true)
/// 5. Stores each chunk as a `MemoryEntry` with the collection's ID
/// 6. Updates the collection's document count
#[allow(clippy::too_many_arguments)]
pub async fn ingest_document(
    file_path: &Path,
    collection_name: &str,
    collection_description: &str,
    source: Option<&str>,
    chunking_config: &ChunkingConfig,
    entry_type: MemoryType,
    importance: f32,
    replace_existing: bool,
    db: &Pool<Sqlite>,
    memory: &mut LongTermMemory,
) -> Result<IngestResult> {
    // 1. Extract text from the document
    let text = document_parser::extract_text(file_path)
        .with_context(|| format!("Failed to extract text from: {}", file_path.display()))?;

    let total_chars = text.len();
    if text.trim().is_empty() {
        anyhow::bail!(
            "No text content could be extracted from: {}",
            file_path.display()
        );
    }

    tracing::info!(
        "Extracted {} chars from '{}', chunking...",
        total_chars,
        file_path.display()
    );

    // 2. Chunk the text
    let chunks = chunk_text(&text, chunking_config);
    if chunks.is_empty() {
        anyhow::bail!("Text chunking produced no chunks");
    }

    tracing::info!(
        "Created {} chunks from '{}' (config: max_tokens={}, overlap={})",
        chunks.len(),
        file_path.display(),
        chunking_config.max_chunk_tokens,
        chunking_config.overlap_tokens
    );

    // 3. Find or create the collection
    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    let effective_source = source.unwrap_or(file_name);

    let collection = find_or_create_collection(
        db,
        collection_name,
        collection_description,
        Some(effective_source),
    )
    .await
    .context("Failed to find or create collection")?;

    // 4. If replacing, clear existing entries
    if replace_existing {
        let cleared = clear_collection_entries(db, &collection.id).await?;
        if cleared > 0 {
            tracing::info!(
                "Cleared {} existing entries from collection '{}'",
                cleared,
                collection_name
            );
        }
    }

    // 5. Store each chunk as a MemoryEntry
    let mut chunks_created = 0;
    for chunk in &chunks {
        let entry = MemoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            content: chunk.text.clone(),
            entry_type: entry_type.clone(),
            importance: importance.clamp(0.0, 1.0),
            created_at: Utc::now(),
            last_accessed: Utc::now(),
            access_count: 0,
            tags: vec![],
            source_message_ids: vec![],
            collection_id: Some(collection.id.clone()),
        };

        match memory.store(entry).await {
            Ok(()) => {
                chunks_created += 1;
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to store chunk {} of '{}': {}",
                    chunk.index,
                    file_path.display(),
                    e
                );
            }
        }
    }

    // 6. Update collection count
    update_collection_count(db, &collection.id).await?;

    tracing::info!(
        "Ingested {} chunks into collection '{}' (id: {})",
        chunks_created,
        collection_name,
        collection.id
    );

    Ok(IngestResult {
        collection_id: collection.id,
        collection_name: collection_name.to_string(),
        chunks_created,
        total_chars,
    })
}
