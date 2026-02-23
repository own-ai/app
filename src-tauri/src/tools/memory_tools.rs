//! Memory tools for agent access to the long-term vector store.
//!
//! Provides three rig Tools that allow agents (main and sub-agents) to
//! interact with the long-term memory system:
//! - `SearchMemoryTool`: Semantic search over stored memories
//! - `AddMemoryTool`: Store new facts/preferences/skills in long-term memory
//! - `DeleteMemoryTool`: Remove memory entries by ID

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::memory::fact_extraction::parse_memory_type;
use crate::memory::{MemoryEntry, SharedLongTermMemory};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct MemoryToolError(String);

// ---------------------------------------------------------------------------
// SearchMemoryTool
// ---------------------------------------------------------------------------

/// Arguments for semantic memory search.
#[derive(Debug, Deserialize)]
pub struct SearchMemoryArgs {
    /// The search query (used for semantic similarity matching).
    query: String,
    /// Maximum number of results to return (default: 10).
    #[serde(default = "default_limit")]
    limit: usize,
    /// Minimum importance threshold (0.0-1.0, default: 0.0).
    #[serde(default)]
    min_importance: f32,
}

fn default_limit() -> usize {
    10
}

/// rig Tool for semantic search in long-term memory.
#[derive(Clone, Serialize, Deserialize)]
pub struct SearchMemoryTool {
    #[serde(skip)]
    memory: Option<SharedLongTermMemory>,
}

impl SearchMemoryTool {
    pub fn new(memory: SharedLongTermMemory) -> Self {
        Self {
            memory: Some(memory),
        }
    }
}

impl Tool for SearchMemoryTool {
    const NAME: &'static str = "search_memory";
    type Error = MemoryToolError;
    type Args = SearchMemoryArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "search_memory".to_string(),
            description: "Search long-term memory using semantic similarity. \
                Returns stored facts, preferences, skills, and context that are \
                semantically related to the query. Use this to recall information \
                from previous conversations or stored knowledge."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query for semantic matching"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results (default: 10)"
                    },
                    "min_importance": {
                        "type": "number",
                        "description": "Minimum importance threshold 0.0-1.0 (default: 0.0)"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let memory = self
            .memory
            .as_ref()
            .ok_or_else(|| MemoryToolError("Long-term memory not initialized".to_string()))?;

        let mut mem = memory.lock().await;
        let results = mem
            .recall(&args.query, args.limit, args.min_importance)
            .await
            .map_err(|e| MemoryToolError(format!("Memory search failed: {}", e)))?;

        if results.is_empty() {
            return Ok("No matching memories found.".to_string());
        }

        let mut output = format!("Found {} matching memories:\n\n", results.len());
        for (i, entry) in results.iter().enumerate() {
            output.push_str(&format!(
                "{}. [{}] (type: {:?}, importance: {:.2})\n   {}\n\n",
                i + 1,
                entry.id,
                entry.entry_type,
                entry.importance,
                entry.content,
            ));
        }

        tracing::info!(
            "Memory search for '{}' returned {} results",
            args.query,
            results.len()
        );

        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// AddMemoryTool
// ---------------------------------------------------------------------------

/// Arguments for adding a memory entry.
#[derive(Debug, Deserialize)]
pub struct AddMemoryArgs {
    /// The content to remember (a concise, self-contained fact or piece of information).
    content: String,
    /// Type of memory: "fact", "preference", "skill", "context", or "tool_usage".
    #[serde(default = "default_entry_type")]
    entry_type: String,
    /// Importance score from 0.0 to 1.0 (default: 0.7).
    #[serde(default = "default_importance")]
    importance: f32,
}

fn default_entry_type() -> String {
    "fact".to_string()
}

fn default_importance() -> f32 {
    0.7
}

/// rig Tool for storing new entries in long-term memory.
#[derive(Clone, Serialize, Deserialize)]
pub struct AddMemoryTool {
    #[serde(skip)]
    memory: Option<SharedLongTermMemory>,
}

impl AddMemoryTool {
    pub fn new(memory: SharedLongTermMemory) -> Self {
        Self {
            memory: Some(memory),
        }
    }
}

impl Tool for AddMemoryTool {
    const NAME: &'static str = "add_memory";
    type Error = MemoryToolError;
    type Args = AddMemoryArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "add_memory".to_string(),
            description: "Store a new entry in long-term memory. Use this to remember \
                important facts, user preferences, skills, or context that should be \
                available in future conversations. Each entry should be concise and \
                self-contained."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "The content to remember (concise, self-contained)"
                    },
                    "entry_type": {
                        "type": "string",
                        "enum": ["fact", "preference", "skill", "context", "tool_usage"],
                        "description": "Type of memory (default: 'fact')"
                    },
                    "importance": {
                        "type": "number",
                        "description": "Importance score 0.0-1.0 (default: 0.7)"
                    }
                },
                "required": ["content"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let memory = self
            .memory
            .as_ref()
            .ok_or_else(|| MemoryToolError("Long-term memory not initialized".to_string()))?;

        let memory_type = parse_memory_type(&args.entry_type);
        let importance = args.importance.clamp(0.0, 1.0);

        let entry = MemoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            content: args.content.clone(),
            entry_type: memory_type.clone(),
            importance,
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            access_count: 0,
            tags: Vec::new(),
            source_message_ids: Vec::new(),
        };

        let entry_id = entry.id.clone();

        let mut mem = memory.lock().await;
        mem.store(entry)
            .await
            .map_err(|e| MemoryToolError(format!("Failed to store memory: {}", e)))?;

        tracing::info!(
            "Agent stored memory '{}' (type: {:?}, importance: {:.2})",
            entry_id,
            memory_type,
            importance
        );

        Ok(format!(
            "Memory stored successfully (id: {}, type: {:?}, importance: {:.2}).\n\
             Content: {}",
            entry_id, memory_type, importance, args.content
        ))
    }
}

// ---------------------------------------------------------------------------
// DeleteMemoryTool
// ---------------------------------------------------------------------------

/// Arguments for deleting a memory entry.
#[derive(Debug, Deserialize)]
pub struct DeleteMemoryArgs {
    /// The ID of the memory entry to delete.
    entry_id: String,
}

/// rig Tool for removing entries from long-term memory.
#[derive(Clone, Serialize, Deserialize)]
pub struct DeleteMemoryTool {
    #[serde(skip)]
    memory: Option<SharedLongTermMemory>,
}

impl DeleteMemoryTool {
    pub fn new(memory: SharedLongTermMemory) -> Self {
        Self {
            memory: Some(memory),
        }
    }
}

impl Tool for DeleteMemoryTool {
    const NAME: &'static str = "delete_memory";
    type Error = MemoryToolError;
    type Args = DeleteMemoryArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "delete_memory".to_string(),
            description: "Delete a memory entry from long-term memory by its ID. \
                Use search_memory first to find the entry ID, then delete if the \
                information is outdated or incorrect."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "entry_id": {
                        "type": "string",
                        "description": "The ID of the memory entry to delete"
                    }
                },
                "required": ["entry_id"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let memory = self
            .memory
            .as_ref()
            .ok_or_else(|| MemoryToolError("Long-term memory not initialized".to_string()))?;

        let mem = memory.lock().await;
        mem.delete(&args.entry_id)
            .await
            .map_err(|e| MemoryToolError(format!("Failed to delete memory: {}", e)))?;

        tracing::info!("Agent deleted memory entry '{}'", args.entry_id);

        Ok(format!(
            "Memory entry '{}' deleted successfully.",
            args.entry_id
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
    fn test_search_memory_tool_name() {
        assert_eq!(SearchMemoryTool::NAME, "search_memory");
    }

    #[test]
    fn test_add_memory_tool_name() {
        assert_eq!(AddMemoryTool::NAME, "add_memory");
    }

    #[test]
    fn test_delete_memory_tool_name() {
        assert_eq!(DeleteMemoryTool::NAME, "delete_memory");
    }

    #[tokio::test]
    async fn test_search_memory_no_init() {
        let tool = SearchMemoryTool { memory: None };

        let result = tool
            .call(SearchMemoryArgs {
                query: "test".to_string(),
                limit: 5,
                min_importance: 0.0,
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not initialized"));
    }

    #[tokio::test]
    async fn test_add_memory_no_init() {
        let tool = AddMemoryTool { memory: None };

        let result = tool
            .call(AddMemoryArgs {
                content: "test".to_string(),
                entry_type: "fact".to_string(),
                importance: 0.5,
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not initialized"));
    }

    #[tokio::test]
    async fn test_delete_memory_no_init() {
        let tool = DeleteMemoryTool { memory: None };

        let result = tool
            .call(DeleteMemoryArgs {
                entry_id: "test-id".to_string(),
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not initialized"));
    }

    #[tokio::test]
    async fn test_search_memory_definition() {
        let tool = SearchMemoryTool { memory: None };

        let def = tool.definition("test".to_string()).await;
        assert_eq!(def.name, "search_memory");
        assert!(def.description.contains("semantic"));
    }

    #[tokio::test]
    async fn test_add_memory_definition() {
        let tool = AddMemoryTool { memory: None };

        let def = tool.definition("test".to_string()).await;
        assert_eq!(def.name, "add_memory");
        assert!(def.description.contains("long-term memory"));
    }

    #[tokio::test]
    async fn test_delete_memory_definition() {
        let tool = DeleteMemoryTool { memory: None };

        let def = tool.definition("test".to_string()).await;
        assert_eq!(def.name, "delete_memory");
        assert!(def.description.contains("Delete"));
    }
}
