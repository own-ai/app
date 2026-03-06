use anyhow::Result;
use chrono::{DateTime, Local, Utc};

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

        // 1. Temporal context: current date/time in user's local timezone
        let now = Utc::now();
        let local_now = Local::now();
        context_parts.push(format!(
            "## Current Date and Time\n{}\n\n",
            local_now.format("%A, %Y-%m-%d %H:%M %:z")
        ));

        // Check for significant time gap since last message in working memory
        let wm_messages = self.working_memory.get_context();
        if let Some(last_msg) = wm_messages.last() {
            let gap = now.signed_duration_since(last_msg.timestamp);
            if let Some(gap_description) = Self::format_time_gap(gap, &last_msg.timestamp) {
                context_parts.push(format!(
                    "## Time Since Last Conversation\n{}\n\n",
                    gap_description
                ));
            }
        }

        // 2. Active TODO list (if any)
        if let Some(ref todo_list) = self.todo_list {
            let list_guard = todo_list.read().await;
            if let Some(ref list) = *list_guard {
                context_parts.push("## Active TODO List:\n".to_string());
                context_parts.push(list.to_markdown());
                context_parts.push("\n".to_string());
            }
        }

        // 3. Long-term memories (semantically relevant)
        let memories = {
            let mut ltm = self.long_term_memory.lock().await;
            ltm.recall(user_query, 10, 0.5).await?
        };

        if !memories.is_empty() {
            context_parts.push("## Relevant Context:\n".to_string());
            for (similarity, memory) in memories {
                context_parts.push(format!(
                    "- {} (Type: {:?}, Importance: {:.2}, similarity: {:.3})\n",
                    memory.content, memory.entry_type, memory.importance, similarity
                ));
            }
            context_parts.push("\n".to_string());
        }

        // 4. Recent summaries (3 most recent by timestamp)
        let recent_summaries = self.get_recent_summaries(3).await?;
        let recent_ids: Vec<String> = recent_summaries.iter().map(|s| s.id.clone()).collect();

        if !recent_summaries.is_empty() {
            context_parts.push("## Recent Session Summaries:\n".to_string());
            for summary in &recent_summaries {
                let date = summary.timestamp.format("%Y-%m-%d");
                context_parts.push(format!("- [{}] {}\n", date, summary.summary_text));
                if !summary.key_facts.is_empty() {
                    context_parts.push(format!("  Facts: {}\n", summary.key_facts.join(", ")));
                }
            }
            context_parts.push("\n".to_string());
        }

        // 5. Semantically relevant older summary (if not already in recent 3)
        {
            let ltm = self.long_term_memory.lock().await;
            match ltm.embed_text(user_query) {
                Ok(query_embedding) => {
                    drop(ltm); // Release lock before DB query
                    match self
                        .summarization_agent
                        .search_similar_summaries(&query_embedding, 1, 0.6)
                        .await
                    {
                        Ok(results) => {
                            if let Some((similarity, summary)) = results.into_iter().next() {
                                if !recent_ids.contains(&summary.id) {
                                    let date = summary.timestamp.format("%Y-%m-%d");
                                    context_parts
                                        .push("## Relevant Earlier Conversation:\n".to_string());
                                    context_parts.push(format!(
                                        "- [{}] {} (relevance: {:.0}%)\n",
                                        date,
                                        summary.summary_text,
                                        similarity * 100.0
                                    ));
                                    if !summary.key_facts.is_empty() {
                                        context_parts.push(format!(
                                            "  Facts: {}\n",
                                            summary.key_facts.join(", ")
                                        ));
                                    }
                                    context_parts.push("\n".to_string());
                                }
                            }
                        }
                        Err(e) => {
                            tracing::debug!("Semantic summary search skipped: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!("Failed to embed query for summary search: {}", e);
                }
            }
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

    /// Get mutable summarization agent reference (e.g. to set the extractor)
    pub fn summarization_agent_mut(&mut self) -> &mut SummarizationAgent {
        &mut self.summarization_agent
    }

    /// Format a time gap into a human-readable description for the LLM.
    /// Returns `None` if the gap is less than 1 hour (not significant enough to mention).
    fn format_time_gap(gap: chrono::TimeDelta, last_timestamp: &DateTime<Utc>) -> Option<String> {
        let total_minutes = gap.num_minutes();
        let total_hours = gap.num_hours();
        let total_days = gap.num_days();

        if total_minutes < 60 {
            // Less than 1 hour -- not significant, no mention needed
            return None;
        }

        let last_local = last_timestamp.with_timezone(&Local);
        let last_date = last_local.format("%A, %Y-%m-%d at %H:%M");

        let gap_text = if total_days >= 30 {
            let months = total_days / 30;
            if months == 1 {
                "about 1 month ago".to_string()
            } else {
                format!("about {} months ago", months)
            }
        } else if total_days >= 7 {
            let weeks = total_days / 7;
            if weeks == 1 {
                "about 1 week ago".to_string()
            } else {
                format!("about {} weeks ago", weeks)
            }
        } else if total_days >= 1 {
            if total_days == 1 {
                "yesterday".to_string()
            } else {
                format!("{} days ago", total_days)
            }
        } else {
            // 1-23 hours
            if total_hours == 1 {
                "about 1 hour ago".to_string()
            } else {
                format!("about {} hours ago", total_hours)
            }
        };

        Some(format!(
            "The last message in this conversation was {} ({}).\n\
             The user is returning after a break. Acknowledge the time gap naturally \
             if appropriate, but do not overemphasize it.",
            gap_text, last_date
        ))
    }

    /// Format a time gap between two consecutive messages as a short marker.
    /// Returns `None` if the gap is less than 4 hours (not significant for history).
    /// These markers are inserted into the chat history so the LLM can see
    /// temporal breaks between messages.
    pub fn format_history_time_marker(gap: chrono::TimeDelta) -> Option<String> {
        let total_hours = gap.num_hours();
        let total_days = gap.num_days();

        if total_hours < 4 {
            return None;
        }

        let marker = if total_days >= 30 {
            let months = total_days / 30;
            if months == 1 {
                "about 1 month later".to_string()
            } else {
                format!("about {} months later", months)
            }
        } else if total_days >= 7 {
            let weeks = total_days / 7;
            if weeks == 1 {
                "about 1 week later".to_string()
            } else {
                format!("about {} weeks later", weeks)
            }
        } else if total_days >= 1 {
            if total_days == 1 {
                "next day".to_string()
            } else {
                format!("{} days later", total_days)
            }
        } else {
            format!("{} hours later", total_hours)
        };

        Some(format!("[--- {} ---]", marker))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    #[test]
    fn test_format_time_gap_under_1_hour_returns_none() {
        let now = Utc::now();
        let gap = TimeDelta::minutes(30);
        assert!(ContextBuilder::format_time_gap(gap, &now).is_none());
    }

    #[test]
    fn test_format_time_gap_under_1_minute_returns_none() {
        let now = Utc::now();
        let gap = TimeDelta::seconds(45);
        assert!(ContextBuilder::format_time_gap(gap, &now).is_none());
    }

    #[test]
    fn test_format_time_gap_1_hour() {
        let now = Utc::now();
        let gap = TimeDelta::hours(1);
        let result = ContextBuilder::format_time_gap(gap, &now).unwrap();
        assert!(result.contains("about 1 hour ago"));
        assert!(result.contains("returning after a break"));
    }

    #[test]
    fn test_format_time_gap_several_hours() {
        let now = Utc::now();
        let gap = TimeDelta::hours(5);
        let result = ContextBuilder::format_time_gap(gap, &now).unwrap();
        assert!(result.contains("about 5 hours ago"));
    }

    #[test]
    fn test_format_time_gap_yesterday() {
        let now = Utc::now();
        let gap = TimeDelta::days(1);
        let result = ContextBuilder::format_time_gap(gap, &now).unwrap();
        assert!(result.contains("yesterday"));
    }

    #[test]
    fn test_format_time_gap_several_days() {
        let now = Utc::now();
        let gap = TimeDelta::days(3);
        let result = ContextBuilder::format_time_gap(gap, &now).unwrap();
        assert!(result.contains("3 days ago"));
    }

    #[test]
    fn test_format_time_gap_1_week() {
        let now = Utc::now();
        let gap = TimeDelta::weeks(1);
        let result = ContextBuilder::format_time_gap(gap, &now).unwrap();
        assert!(result.contains("about 1 week ago"));
    }

    #[test]
    fn test_format_time_gap_several_weeks() {
        let now = Utc::now();
        let gap = TimeDelta::weeks(3);
        let result = ContextBuilder::format_time_gap(gap, &now).unwrap();
        assert!(result.contains("about 3 weeks ago"));
    }

    #[test]
    fn test_format_time_gap_1_month() {
        let now = Utc::now();
        let gap = TimeDelta::days(35);
        let result = ContextBuilder::format_time_gap(gap, &now).unwrap();
        assert!(result.contains("about 1 month ago"));
    }

    #[test]
    fn test_format_time_gap_several_months() {
        let now = Utc::now();
        let gap = TimeDelta::days(90);
        let result = ContextBuilder::format_time_gap(gap, &now).unwrap();
        assert!(result.contains("about 3 months ago"));
    }

    #[test]
    fn test_format_history_time_marker_under_4_hours_returns_none() {
        let gap = TimeDelta::hours(3);
        assert!(ContextBuilder::format_history_time_marker(gap).is_none());
    }

    #[test]
    fn test_format_history_time_marker_4_hours() {
        let gap = TimeDelta::hours(4);
        let result = ContextBuilder::format_history_time_marker(gap).unwrap();
        assert_eq!(result, "[--- 4 hours later ---]");
    }

    #[test]
    fn test_format_history_time_marker_next_day() {
        let gap = TimeDelta::days(1);
        let result = ContextBuilder::format_history_time_marker(gap).unwrap();
        assert_eq!(result, "[--- next day ---]");
    }

    #[test]
    fn test_format_history_time_marker_several_days() {
        let gap = TimeDelta::days(5);
        let result = ContextBuilder::format_history_time_marker(gap).unwrap();
        assert_eq!(result, "[--- 5 days later ---]");
    }

    #[test]
    fn test_format_history_time_marker_1_week() {
        let gap = TimeDelta::weeks(1);
        let result = ContextBuilder::format_history_time_marker(gap).unwrap();
        assert_eq!(result, "[--- about 1 week later ---]");
    }

    #[test]
    fn test_format_history_time_marker_1_month() {
        let gap = TimeDelta::days(35);
        let result = ContextBuilder::format_history_time_marker(gap).unwrap();
        assert_eq!(result, "[--- about 1 month later ---]");
    }
}
