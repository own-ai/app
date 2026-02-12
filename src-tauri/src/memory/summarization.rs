use anyhow::Result;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
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

/// Structured response extracted from LLM when summarizing conversations.
/// Used with rig Extractors for type-safe structured output.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SummaryResponse {
    /// Concise summary of the conversation
    pub summary: String,
    /// Key facts learned about the user or discussed topics
    pub key_facts: Vec<String>,
    /// Tools that were used or mentioned in the conversation
    #[serde(default)]
    pub tools_used: Vec<String>,
    /// Main topics discussed in the conversation
    #[serde(default)]
    pub topics: Vec<String>,
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

    /// Get total count of summaries in database
    pub async fn count_summaries(&self) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM summaries")
            .fetch_one(&self.db)
            .await?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create an in-memory SQLite database with the messages table
    async fn setup_test_db() -> Pool<Sqlite> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory database");

        // Create messages table (needed for foreign keys)
        sqlx::query(
            r#"
            CREATE TABLE messages (
                id TEXT PRIMARY KEY,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp DATETIME NOT NULL,
                metadata TEXT,
                summary_id TEXT
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create messages table");

        pool
    }

    fn create_test_summary(id: &str) -> SessionSummary {
        SessionSummary {
            id: id.to_string(),
            start_message_id: "msg_start".to_string(),
            end_message_id: "msg_end".to_string(),
            summary_text: "Test summary of a conversation about Rust programming.".to_string(),
            key_facts: vec![
                "User is learning Rust".to_string(),
                "Discussed ownership and borrowing".to_string(),
            ],
            tools_mentioned: vec!["read_file".to_string()],
            topics: vec!["Rust".to_string(), "Programming".to_string()],
            timestamp: Utc::now(),
            token_savings: 500,
        }
    }

    #[tokio::test]
    async fn test_init_table() {
        let db = setup_test_db().await;
        let agent = SummarizationAgent::new(db);

        let result = agent.init_table().await;
        assert!(result.is_ok(), "init_table should succeed");
    }

    #[tokio::test]
    async fn test_save_and_retrieve_summary() {
        let db = setup_test_db().await;
        let agent = SummarizationAgent::new(db);
        agent.init_table().await.unwrap();

        // Insert referenced messages first (for foreign key)
        sqlx::query("INSERT INTO messages (id, role, content, timestamp) VALUES (?, ?, ?, ?)")
            .bind("msg_start")
            .bind("user")
            .bind("Hello")
            .bind(Utc::now())
            .execute(&agent.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO messages (id, role, content, timestamp) VALUES (?, ?, ?, ?)")
            .bind("msg_end")
            .bind("agent")
            .bind("Hi there!")
            .bind(Utc::now())
            .execute(&agent.db)
            .await
            .unwrap();

        // Save summary
        let summary = create_test_summary("summary_1");
        agent.save_summary(&summary).await.unwrap();

        // Retrieve
        let summaries = agent.get_recent_summaries(10).await.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, "summary_1");
        assert_eq!(
            summaries[0].summary_text,
            "Test summary of a conversation about Rust programming."
        );
        assert_eq!(summaries[0].key_facts.len(), 2);
        assert_eq!(summaries[0].tools_mentioned, vec!["read_file"]);
        assert_eq!(summaries[0].topics, vec!["Rust", "Programming"]);
        assert_eq!(summaries[0].token_savings, 500);
    }

    #[tokio::test]
    async fn test_count_summaries() {
        let db = setup_test_db().await;
        let agent = SummarizationAgent::new(db);
        agent.init_table().await.unwrap();

        // Insert referenced messages
        for id in ["msg_start", "msg_end", "msg_start2", "msg_end2"] {
            sqlx::query("INSERT INTO messages (id, role, content, timestamp) VALUES (?, ?, ?, ?)")
                .bind(id)
                .bind("user")
                .bind("content")
                .bind(Utc::now())
                .execute(&agent.db)
                .await
                .unwrap();
        }

        assert_eq!(agent.count_summaries().await.unwrap(), 0);

        agent
            .save_summary(&create_test_summary("s1"))
            .await
            .unwrap();
        assert_eq!(agent.count_summaries().await.unwrap(), 1);

        let mut s2 = create_test_summary("s2");
        s2.start_message_id = "msg_start2".to_string();
        s2.end_message_id = "msg_end2".to_string();
        agent.save_summary(&s2).await.unwrap();
        assert_eq!(agent.count_summaries().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_link_messages_to_summary() {
        let db = setup_test_db().await;
        let agent = SummarizationAgent::new(db);
        agent.init_table().await.unwrap();

        // Create messages
        let msg_ids: Vec<String> = (0..3).map(|i| format!("msg_{}", i)).collect();
        for id in &msg_ids {
            sqlx::query("INSERT INTO messages (id, role, content, timestamp) VALUES (?, ?, ?, ?)")
                .bind(id)
                .bind("user")
                .bind("test content")
                .bind(Utc::now())
                .execute(&agent.db)
                .await
                .unwrap();
        }

        // Save summary
        let mut summary = create_test_summary("sum_1");
        summary.start_message_id = msg_ids[0].clone();
        summary.end_message_id = msg_ids[2].clone();
        agent.save_summary(&summary).await.unwrap();

        // Link messages
        agent
            .link_messages_to_summary(&msg_ids, "sum_1")
            .await
            .unwrap();

        // Verify links
        let linked: Vec<(String,)> =
            sqlx::query_as("SELECT summary_id FROM messages WHERE summary_id IS NOT NULL")
                .fetch_all(&agent.db)
                .await
                .unwrap();

        assert_eq!(linked.len(), 3);
        for (sid,) in &linked {
            assert_eq!(sid, "sum_1");
        }
    }

    #[tokio::test]
    async fn test_get_recent_summaries_ordering() {
        let db = setup_test_db().await;
        let agent = SummarizationAgent::new(db);
        agent.init_table().await.unwrap();

        // Create messages for foreign keys
        for i in 0..6 {
            sqlx::query("INSERT INTO messages (id, role, content, timestamp) VALUES (?, ?, ?, ?)")
                .bind(format!("m{}", i))
                .bind("user")
                .bind("content")
                .bind(Utc::now())
                .execute(&agent.db)
                .await
                .unwrap();
        }

        // Save 3 summaries
        for i in 0..3 {
            let mut s = create_test_summary(&format!("s{}", i));
            s.start_message_id = format!("m{}", i * 2);
            s.end_message_id = format!("m{}", i * 2 + 1);
            s.summary_text = format!("Summary {}", i);
            // Small delay to ensure different timestamps
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            agent.save_summary(&s).await.unwrap();
        }

        // Get with limit 2 -> should return the 2 most recent
        let summaries = agent.get_recent_summaries(2).await.unwrap();
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].summary_text, "Summary 2"); // Most recent first
        assert_eq!(summaries[1].summary_text, "Summary 1");
    }

    #[tokio::test]
    async fn test_summarize_messages_basic() {
        let db = setup_test_db().await;
        let agent = SummarizationAgent::new(db);

        let messages = vec![
            Message {
                id: "m1".to_string(),
                role: "user".to_string(),
                content: "Hello, I need help with Rust.".to_string(),
            },
            Message {
                id: "m2".to_string(),
                role: "agent".to_string(),
                content: "Sure, what do you need help with?".to_string(),
            },
            Message {
                id: "m3".to_string(),
                role: "user".to_string(),
                content: "How does ownership work?".to_string(),
            },
        ];

        let summary = agent.summarize_messages(&messages).await.unwrap();

        assert_eq!(summary.start_message_id, "m1");
        assert_eq!(summary.end_message_id, "m3");
        assert!(!summary.summary_text.is_empty());
        assert!(summary.token_savings > 0);
    }

    #[tokio::test]
    async fn test_summarize_empty_messages_fails() {
        let db = setup_test_db().await;
        let agent = SummarizationAgent::new(db);

        let result = agent.summarize_messages(&[]).await;
        assert!(result.is_err());
    }
}
