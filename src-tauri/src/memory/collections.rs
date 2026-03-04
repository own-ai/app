//! Knowledge Collections for organizing domain-specific knowledge.
//!
//! A Knowledge Collection groups related memory entries by topic/source,
//! enabling structured ingestion of documents and targeted retrieval.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Row, Sqlite};

/// A knowledge collection groups memory entries by topic or source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeCollection {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: Option<String>,
    pub document_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create a new knowledge collection.
pub async fn create_collection(
    db: &Pool<Sqlite>,
    name: &str,
    description: &str,
    source: Option<&str>,
) -> Result<KnowledgeCollection> {
    let now = Utc::now();
    let collection = KnowledgeCollection {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_string(),
        description: description.to_string(),
        source: source.map(|s| s.to_string()),
        document_count: 0,
        created_at: now,
        updated_at: now,
    };

    sqlx::query(
        r#"
        INSERT INTO knowledge_collections (id, name, description, source, document_count, created_at, updated_at)
        VALUES (?, ?, ?, ?, 0, ?, ?)
        "#,
    )
    .bind(&collection.id)
    .bind(&collection.name)
    .bind(&collection.description)
    .bind(&collection.source)
    .bind(collection.created_at)
    .bind(collection.updated_at)
    .execute(db)
    .await
    .context("Failed to create knowledge collection")?;

    tracing::info!(
        "Created knowledge collection '{}' (id: {})",
        collection.name,
        collection.id
    );

    Ok(collection)
}

/// Find a collection by name. Returns None if not found.
pub async fn get_collection_by_name(
    db: &Pool<Sqlite>,
    name: &str,
) -> Result<Option<KnowledgeCollection>> {
    let row = sqlx::query(
        r#"
        SELECT id, name, description, source, document_count, created_at, updated_at
        FROM knowledge_collections
        WHERE name = ?
        "#,
    )
    .bind(name)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|r| KnowledgeCollection {
        id: r.get("id"),
        name: r.get("name"),
        description: r.get("description"),
        source: r.get("source"),
        document_count: r.get("document_count"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }))
}

/// Find a collection by ID. Returns None if not found.
pub async fn get_collection_by_id(
    db: &Pool<Sqlite>,
    id: &str,
) -> Result<Option<KnowledgeCollection>> {
    let row = sqlx::query(
        r#"
        SELECT id, name, description, source, document_count, created_at, updated_at
        FROM knowledge_collections
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|r| KnowledgeCollection {
        id: r.get("id"),
        name: r.get("name"),
        description: r.get("description"),
        source: r.get("source"),
        document_count: r.get("document_count"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }))
}

/// List all knowledge collections, ordered by most recently updated.
pub async fn list_collections(db: &Pool<Sqlite>) -> Result<Vec<KnowledgeCollection>> {
    let rows = sqlx::query(
        r#"
        SELECT id, name, description, source, document_count, created_at, updated_at
        FROM knowledge_collections
        ORDER BY updated_at DESC
        "#,
    )
    .fetch_all(db)
    .await?;

    let collections = rows
        .into_iter()
        .map(|r| KnowledgeCollection {
            id: r.get("id"),
            name: r.get("name"),
            description: r.get("description"),
            source: r.get("source"),
            document_count: r.get("document_count"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        })
        .collect();

    Ok(collections)
}

/// Delete a collection and all its associated memory entries.
pub async fn delete_collection(db: &Pool<Sqlite>, id: &str) -> Result<()> {
    // Delete all memory entries belonging to this collection
    sqlx::query("DELETE FROM memory_entries WHERE collection_id = ?")
        .bind(id)
        .execute(db)
        .await
        .context("Failed to delete collection memory entries")?;

    // Delete the collection itself
    sqlx::query("DELETE FROM knowledge_collections WHERE id = ?")
        .bind(id)
        .execute(db)
        .await
        .context("Failed to delete knowledge collection")?;

    tracing::info!("Deleted knowledge collection: {}", id);

    Ok(())
}

/// Delete all memory entries belonging to a collection (without deleting the collection).
pub async fn clear_collection_entries(db: &Pool<Sqlite>, collection_id: &str) -> Result<u64> {
    let result = sqlx::query("DELETE FROM memory_entries WHERE collection_id = ?")
        .bind(collection_id)
        .execute(db)
        .await
        .context("Failed to clear collection entries")?;

    let count = result.rows_affected();
    tracing::info!(
        "Cleared {} entries from collection {}",
        count,
        collection_id
    );

    Ok(count)
}

/// Recount and update the document_count for a collection.
pub async fn update_collection_count(db: &Pool<Sqlite>, collection_id: &str) -> Result<i32> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memory_entries WHERE collection_id = ?")
            .bind(collection_id)
            .fetch_one(db)
            .await?;

    let now = Utc::now();
    sqlx::query("UPDATE knowledge_collections SET document_count = ?, updated_at = ? WHERE id = ?")
        .bind(count as i32)
        .bind(now)
        .bind(collection_id)
        .execute(db)
        .await?;

    Ok(count as i32)
}

/// Find or create a collection by name. If it exists, return it; otherwise create it.
pub async fn find_or_create_collection(
    db: &Pool<Sqlite>,
    name: &str,
    description: &str,
    source: Option<&str>,
) -> Result<KnowledgeCollection> {
    if let Some(existing) = get_collection_by_name(db, name).await? {
        return Ok(existing);
    }
    create_collection(db, name, description, source).await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_test_db() -> Pool<Sqlite> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory database");

        crate::database::schema::run_migrations(&pool)
            .await
            .expect("Failed to run migrations");

        pool
    }

    #[tokio::test]
    async fn test_create_and_get_collection() {
        let db = setup_test_db().await;

        let collection = create_collection(
            &db,
            "funding-2025",
            "Funding applications guide",
            Some("guide.pdf"),
        )
        .await
        .unwrap();

        assert_eq!(collection.name, "funding-2025");
        assert_eq!(collection.description, "Funding applications guide");
        assert_eq!(collection.source.as_deref(), Some("guide.pdf"));
        assert_eq!(collection.document_count, 0);

        // Get by name
        let found = get_collection_by_name(&db, "funding-2025").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, collection.id);

        // Get by ID
        let found = get_collection_by_id(&db, &collection.id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "funding-2025");
    }

    #[tokio::test]
    async fn test_list_collections() {
        let db = setup_test_db().await;

        create_collection(&db, "topic-a", "Topic A", None)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        create_collection(&db, "topic-b", "Topic B", Some("source.pdf"))
            .await
            .unwrap();

        let collections = list_collections(&db).await.unwrap();
        assert_eq!(collections.len(), 2);
        // Most recently updated first
        assert_eq!(collections[0].name, "topic-b");
        assert_eq!(collections[1].name, "topic-a");
    }

    #[tokio::test]
    async fn test_delete_collection() {
        let db = setup_test_db().await;

        let collection = create_collection(&db, "to-delete", "Will be deleted", None)
            .await
            .unwrap();

        // Insert a fake memory entry with this collection_id
        sqlx::query(
            "INSERT INTO memory_entries (id, content, embedding, entry_type, importance, created_at, last_accessed, collection_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("entry-1")
        .bind("Some fact")
        .bind(vec![0u8; 4])
        .bind("\"fact\"")
        .bind(0.5)
        .bind(Utc::now())
        .bind(Utc::now())
        .bind(&collection.id)
        .execute(&db)
        .await
        .unwrap();

        delete_collection(&db, &collection.id).await.unwrap();

        // Collection should be gone
        let found = get_collection_by_id(&db, &collection.id).await.unwrap();
        assert!(found.is_none());

        // Memory entry should be gone too
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_entries")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_update_collection_count() {
        let db = setup_test_db().await;

        let collection = create_collection(&db, "counted", "Counting test", None)
            .await
            .unwrap();

        // Insert 3 memory entries
        for i in 0..3 {
            sqlx::query(
                "INSERT INTO memory_entries (id, content, embedding, entry_type, importance, created_at, last_accessed, collection_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(format!("entry-{}", i))
            .bind(format!("Fact {}", i))
            .bind(vec![0u8; 4])
            .bind("\"fact\"")
            .bind(0.5)
            .bind(Utc::now())
            .bind(Utc::now())
            .bind(&collection.id)
            .execute(&db)
            .await
            .unwrap();
        }

        let count = update_collection_count(&db, &collection.id).await.unwrap();
        assert_eq!(count, 3);

        // Verify it was persisted
        let updated = get_collection_by_id(&db, &collection.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.document_count, 3);
    }

    #[tokio::test]
    async fn test_find_or_create_collection() {
        let db = setup_test_db().await;

        // First call creates
        let c1 = find_or_create_collection(&db, "my-topic", "A topic", None)
            .await
            .unwrap();
        assert_eq!(c1.name, "my-topic");

        // Second call returns existing
        let c2 = find_or_create_collection(&db, "my-topic", "Different desc", Some("new-source"))
            .await
            .unwrap();
        assert_eq!(c2.id, c1.id);
        // Description stays the same (not updated)
        assert_eq!(c2.description, "A topic");
    }

    #[tokio::test]
    async fn test_clear_collection_entries() {
        let db = setup_test_db().await;

        let collection = create_collection(&db, "clearable", "Will be cleared", None)
            .await
            .unwrap();

        // Insert entries
        for i in 0..5 {
            sqlx::query(
                "INSERT INTO memory_entries (id, content, embedding, entry_type, importance, created_at, last_accessed, collection_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(format!("entry-{}", i))
            .bind(format!("Fact {}", i))
            .bind(vec![0u8; 4])
            .bind("\"fact\"")
            .bind(0.5)
            .bind(Utc::now())
            .bind(Utc::now())
            .bind(&collection.id)
            .execute(&db)
            .await
            .unwrap();
        }

        let deleted = clear_collection_entries(&db, &collection.id).await.unwrap();
        assert_eq!(deleted, 5);

        // Collection itself still exists
        let found = get_collection_by_id(&db, &collection.id).await.unwrap();
        assert!(found.is_some());

        // But entries are gone
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_entries")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_get_nonexistent_collection() {
        let db = setup_test_db().await;

        let found = get_collection_by_name(&db, "nonexistent").await.unwrap();
        assert!(found.is_none());

        let found = get_collection_by_id(&db, "no-such-id").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_duplicate_collection_name_fails() {
        let db = setup_test_db().await;

        create_collection(&db, "unique-name", "First", None)
            .await
            .unwrap();

        let result = create_collection(&db, "unique-name", "Second", None).await;
        assert!(result.is_err()); // UNIQUE constraint violation
    }
}
