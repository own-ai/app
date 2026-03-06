pub mod schema;

use crate::utils::paths::get_instance_db_path;
use anyhow::{Context, Result};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePool},
    Pool, Sqlite,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Cache of database pools per instance, avoiding repeated init_database() calls.
///
/// Each instance gets a single `SqlitePool` that is created on first access and
/// reused for all subsequent operations. `SqlitePool` is internally `Arc`-based,
/// so cloning from the cache is cheap (shared handle, not a new connection).
pub type DbCache = Arc<Mutex<HashMap<String, Pool<Sqlite>>>>;

/// Get a cached database pool for an instance, or initialize and cache it.
///
/// This is the primary entry point for obtaining a database pool. It avoids
/// the overhead of creating a new connection + running migration checks on
/// every command invocation.
pub async fn get_or_init_db(cache: &DbCache, instance_id: &str) -> Result<Pool<Sqlite>> {
    let mut map = cache.lock().await;
    if let Some(pool) = map.get(instance_id) {
        return Ok(pool.clone());
    }

    let pool = init_database(instance_id).await?;
    map.insert(instance_id.to_string(), pool.clone());
    Ok(pool)
}

/// Remove a cached pool for an instance (call when instance is deleted).
pub async fn remove_cached_db(cache: &DbCache, instance_id: &str) {
    let mut map = cache.lock().await;
    map.remove(instance_id);
}

/// Initialize database connection for an instance.
///
/// This is the low-level function that creates a new pool and runs migrations.
/// Prefer `get_or_init_db()` for normal usage to benefit from caching.
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
    schema::run_migrations(&pool).await?;

    tracing::info!("Database initialized for instance: {}", instance_id);

    Ok(pool)
}
