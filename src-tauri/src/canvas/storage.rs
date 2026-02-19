use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::{Pool, Row, Sqlite};
use std::path::Path;

use super::ProgramMetadata;

/// Create a new program entry in the database and its directory on disk.
pub async fn create_program_in_db(
    db: &Pool<Sqlite>,
    instance_id: &str,
    name: &str,
    description: &str,
    programs_root: &Path,
) -> Result<ProgramMetadata> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now();

    // Create the program directory
    let program_dir = programs_root.join(name);
    tokio::fs::create_dir_all(&program_dir)
        .await
        .context("Failed to create program directory")?;

    // Insert into database
    sqlx::query(
        r#"
        INSERT INTO programs (id, instance_id, name, description, version, created_at, updated_at)
        VALUES (?, ?, ?, ?, '1.0.0', ?, ?)
        "#,
    )
    .bind(&id)
    .bind(instance_id)
    .bind(name)
    .bind(description)
    .bind(now)
    .bind(now)
    .execute(db)
    .await
    .context("Failed to insert program into database")?;

    Ok(ProgramMetadata {
        id,
        instance_id: instance_id.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        version: "1.0.0".to_string(),
        created_at: now.to_rfc3339(),
        updated_at: now.to_rfc3339(),
    })
}

/// List all programs for an instance.
pub async fn list_programs_from_db(
    db: &Pool<Sqlite>,
    instance_id: &str,
) -> Result<Vec<ProgramMetadata>> {
    let rows = sqlx::query(
        r#"
        SELECT id, instance_id, name, description, version, created_at, updated_at
        FROM programs
        WHERE instance_id = ?
        ORDER BY name ASC
        "#,
    )
    .bind(instance_id)
    .fetch_all(db)
    .await
    .context("Failed to list programs")?;

    let programs = rows
        .into_iter()
        .map(|row| ProgramMetadata {
            id: row.get("id"),
            instance_id: row.get("instance_id"),
            name: row.get("name"),
            description: row.get("description"),
            version: row.get("version"),
            created_at: row.get::<String, _>("created_at"),
            updated_at: row.get::<String, _>("updated_at"),
        })
        .collect();

    Ok(programs)
}

/// Get a program by its name within an instance.
pub async fn get_program_by_name(
    db: &Pool<Sqlite>,
    instance_id: &str,
    name: &str,
) -> Result<Option<ProgramMetadata>> {
    let row = sqlx::query(
        r#"
        SELECT id, instance_id, name, description, version, created_at, updated_at
        FROM programs
        WHERE instance_id = ? AND name = ?
        "#,
    )
    .bind(instance_id)
    .bind(name)
    .fetch_optional(db)
    .await
    .context("Failed to get program")?;

    Ok(row.map(|r| ProgramMetadata {
        id: r.get("id"),
        instance_id: r.get("instance_id"),
        name: r.get("name"),
        description: r.get("description"),
        version: r.get("version"),
        created_at: r.get::<String, _>("created_at"),
        updated_at: r.get::<String, _>("updated_at"),
    }))
}

/// Delete a program from the database and remove its directory from disk.
pub async fn delete_program_from_db(
    db: &Pool<Sqlite>,
    instance_id: &str,
    program_name: &str,
    programs_root: &Path,
) -> Result<()> {
    // Delete from database
    let result = sqlx::query(
        r#"
        DELETE FROM programs
        WHERE instance_id = ? AND name = ?
        "#,
    )
    .bind(instance_id)
    .bind(program_name)
    .execute(db)
    .await
    .context("Failed to delete program from database")?;

    if result.rows_affected() == 0 {
        return Err(anyhow::anyhow!("Program '{}' not found", program_name));
    }

    // Remove program directory from disk
    let program_dir = programs_root.join(program_name);
    if program_dir.exists() {
        tokio::fs::remove_dir_all(&program_dir)
            .await
            .context("Failed to remove program directory")?;
    }

    Ok(())
}

/// Increment the version of a program and update its timestamp.
pub async fn update_program_version(
    db: &Pool<Sqlite>,
    instance_id: &str,
    program_name: &str,
) -> Result<String> {
    let now = Utc::now();

    // Get current version
    let row = sqlx::query(
        r#"
        SELECT version FROM programs
        WHERE instance_id = ? AND name = ?
        "#,
    )
    .bind(instance_id)
    .bind(program_name)
    .fetch_optional(db)
    .await
    .context("Failed to get program version")?
    .ok_or_else(|| anyhow::anyhow!("Program '{}' not found", program_name))?;

    let current_version: String = row.get("version");
    let new_version = increment_version(&current_version);

    // Update version and timestamp
    sqlx::query(
        r#"
        UPDATE programs
        SET version = ?, updated_at = ?
        WHERE instance_id = ? AND name = ?
        "#,
    )
    .bind(&new_version)
    .bind(now)
    .bind(instance_id)
    .bind(program_name)
    .execute(db)
    .await
    .context("Failed to update program version")?;

    Ok(new_version)
}

/// Increment a semver-style version string (e.g. "1.0.0" -> "1.0.1")
fn increment_version(version: &str) -> String {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() == 3 {
        if let Ok(patch) = parts[2].parse::<u32>() {
            return format!("{}.{}.{}", parts[0], parts[1], patch + 1);
        }
    }
    format!("{}.1", version)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use tempfile::TempDir;

    async fn setup_test_db() -> (Pool<Sqlite>, TempDir) {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();

        crate::database::schema::create_tables(&pool).await.unwrap();

        let temp_dir = TempDir::new().unwrap();
        (pool, temp_dir)
    }

    #[tokio::test]
    async fn test_create_program() {
        let (db, temp_dir) = setup_test_db().await;
        let programs_root = temp_dir.path();

        let program = create_program_in_db(&db, "inst-1", "chess", "A chess game", programs_root)
            .await
            .unwrap();

        assert_eq!(program.name, "chess");
        assert_eq!(program.description, "A chess game");
        assert_eq!(program.version, "1.0.0");
        assert_eq!(program.instance_id, "inst-1");

        // Verify directory was created
        assert!(programs_root.join("chess").exists());
    }

    #[tokio::test]
    async fn test_create_duplicate_program_fails() {
        let (db, temp_dir) = setup_test_db().await;
        let programs_root = temp_dir.path();

        create_program_in_db(&db, "inst-1", "chess", "v1", programs_root)
            .await
            .unwrap();

        let result = create_program_in_db(&db, "inst-1", "chess", "v2", programs_root).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_programs() {
        let (db, temp_dir) = setup_test_db().await;
        let programs_root = temp_dir.path();

        create_program_in_db(&db, "inst-1", "chess", "Chess game", programs_root)
            .await
            .unwrap();
        create_program_in_db(&db, "inst-1", "todo", "Todo app", programs_root)
            .await
            .unwrap();
        // Different instance
        create_program_in_db(&db, "inst-2", "other", "Other", programs_root)
            .await
            .unwrap();

        let programs = list_programs_from_db(&db, "inst-1").await.unwrap();
        assert_eq!(programs.len(), 2);
        assert_eq!(programs[0].name, "chess");
        assert_eq!(programs[1].name, "todo");
    }

    #[tokio::test]
    async fn test_get_program_by_name() {
        let (db, temp_dir) = setup_test_db().await;
        let programs_root = temp_dir.path();

        create_program_in_db(&db, "inst-1", "chess", "Chess game", programs_root)
            .await
            .unwrap();

        let found = get_program_by_name(&db, "inst-1", "chess").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "chess");

        let not_found = get_program_by_name(&db, "inst-1", "nope").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_delete_program() {
        let (db, temp_dir) = setup_test_db().await;
        let programs_root = temp_dir.path();

        create_program_in_db(&db, "inst-1", "chess", "Chess game", programs_root)
            .await
            .unwrap();

        assert!(programs_root.join("chess").exists());

        delete_program_from_db(&db, "inst-1", "chess", programs_root)
            .await
            .unwrap();

        assert!(!programs_root.join("chess").exists());
        let programs = list_programs_from_db(&db, "inst-1").await.unwrap();
        assert!(programs.is_empty());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_program_fails() {
        let (db, temp_dir) = setup_test_db().await;
        let programs_root = temp_dir.path();

        let result = delete_program_from_db(&db, "inst-1", "nope", programs_root).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_program_version() {
        let (db, temp_dir) = setup_test_db().await;
        let programs_root = temp_dir.path();

        create_program_in_db(&db, "inst-1", "chess", "Chess", programs_root)
            .await
            .unwrap();

        let v1 = update_program_version(&db, "inst-1", "chess")
            .await
            .unwrap();
        assert_eq!(v1, "1.0.1");

        let v2 = update_program_version(&db, "inst-1", "chess")
            .await
            .unwrap();
        assert_eq!(v2, "1.0.2");
    }

    #[test]
    fn test_increment_version() {
        assert_eq!(increment_version("1.0.0"), "1.0.1");
        assert_eq!(increment_version("1.0.9"), "1.0.10");
        assert_eq!(increment_version("2.3.5"), "2.3.6");
        assert_eq!(increment_version("bad"), "bad.1");
    }
}
