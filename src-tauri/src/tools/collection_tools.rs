//! Agent tools for Knowledge Collection management.
//!
//! Provides rig Tools that allow agents to create, list, delete knowledge
//! collections, and ingest documents into them.

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Pool, Sqlite};
use std::path::PathBuf;

use crate::memory::chunking::ChunkingConfig;
use crate::memory::collections;
use crate::memory::ingest;
use crate::memory::SharedLongTermMemory;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct CollectionToolError(String);

// ---------------------------------------------------------------------------
// CreateKnowledgeCollectionTool
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateCollectionArgs {
    /// Name for the collection (lowercase with hyphens, e.g. "funding-guidelines").
    name: String,
    /// Description of what this collection contains.
    description: String,
    /// Optional source identifier (e.g. filename, URL).
    #[serde(default)]
    source: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CreateKnowledgeCollectionTool {
    #[serde(skip)]
    db: Option<Pool<Sqlite>>,
}

impl CreateKnowledgeCollectionTool {
    pub fn new(db: Pool<Sqlite>) -> Self {
        Self { db: Some(db) }
    }
}

impl Tool for CreateKnowledgeCollectionTool {
    const NAME: &'static str = "create_knowledge_collection";
    type Error = CollectionToolError;
    type Args = CreateCollectionArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "create_knowledge_collection".to_string(),
            description: "Create a new knowledge collection for organizing domain-specific \
                knowledge by topic. Collections group memory entries together, making it \
                easy to search within a topic and manage related knowledge as a unit. \
                Use this before ingesting documents or manually adding entries to a topic."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Collection name (lowercase with hyphens, e.g. 'funding-guidelines')"
                    },
                    "description": {
                        "type": "string",
                        "description": "Description of what this collection contains"
                    },
                    "source": {
                        "type": "string",
                        "description": "Optional source identifier (e.g. filename, URL)"
                    }
                },
                "required": ["name", "description"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| CollectionToolError("Database not initialized".to_string()))?;

        let collection = collections::create_collection(
            db,
            &args.name,
            &args.description,
            args.source.as_deref(),
        )
        .await
        .map_err(|e| CollectionToolError(format!("Failed to create collection: {}", e)))?;

        Ok(format!(
            "Knowledge collection '{}' created successfully (id: {}).\n\
             Description: {}\n\
             You can now add entries with add_memory(collection=\"{}\") or ingest documents with ingest_document.",
            collection.name, collection.id, collection.description, collection.name
        ))
    }
}

// ---------------------------------------------------------------------------
// ListKnowledgeCollectionsTool
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListCollectionsArgs {}

#[derive(Clone, Serialize, Deserialize)]
pub struct ListKnowledgeCollectionsTool {
    #[serde(skip)]
    db: Option<Pool<Sqlite>>,
}

impl ListKnowledgeCollectionsTool {
    pub fn new(db: Pool<Sqlite>) -> Self {
        Self { db: Some(db) }
    }
}

impl Tool for ListKnowledgeCollectionsTool {
    const NAME: &'static str = "list_knowledge_collections";
    type Error = CollectionToolError;
    type Args = ListCollectionsArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "list_knowledge_collections".to_string(),
            description: "List all knowledge collections. Shows name, description, \
                source, number of entries, and creation date for each collection."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| CollectionToolError("Database not initialized".to_string()))?;

        let collections = collections::list_collections(db)
            .await
            .map_err(|e| CollectionToolError(format!("Failed to list collections: {}", e)))?;

        if collections.is_empty() {
            return Ok("No knowledge collections found.".to_string());
        }

        let mut output = format!("Found {} knowledge collections:\n\n", collections.len());
        for (i, col) in collections.iter().enumerate() {
            let source = col.source.as_deref().unwrap_or("(no source)");
            let date = col.created_at.format("%Y-%m-%d");
            output.push_str(&format!(
                "{}. **{}** (id: {})\n   Description: {}\n   Source: {}\n   Entries: {} | Created: {}\n\n",
                i + 1,
                col.name,
                col.id,
                col.description,
                source,
                col.document_count,
                date,
            ));
        }

        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// DeleteKnowledgeCollectionTool
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DeleteCollectionArgs {
    /// Name of the collection to delete.
    name: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct DeleteKnowledgeCollectionTool {
    #[serde(skip)]
    db: Option<Pool<Sqlite>>,
}

impl DeleteKnowledgeCollectionTool {
    pub fn new(db: Pool<Sqlite>) -> Self {
        Self { db: Some(db) }
    }
}

impl Tool for DeleteKnowledgeCollectionTool {
    const NAME: &'static str = "delete_knowledge_collection";
    type Error = CollectionToolError;
    type Args = DeleteCollectionArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "delete_knowledge_collection".to_string(),
            description: "Delete a knowledge collection and all its associated memory entries. \
                This permanently removes all knowledge stored in the collection. \
                Use list_knowledge_collections first to find the collection name."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name of the collection to delete"
                    }
                },
                "required": ["name"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| CollectionToolError("Database not initialized".to_string()))?;

        let collection = collections::get_collection_by_name(db, &args.name)
            .await
            .map_err(|e| CollectionToolError(format!("Failed to find collection: {}", e)))?
            .ok_or_else(|| CollectionToolError(format!("Collection '{}' not found", args.name)))?;

        let entry_count = collection.document_count;

        collections::delete_collection(db, &collection.id)
            .await
            .map_err(|e| CollectionToolError(format!("Failed to delete collection: {}", e)))?;

        Ok(format!(
            "Knowledge collection '{}' deleted successfully.\n\
             Removed {} associated memory entries.",
            args.name, entry_count
        ))
    }
}

// ---------------------------------------------------------------------------
// IngestDocumentTool
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct IngestDocumentArgs {
    /// Relative path to the file in the workspace.
    file_path: String,
    /// Name for the knowledge collection to ingest into.
    collection_name: String,
    /// Description for the collection (used if creating a new one).
    #[serde(default = "default_collection_description")]
    collection_description: String,
    /// Maximum tokens per chunk (default: 400).
    #[serde(default = "default_chunk_tokens")]
    chunk_size: usize,
    /// Overlap tokens between chunks (default: 80).
    #[serde(default = "default_overlap_tokens")]
    overlap: usize,
    /// If true and collection exists, replace all existing entries (default: true).
    #[serde(default = "default_replace")]
    replace_existing: bool,
}

fn default_collection_description() -> String {
    String::new()
}
fn default_chunk_tokens() -> usize {
    400
}
fn default_overlap_tokens() -> usize {
    80
}
fn default_replace() -> bool {
    true
}

#[derive(Clone, Serialize, Deserialize)]
pub struct IngestDocumentTool {
    #[serde(skip)]
    db: Option<Pool<Sqlite>>,
    #[serde(skip)]
    memory: Option<SharedLongTermMemory>,
    #[serde(skip)]
    workspace_root: Option<PathBuf>,
}

impl IngestDocumentTool {
    pub fn new(db: Pool<Sqlite>, memory: SharedLongTermMemory, workspace_root: PathBuf) -> Self {
        Self {
            db: Some(db),
            memory: Some(memory),
            workspace_root: Some(workspace_root),
        }
    }
}

impl Tool for IngestDocumentTool {
    const NAME: &'static str = "ingest_document";
    type Error = CollectionToolError;
    type Args = IngestDocumentArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "ingest_document".to_string(),
            description: "Automatically read a document file, split it into chunks, and store \
                all chunks in a knowledge collection for semantic search. Supports PDF, DOCX, \
                Markdown, and plain text files. The file must be in the workspace directory.\n\n\
                This is the recommended way to import domain knowledge from documents. \
                After ingestion, use search_memory with the collection parameter to find \
                specific information within the document."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Relative path to the file in the workspace"
                    },
                    "collection_name": {
                        "type": "string",
                        "description": "Name for the knowledge collection (lowercase with hyphens)"
                    },
                    "collection_description": {
                        "type": "string",
                        "description": "Description of the collection (default: empty)"
                    },
                    "chunk_size": {
                        "type": "integer",
                        "description": "Maximum tokens per chunk (default: 400)"
                    },
                    "overlap": {
                        "type": "integer",
                        "description": "Overlap tokens between chunks (default: 80)"
                    },
                    "replace_existing": {
                        "type": "boolean",
                        "description": "If true and collection exists, replace all entries (default: true)"
                    }
                },
                "required": ["file_path", "collection_name"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| CollectionToolError("Database not initialized".to_string()))?;
        let memory_arc = self
            .memory
            .as_ref()
            .ok_or_else(|| CollectionToolError("Long-term memory not initialized".to_string()))?;
        let workspace = self
            .workspace_root
            .as_ref()
            .ok_or_else(|| CollectionToolError("Workspace root not initialized".to_string()))?;

        // Resolve file path within workspace (security: no absolute paths, no traversal)
        let path = std::path::Path::new(&args.file_path);
        if path.is_absolute() {
            return Err(CollectionToolError(
                "Absolute paths are not allowed. Use a relative path within the workspace."
                    .to_string(),
            ));
        }
        if path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(CollectionToolError(
                "Parent directory traversal (..) is not allowed".to_string(),
            ));
        }

        let full_path = workspace.join(path);
        if !full_path.exists() {
            return Err(CollectionToolError(format!(
                "File not found in workspace: {}",
                args.file_path
            )));
        }

        let chunking_config = ChunkingConfig {
            max_chunk_tokens: args.chunk_size,
            overlap_tokens: args.overlap,
            respect_paragraphs: true,
        };

        // Lock long-term memory for the duration of ingestion
        let mut mem = memory_arc.lock().await;

        let result = ingest::ingest_document(
            &full_path,
            &args.collection_name,
            &args.collection_description,
            Some(&args.file_path),
            &chunking_config,
            crate::memory::MemoryType::Fact,
            0.7,
            args.replace_existing,
            db,
            &mut mem,
        )
        .await
        .map_err(|e| CollectionToolError(format!("Document ingestion failed: {}", e)))?;

        Ok(format!(
            "Document '{}' ingested successfully into collection '{}'.\n\
             - Chunks created: {}\n\
             - Total characters: {}\n\
             - Collection ID: {}\n\n\
             You can now search this knowledge with: search_memory(query, collection=\"{}\")",
            args.file_path,
            result.collection_name,
            result.chunks_created,
            result.total_chars,
            result.collection_id,
            result.collection_name,
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_collection_tool_name() {
        assert_eq!(
            CreateKnowledgeCollectionTool::NAME,
            "create_knowledge_collection"
        );
    }

    #[test]
    fn test_list_collections_tool_name() {
        assert_eq!(
            ListKnowledgeCollectionsTool::NAME,
            "list_knowledge_collections"
        );
    }

    #[test]
    fn test_delete_collection_tool_name() {
        assert_eq!(
            DeleteKnowledgeCollectionTool::NAME,
            "delete_knowledge_collection"
        );
    }

    #[test]
    fn test_ingest_document_tool_name() {
        assert_eq!(IngestDocumentTool::NAME, "ingest_document");
    }

    #[tokio::test]
    async fn test_create_collection_definition() {
        let tool = CreateKnowledgeCollectionTool { db: None };
        let def = tool.definition("test".to_string()).await;
        assert_eq!(def.name, "create_knowledge_collection");
        assert!(def.description.contains("knowledge collection"));
    }

    #[tokio::test]
    async fn test_list_collections_definition() {
        let tool = ListKnowledgeCollectionsTool { db: None };
        let def = tool.definition("test".to_string()).await;
        assert_eq!(def.name, "list_knowledge_collections");
    }

    #[tokio::test]
    async fn test_delete_collection_definition() {
        let tool = DeleteKnowledgeCollectionTool { db: None };
        let def = tool.definition("test".to_string()).await;
        assert_eq!(def.name, "delete_knowledge_collection");
        assert!(def.description.contains("Delete"));
    }

    #[tokio::test]
    async fn test_ingest_document_definition() {
        let tool = IngestDocumentTool {
            db: None,
            memory: None,
            workspace_root: None,
        };
        let def = tool.definition("test".to_string()).await;
        assert_eq!(def.name, "ingest_document");
        assert!(def.description.contains("PDF"));
        assert!(def.description.contains("DOCX"));
    }

    #[tokio::test]
    async fn test_create_collection_no_db() {
        let tool = CreateKnowledgeCollectionTool { db: None };
        let result = tool
            .call(CreateCollectionArgs {
                name: "test".to_string(),
                description: "test".to_string(),
                source: None,
            })
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not initialized"));
    }

    #[tokio::test]
    async fn test_ingest_no_db() {
        let tool = IngestDocumentTool {
            db: None,
            memory: None,
            workspace_root: None,
        };
        let result = tool
            .call(IngestDocumentArgs {
                file_path: "test.txt".to_string(),
                collection_name: "test".to_string(),
                collection_description: String::new(),
                chunk_size: 400,
                overlap: 80,
                replace_existing: true,
            })
            .await;
        assert!(result.is_err());
    }
}
