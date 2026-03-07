use anyhow::{Context, Result};
use sqlx::{Pool, Row, Sqlite};

use crate::memory::working_memory::{Message, MessageMetadata};

use super::OwnAIAgent;

impl OwnAIAgent {
    /// Helper: Load recent messages from database for working memory initialization.
    /// Includes metadata column to restore tool call/result information.
    pub(super) async fn load_recent_messages_from_db(
        db: &Pool<Sqlite>,
        limit: i32,
    ) -> Result<Vec<Message>> {
        let rows = sqlx::query(
            r#"
            SELECT * FROM (
                SELECT id, role, content, timestamp, importance_score, metadata
                FROM messages
                ORDER BY timestamp DESC
                LIMIT ?
            ) ORDER BY timestamp ASC
            "#,
        )
        .bind(limit)
        .fetch_all(db)
        .await
        .context("Failed to load recent messages")?;

        let messages: Vec<Message> = rows
            .into_iter()
            .map(|row| {
                let metadata_str: Option<String> = row.get("metadata");
                let metadata =
                    metadata_str.and_then(|s| serde_json::from_str::<MessageMetadata>(&s).ok());
                Message {
                    id: row.get("id"),
                    role: row.get("role"),
                    content: row.get("content"),
                    timestamp: row.get("timestamp"),
                    importance_score: row.get("importance_score"),
                    metadata,
                }
            })
            .collect();

        tracing::debug!(
            "Loaded {} messages from database for working memory",
            messages.len()
        );
        Ok(messages)
    }

    /// Helper: Save a Message to the database, including metadata as JSON.
    pub(super) async fn save_message_to_db(&self, msg: &Message) -> Result<()> {
        let metadata_json = msg
            .metadata
            .as_ref()
            .and_then(|m| serde_json::to_string(m).ok());

        sqlx::query(
            "INSERT INTO messages (id, role, content, timestamp, metadata) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&msg.id)
        .bind(&msg.role)
        .bind(&msg.content)
        .bind(msg.timestamp)
        .bind(metadata_json)
        .execute(&self.db)
        .await
        .context("Failed to save message")?;

        Ok(())
    }

    /// Helper: Update tokens_used on a message in the database.
    /// Used after streaming to persist LLM token usage (input_tokens on user
    /// message, output_tokens on agent message). Logs errors but does not fail.
    pub(super) async fn update_tokens_used(db: &Pool<Sqlite>, message_id: &str, tokens: i64) {
        if let Err(e) = sqlx::query("UPDATE messages SET tokens_used = ? WHERE id = ?")
            .bind(tokens)
            .bind(message_id)
            .execute(db)
            .await
        {
            tracing::warn!(
                "Failed to update tokens_used for message {}: {}",
                message_id,
                e
            );
        }
    }

    /// Helper: Update importance_score on a message in the database.
    /// Called from fact extraction background task with the max importance
    /// of all extracted facts. Logs errors but does not fail.
    pub(super) async fn update_importance_score(db: &Pool<Sqlite>, message_id: &str, score: f32) {
        if let Err(e) = sqlx::query("UPDATE messages SET importance_score = ? WHERE id = ?")
            .bind(score)
            .bind(message_id)
            .execute(db)
            .await
        {
            tracing::warn!(
                "Failed to update importance_score for message {}: {}",
                message_id,
                e
            );
        }
    }
}
