use anyhow::Result;

use crate::tools::planning::SharedTodoList;

use super::{SessionSummary, SharedLongTermMemory, SummarizationAgent, WorkingMemory};

/// Context builder combines all memory layers into a coherent context
pub struct ContextBuilder {
    working_memory: WorkingMemory,
    long_term_memory: SharedLongTermMemory,
    summarization_agent: SummarizationAgent,
    todo_list: Option<SharedTodoList>,
}

impl ContextBuilder {
    /// Create new context builder
    pub fn new(
        working_memory: WorkingMemory,
        long_term_memory: SharedLongTermMemory,
        summarization_agent: SummarizationAgent,
    ) -> Self {
        Self {
            working_memory,
            long_term_memory,
            summarization_agent,
            todo_list: None,
        }
    }

    /// Set the shared TODO list for context injection
    pub fn set_todo_list(&mut self, todo_list: SharedTodoList) {
        self.todo_list = Some(todo_list);
    }

    /// Build complete context for a user query
    pub async fn build_context(&self, user_query: &str) -> Result<String> {
        let mut context_parts = Vec::new();

        // 0. Active TODO list (if any)
        if let Some(ref todo_list) = self.todo_list {
            let list_guard = todo_list.read().await;
            if let Some(ref list) = *list_guard {
                context_parts.push("## Active TODO List:\n".to_string());
                context_parts.push(list.to_markdown());
                context_parts.push("\n".to_string());
            }
        }

        // 1. Long-term memories (semantically relevant)
        let memories = {
            let mut ltm = self.long_term_memory.lock().await;
            ltm.recall(user_query, 10, 0.5).await?
        };

        if !memories.is_empty() {
            context_parts.push("## Relevant Context:\n".to_string());
            for memory in memories {
                context_parts.push(format!(
                    "- {} (Type: {:?}, Importance: {:.2})\n",
                    memory.content, memory.entry_type, memory.importance
                ));
            }
            context_parts.push("\n".to_string());
        }

        // 2. Recent summaries (if relevant)
        let recent_summaries = self.get_recent_summaries(3).await?;
        if !recent_summaries.is_empty() {
            context_parts.push("## Recent Session Summaries:\n".to_string());
            for summary in recent_summaries {
                context_parts.push(format!("- {}\n", summary.summary_text));
                if !summary.key_facts.is_empty() {
                    context_parts.push(format!("  Facts: {}\n", summary.key_facts.join(", ")));
                }
            }
            context_parts.push("\n".to_string());
        }

        // Note: Working memory messages are sent separately via with_history()
        // in agent/mod.rs - they should not be duplicated in the context string.

        Ok(context_parts.join(""))
    }

    /// Get recent summaries from database
    async fn get_recent_summaries(&self, limit: usize) -> Result<Vec<SessionSummary>> {
        self.summarization_agent.get_recent_summaries(limit).await
    }

    /// Get working memory reference
    pub fn working_memory(&self) -> &WorkingMemory {
        &self.working_memory
    }

    /// Get mutable working memory reference
    pub fn working_memory_mut(&mut self) -> &mut WorkingMemory {
        &mut self.working_memory
    }

    /// Get shared long-term memory reference (for tools and commands)
    pub fn long_term_memory(&self) -> &SharedLongTermMemory {
        &self.long_term_memory
    }

    /// Get summarization agent reference
    pub fn summarization_agent(&self) -> &SummarizationAgent {
        &self.summarization_agent
    }
}
