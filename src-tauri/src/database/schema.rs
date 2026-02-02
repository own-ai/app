use anyhow::{Context, Result};
use sqlx::{Pool, Sqlite};

/// Create all necessary tables for an instance
pub async fn create_tables(pool: &Pool<Sqlite>) -> Result<()> {
    // Messages table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            role TEXT NOT NULL CHECK(role IN ('user', 'agent', 'system')),
            content TEXT NOT NULL,
            timestamp DATETIME NOT NULL,
            tokens_used INTEGER,
            metadata TEXT
        )
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create messages table")?;
    
    // Index for timestamp queries
    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_messages_timestamp 
        ON messages(timestamp)
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create messages timestamp index")?;
    
    // User profile table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS user_profile (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at DATETIME NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create user_profile table")?;
    
    tracing::debug!("Database tables created successfully");
    
    Ok(())
}
