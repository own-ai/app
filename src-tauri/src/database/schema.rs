use anyhow::{Context, Result};
use sqlx::{Pool, Sqlite};

/// Run all pending database migrations for an instance.
///
/// Migrations are embedded at compile time from `src-tauri/migrations/`.
/// sqlx automatically tracks which migrations have been applied via a
/// `_sqlx_migrations` table, so each migration runs at most once.
///
/// For new schema changes, add a new `.sql` file to the migrations directory
/// with a timestamp prefix (e.g. `20260304162200_description.sql`).
pub async fn run_migrations(pool: &Pool<Sqlite>) -> Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("Failed to run database migrations")?;

    tracing::debug!("Database migrations completed successfully");

    Ok(())
}
