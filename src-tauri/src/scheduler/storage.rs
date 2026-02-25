//! Database CRUD operations for scheduled tasks.

use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::{Pool, Row, Sqlite};

use super::ScheduledTask;

/// Load all scheduled tasks for an instance from the database.
pub async fn load_tasks(db: &Pool<Sqlite>, instance_id: &str) -> Result<Vec<ScheduledTask>> {
    let rows = sqlx::query(
        r#"
        SELECT id, instance_id, name, cron_expression, task_prompt,
               enabled, last_run, last_result, created_at
        FROM scheduled_tasks
        WHERE instance_id = ?
        ORDER BY created_at ASC
        "#,
    )
    .bind(instance_id)
    .fetch_all(db)
    .await
    .context("Failed to load scheduled tasks")?;

    let tasks = rows
        .into_iter()
        .map(|row| ScheduledTask {
            id: row.get("id"),
            instance_id: row.get("instance_id"),
            name: row.get("name"),
            cron_expression: row.get("cron_expression"),
            task_prompt: row.get("task_prompt"),
            enabled: row.get::<i32, _>("enabled") != 0,
            last_run: row.get("last_run"),
            last_result: row.get("last_result"),
            created_at: row.get("created_at"),
        })
        .collect();

    Ok(tasks)
}

/// Save a new scheduled task to the database.
pub async fn save_task(db: &Pool<Sqlite>, task: &ScheduledTask) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO scheduled_tasks
            (id, instance_id, name, cron_expression, task_prompt, enabled, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&task.id)
    .bind(&task.instance_id)
    .bind(&task.name)
    .bind(&task.cron_expression)
    .bind(&task.task_prompt)
    .bind(task.enabled as i32)
    .bind(task.created_at)
    .execute(db)
    .await
    .context("Failed to save scheduled task")?;

    tracing::info!("Saved scheduled task '{}' ({})", task.name, task.id);
    Ok(())
}

/// Delete a scheduled task from the database.
pub async fn delete_task(db: &Pool<Sqlite>, task_id: &str) -> Result<()> {
    let result = sqlx::query("DELETE FROM scheduled_tasks WHERE id = ?")
        .bind(task_id)
        .execute(db)
        .await
        .context("Failed to delete scheduled task")?;

    if result.rows_affected() == 0 {
        anyhow::bail!("Scheduled task not found: {}", task_id);
    }

    tracing::info!("Deleted scheduled task: {}", task_id);
    Ok(())
}

/// Update the last_run timestamp and last_result for a task.
pub async fn update_task_last_run(db: &Pool<Sqlite>, task_id: &str, result: &str) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE scheduled_tasks
        SET last_run = ?, last_result = ?
        WHERE id = ?
        "#,
    )
    .bind(Utc::now())
    .bind(result)
    .bind(task_id)
    .execute(db)
    .await
    .context("Failed to update task last_run")?;

    Ok(())
}

/// Enable or disable a scheduled task.
pub async fn set_task_enabled(db: &Pool<Sqlite>, task_id: &str, enabled: bool) -> Result<()> {
    let result = sqlx::query("UPDATE scheduled_tasks SET enabled = ? WHERE id = ?")
        .bind(enabled as i32)
        .bind(task_id)
        .execute(db)
        .await
        .context("Failed to update task enabled state")?;

    if result.rows_affected() == 0 {
        anyhow::bail!("Scheduled task not found: {}", task_id);
    }

    tracing::info!(
        "Scheduled task '{}' {}",
        task_id,
        if enabled { "enabled" } else { "disabled" }
    );
    Ok(())
}

/// Get a single scheduled task by ID.
pub async fn get_task(db: &Pool<Sqlite>, task_id: &str) -> Result<Option<ScheduledTask>> {
    let row = sqlx::query(
        r#"
        SELECT id, instance_id, name, cron_expression, task_prompt,
               enabled, last_run, last_result, created_at
        FROM scheduled_tasks
        WHERE id = ?
        "#,
    )
    .bind(task_id)
    .fetch_optional(db)
    .await
    .context("Failed to get scheduled task")?;

    Ok(row.map(|row| ScheduledTask {
        id: row.get("id"),
        instance_id: row.get("instance_id"),
        name: row.get("name"),
        cron_expression: row.get("cron_expression"),
        task_prompt: row.get("task_prompt"),
        enabled: row.get::<i32, _>("enabled") != 0,
        last_run: row.get("last_run"),
        last_result: row.get("last_result"),
        created_at: row.get("created_at"),
    }))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    async fn setup_test_db() -> Pool<Sqlite> {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::database::schema::create_tables(&pool).await.unwrap();
        pool
    }

    fn make_task(id: &str, name: &str) -> ScheduledTask {
        ScheduledTask {
            id: id.to_string(),
            instance_id: "test-instance".to_string(),
            name: name.to_string(),
            cron_expression: "0 8 * * *".to_string(),
            task_prompt: "Do something".to_string(),
            enabled: true,
            last_run: None,
            last_result: None,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_save_and_load_tasks() {
        let db = setup_test_db().await;

        save_task(&db, &make_task("t1", "task-one")).await.unwrap();
        save_task(&db, &make_task("t2", "task-two")).await.unwrap();

        let tasks = load_tasks(&db, "test-instance").await.unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].name, "task-one");
        assert_eq!(tasks[1].name, "task-two");
    }

    #[tokio::test]
    async fn test_load_tasks_filters_by_instance() {
        let db = setup_test_db().await;

        let mut t1 = make_task("t1", "task-one");
        t1.instance_id = "inst-a".to_string();
        save_task(&db, &t1).await.unwrap();

        let mut t2 = make_task("t2", "task-two");
        t2.instance_id = "inst-b".to_string();
        save_task(&db, &t2).await.unwrap();

        let tasks_a = load_tasks(&db, "inst-a").await.unwrap();
        assert_eq!(tasks_a.len(), 1);
        assert_eq!(tasks_a[0].name, "task-one");
    }

    #[tokio::test]
    async fn test_delete_task() {
        let db = setup_test_db().await;

        save_task(&db, &make_task("t1", "task-one")).await.unwrap();
        assert_eq!(load_tasks(&db, "test-instance").await.unwrap().len(), 1);

        delete_task(&db, "t1").await.unwrap();
        assert_eq!(load_tasks(&db, "test-instance").await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_delete_nonexistent_task() {
        let db = setup_test_db().await;
        let result = delete_task(&db, "nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_last_run() {
        let db = setup_test_db().await;
        save_task(&db, &make_task("t1", "task-one")).await.unwrap();

        update_task_last_run(&db, "t1", "Success: done")
            .await
            .unwrap();

        let task = get_task(&db, "t1").await.unwrap().unwrap();
        assert!(task.last_run.is_some());
        assert_eq!(task.last_result.unwrap(), "Success: done");
    }

    #[tokio::test]
    async fn test_set_task_enabled() {
        let db = setup_test_db().await;
        save_task(&db, &make_task("t1", "task-one")).await.unwrap();

        // Disable
        set_task_enabled(&db, "t1", false).await.unwrap();
        let task = get_task(&db, "t1").await.unwrap().unwrap();
        assert!(!task.enabled);

        // Re-enable
        set_task_enabled(&db, "t1", true).await.unwrap();
        let task = get_task(&db, "t1").await.unwrap().unwrap();
        assert!(task.enabled);
    }

    #[tokio::test]
    async fn test_get_task_not_found() {
        let db = setup_test_db().await;
        let result = get_task(&db, "nonexistent").await.unwrap();
        assert!(result.is_none());
    }
}
