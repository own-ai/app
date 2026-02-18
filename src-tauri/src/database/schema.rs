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
            importance_score REAL DEFAULT 0.5,
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

    // Dynamic tools table (Rhai scripts)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tools (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            description TEXT NOT NULL,
            version TEXT NOT NULL DEFAULT '1.0.0',
            script_content TEXT NOT NULL,
            parameters TEXT NOT NULL DEFAULT '[]',
            status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active', 'deprecated', 'testing')),
            created_at DATETIME NOT NULL,
            last_used DATETIME,
            usage_count INTEGER DEFAULT 0,
            success_count INTEGER DEFAULT 0,
            failure_count INTEGER DEFAULT 0,
            parent_tool_id TEXT
        )
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create tools table")?;

    // Tool execution log
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tool_executions (
            id TEXT PRIMARY KEY,
            tool_id TEXT NOT NULL,
            timestamp DATETIME NOT NULL,
            success INTEGER NOT NULL DEFAULT 0,
            execution_time_ms INTEGER,
            error_message TEXT,
            input_params TEXT,
            output TEXT,
            FOREIGN KEY (tool_id) REFERENCES tools(id)
        )
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create tool_executions table")?;

    // Index for tool executions by tool_id
    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_tool_executions_tool_id
        ON tool_executions(tool_id)
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create tool_executions index")?;

    tracing::debug!("Database tables created successfully");

    Ok(())
}
