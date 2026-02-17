pub mod schema;

use crate::utils::paths::get_instance_db_path;
use anyhow::{Context, Result};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePool},
    Pool, Sqlite,
};

/// Initialize database connection for an instance
pub async fn init_database(instance_id: &str) -> Result<Pool<Sqlite>> {
    let db_path = get_instance_db_path(instance_id)?;

    // Create parent directory if it doesn't exist
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create database directory")?;
    }

    // Use SqliteConnectOptions to ensure the database file is created
    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);

    let pool = SqlitePool::connect_with(options)
        .await
        .context("Failed to connect to database")?;

    // Run migrations
    schema::create_tables(&pool).await?;

    tracing::info!("Database initialized for instance: {}", instance_id);

    Ok(pool)
}
