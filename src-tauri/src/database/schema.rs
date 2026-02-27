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

    // Canvas programs table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS programs (
            id TEXT PRIMARY KEY,
            instance_id TEXT NOT NULL,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            version TEXT NOT NULL DEFAULT '1.0.0',
            created_at DATETIME NOT NULL,
            updated_at DATETIME NOT NULL,
            UNIQUE(instance_id, name)
        )
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create programs table")?;

    // Index for programs by instance_id
    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_programs_instance_id
        ON programs(instance_id)
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create programs index")?;

    // Program data table (key-value storage per program, used by Bridge API)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS program_data (
            program_name TEXT NOT NULL,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            updated_at DATETIME NOT NULL,
            PRIMARY KEY (program_name, key)
        )
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create program_data table")?;

    // Scheduled tasks table (cron-based recurring agent tasks)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS scheduled_tasks (
            id TEXT PRIMARY KEY,
            instance_id TEXT NOT NULL,
            name TEXT NOT NULL,
            cron_expression TEXT NOT NULL,
            task_prompt TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            notify INTEGER NOT NULL DEFAULT 1,
            last_run DATETIME,
            last_result TEXT,
            created_at DATETIME NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create scheduled_tasks table")?;

    // Migration: Add notify column if missing (for existing databases)
    let _ = sqlx::query("ALTER TABLE scheduled_tasks ADD COLUMN notify INTEGER NOT NULL DEFAULT 1")
        .execute(pool)
        .await;

    // Index for scheduled tasks by instance_id
    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_instance_id
        ON scheduled_tasks(instance_id)
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create scheduled_tasks index")?;

    tracing::debug!("Database tables created successfully");

    Ok(())
}
