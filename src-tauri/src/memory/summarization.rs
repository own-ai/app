use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Row, Sqlite};

/// A session summary containing compressed conversation history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub start_message_id: String,
    pub end_message_id: String,
    pub summary_text: String,
    pub key_facts: Vec<String>,
    pub tools_mentioned: Vec<String>,
    pub topics: Vec<String>,
    pub timestamp: DateTime<Utc>,
    pub token_savings: usize,
}

/// Response structure from LLM when summarizing
#[derive(Debug, Deserialize)]
struct SummaryResponse {
    summary: String,
    key_facts: Vec<String>,
    #[serde(default)]
    tools_used: Vec<String>,
    #[serde(default)]
    topics: Vec<String>,
}

/// Message structure for summarization
#[derive(Debug, Clone)]
pub struct Message {
    pub id: String,
    pub role: String,
    pub content: String,
}

/// Summarization agent that uses LLM to create concise summaries
pub struct SummarizationAgent {
    db: Pool<Sqlite>,
}

impl SummarizationAgent {
    /// Create new summarization agent
    pub fn new(db: Pool<Sqlite>) -> Self {
        Self { db }
    }
    
    /// Initialize summaries table
    pub async fn init_table(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS summaries (
                id TEXT PRIMARY KEY,
                start_message_id TEXT NOT NULL,
                end_message_id TEXT NOT NULL,
                summary_text TEXT NOT NULL,
                key_facts TEXT NOT NULL,  -- JSON array
                tools_mentioned TEXT,      -- JSON array
                topics TEXT,               -- JSON array
                timestamp DATETIME NOT NULL,
                token_savings INTEGER,
                FOREIGN KEY (start_message_id) REFERENCES messages(id),
                FOREIGN KEY (end_message_id) REFERENCES messages(id)
            )
            "#,
        )
        .execute(&self.db)
        .await?;
        
        // Add summary_id column to messages if not exists
        // Note: SQLite doesn't support ALTER TABLE ADD COLUMN IF NOT EXISTS directly
        // We'll handle this gracefully
        let _ = sqlx::query(
            "ALTER TABLE messages ADD COLUMN summary_id TEXT REFERENCES summaries(id)"
        )
        .execute(&self.db)
        .await;
        
        // Create indices
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_summaries_timestamp ON summaries(timestamp DESC)"
        )
        .execute(&self.db)
        .await?;
        
        Ok(())
    }
    
    /// Summarize a list of messages
    /// For now, this creates a basic summary without LLM
    /// In Phase 3, we'll integrate with rig-core for proper LLM summarization
    pub async fn summarize_messages(&self, messages: &[Message]) -> Result<SessionSummary> {
        if messages.is_empty() {
            anyhow::bail!("Cannot summarize empty message list");
        }
        
        // Create basic summary for now
        let summary_text = self.create_basic_summary(messages);
        let key_facts = self.extract_basic_facts(messages);
        
        let summary = SessionSummary {
            id: uuid::Uuid::new_v4().to_string(),
            start_message_id: messages.first().unwrap().id.clone(),
            end_message_id: messages.last().unwrap().id.clone(),
            summary_text,
            key_facts,
            tools_mentioned: Vec::new(),
            topics: Vec::new(),
            timestamp: Utc::now(),
            token_savings: self.estimate_token_savings(messages),
        };
        
        tracing::info!(
            "Created summary for {} messages (saved ~{} tokens)",
            messages.len(),
            summary.token_savings
        );
        
        Ok(summary)
    }
    
    /// Save summary to database
    pub async fn save_summary(&self, summary: &SessionSummary) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO summaries 
            (id, start_message_id, end_message_id, summary_text, key_facts, 
             tools_mentioned, topics, timestamp, token_savings)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&summary.id)
        .bind(&summary.start_message_id)
        .bind(&summary.end_message_id)
        .bind(&summary.summary_text)
        .bind(serde_json::to_string(&summary.key_facts)?)
        .bind(serde_json::to_string(&summary.tools_mentioned)?)
        .bind(serde_json::to_string(&summary.topics)?)
        .bind(summary.timestamp)
        .bind(summary.token_savings as i32)
        .execute(&self.db)
        .await?;
        
        tracing::info!("Saved summary: {}", summary.id);
        Ok(())
    }
    
    /// Link messages to their summary
    pub async fn link_messages_to_summary(
        &self,
        message_ids: &[String],
        summary_id: &str,
    ) -> Result<()> {
        for msg_id in message_ids {
            sqlx::query("UPDATE messages SET summary_id = ? WHERE id = ?")
                .bind(summary_id)
                .bind(msg_id)
                .execute(&self.db)
                .await?;
        }
        
        tracing::debug!(
            "Linked {} messages to summary {}",
            message_ids.len(),
            summary_id
        );
        
        Ok(())
    }
    
    /// Get recent summaries
    pub async fn get_recent_summaries(&self, limit: usize) -> Result<Vec<SessionSummary>> {
        let rows = sqlx::query(
            r#"
            SELECT id, start_message_id, end_message_id, summary_text, 
                   key_facts, tools_mentioned, topics, timestamp, token_savings
            FROM summaries
            ORDER BY timestamp DESC
            LIMIT ?
            "#,
        )
        .bind(limit as i32)
        .fetch_all(&self.db)
        .await?;
        
        let summaries = rows
            .into_iter()
            .filter_map(|row| {
                Some(SessionSummary {
                    id: row.get("id"),
                    start_message_id: row.get("start_message_id"),
                    end_message_id: row.get("end_message_id"),
                    summary_text: row.get("summary_text"),
                    key_facts: serde_json::from_str(row.get("key_facts")).ok()?,
                    tools_mentioned: serde_json::from_str(row.get("tools_mentioned")).ok()?,
                    topics: serde_json::from_str(row.get("topics")).ok()?,
                    timestamp: row.get("timestamp"),
                    token_savings: row.get::<i32, _>("token_savings") as usize,
                })
            })
            .collect();
        
        Ok(summaries)
    }
    
    /// Create a basic summary without LLM (placeholder for Phase 3)
    fn create_basic_summary(&self, messages: &[Message]) -> String {
        let user_count = messages.iter().filter(|m| m.role == "user").count();
        let agent_count = messages.iter().filter(|m| m.role == "agent").count();
        
        format!(
            "Conversation with {} user messages and {} agent responses. \
             Topics discussed include various queries and responses.",
            user_count, agent_count
        )
    }
    
    /// Extract basic facts (placeholder for Phase 3 LLM extraction)
    fn extract_basic_facts(&self, messages: &[Message]) -> Vec<String> {
        // For now, just return message count as a fact
        vec![format!("Conversation contained {} messages", messages.len())]
    }
    
    /// Estimate token savings from summarization
    fn estimate_token_savings(&self, messages: &[Message]) -> usize {
        let original_tokens: usize = messages
            .iter()
            .map(|m| (m.content.len() + m.role.len()) / 4)
            .sum();
        
        // Assume summary is ~10% of original size
        let summary_tokens = original_tokens / 10;
        
        original_tokens.saturating_sub(summary_tokens)
    }
}
