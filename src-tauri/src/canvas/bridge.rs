//! Bridge API for Canvas programs.
//!
//! Provides communication between Canvas iframe programs and the Rust backend.
//! Programs call `window.ownai.*` methods which are forwarded via postMessage
//! from the React frontend to Tauri commands, and then dispatched here.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Row, Sqlite};
use std::path::{Component, Path, PathBuf};
use tauri::AppHandle;
use tokio::fs;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A bridge request from a Canvas program.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum BridgeRequest {
    #[serde(rename = "chat")]
    Chat { prompt: String },
    #[serde(rename = "storeData")]
    StoreData {
        key: String,
        value: serde_json::Value,
    },
    #[serde(rename = "loadData")]
    LoadData { key: String },
    #[serde(rename = "notify")]
    Notify {
        message: String,
        delay_ms: Option<u64>,
    },
    #[serde(rename = "readFile")]
    ReadFile { path: String },
    #[serde(rename = "writeFile")]
    WriteFile { path: String, content: String },
}

/// A bridge response sent back to the Canvas program.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeResponse {
    pub success: bool,
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
}

impl BridgeResponse {
    pub fn ok(data: serde_json::Value) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn ok_empty() -> Self {
        Self {
            success: true,
            data: None,
            error: None,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

// ---------------------------------------------------------------------------
// Program Data (key-value storage per program)
// ---------------------------------------------------------------------------

/// Store a key-value pair for a program.
pub async fn store_program_data(
    db: &Pool<Sqlite>,
    program_name: &str,
    key: &str,
    value: &serde_json::Value,
) -> Result<()> {
    let now = Utc::now();
    let value_str = serde_json::to_string(value).context("Failed to serialize value")?;

    sqlx::query(
        r#"
        INSERT INTO program_data (program_name, key, value, updated_at)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(program_name, key) DO UPDATE SET
            value = excluded.value,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(program_name)
    .bind(key)
    .bind(&value_str)
    .bind(now)
    .execute(db)
    .await
    .context("Failed to store program data")?;

    Ok(())
}

/// Load a value for a program by key.
pub async fn load_program_data(
    db: &Pool<Sqlite>,
    program_name: &str,
    key: &str,
) -> Result<Option<serde_json::Value>> {
    let row = sqlx::query(
        r#"
        SELECT value FROM program_data
        WHERE program_name = ? AND key = ?
        "#,
    )
    .bind(program_name)
    .bind(key)
    .fetch_optional(db)
    .await
    .context("Failed to load program data")?;

    match row {
        Some(r) => {
            let value_str: String = r.get("value");
            let value: serde_json::Value =
                serde_json::from_str(&value_str).context("Failed to deserialize stored value")?;
            Ok(Some(value))
        }
        None => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Bridge handlers
// ---------------------------------------------------------------------------

/// Handle a storeData bridge request.
pub async fn handle_store_data(
    db: &Pool<Sqlite>,
    program_name: &str,
    key: &str,
    value: &serde_json::Value,
) -> BridgeResponse {
    match store_program_data(db, program_name, key, value).await {
        Ok(()) => BridgeResponse::ok_empty(),
        Err(e) => BridgeResponse::err(format!("Failed to store data: {}", e)),
    }
}

/// Handle a loadData bridge request.
pub async fn handle_load_data(db: &Pool<Sqlite>, program_name: &str, key: &str) -> BridgeResponse {
    match load_program_data(db, program_name, key).await {
        Ok(Some(value)) => BridgeResponse::ok(value),
        Ok(None) => BridgeResponse::ok(serde_json::Value::Null),
        Err(e) => BridgeResponse::err(format!("Failed to load data: {}", e)),
    }
}

/// Handle a notify bridge request.
///
/// Sends a native OS notification via `tauri-plugin-notification` when an
/// `AppHandle` is available. The `instance_name` is used as the notification
/// title. If `delay_ms` is provided, the notification is delayed accordingly.
/// Without an `AppHandle` (e.g. in tests), the notification is logged only.
pub async fn handle_notify(
    app_handle: Option<&AppHandle>,
    instance_name: &str,
    message: &str,
    delay_ms: Option<u64>,
) -> BridgeResponse {
    // Apply optional delay
    if let Some(ms) = delay_ms {
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    }

    match app_handle {
        Some(handle) => {
            use tauri_plugin_notification::NotificationExt;
            match handle
                .notification()
                .builder()
                .title(instance_name)
                .body(message)
                .show()
            {
                Ok(()) => {
                    tracing::info!(
                        "Bridge notification sent - title: '{}', body: '{}'",
                        instance_name,
                        message
                    );
                    BridgeResponse::ok_empty()
                }
                Err(e) => {
                    tracing::warn!("Failed to send bridge notification: {}", e);
                    BridgeResponse::err(format!("Notification failed: {}", e))
                }
            }
        }
        None => {
            tracing::info!(
                "Bridge notification (no AppHandle) - title: '{}', body: '{}'",
                instance_name,
                message
            );
            BridgeResponse::ok_empty()
        }
    }
}

/// Resolves a user-provided relative path within a root directory.
/// Prevents directory traversal attacks and absolute paths.
/// Same pattern as the filesystem tools' resolve_path.
fn resolve_workspace_path(root: &Path, user_path: &str) -> Result<PathBuf, String> {
    let path = Path::new(user_path);

    if path.is_absolute() {
        return Err("Absolute paths are not allowed".to_string());
    }

    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("Parent directory traversal (..) is not allowed".to_string());
    }

    Ok(root.join(path))
}

/// Handle a readFile bridge request (scoped to workspace directory).
pub async fn handle_read_file(workspace: &Path, path: &str) -> BridgeResponse {
    let resolved = match resolve_workspace_path(workspace, path) {
        Ok(p) => p,
        Err(e) => return BridgeResponse::err(e),
    };

    match fs::read_to_string(&resolved).await {
        Ok(content) => BridgeResponse::ok(serde_json::Value::String(content)),
        Err(e) => BridgeResponse::err(format!("Failed to read file '{}': {}", path, e)),
    }
}

/// Handle a writeFile bridge request (scoped to workspace directory).
pub async fn handle_write_file(workspace: &Path, path: &str, content: &str) -> BridgeResponse {
    let resolved = match resolve_workspace_path(workspace, path) {
        Ok(p) => p,
        Err(e) => return BridgeResponse::err(e),
    };

    // Create parent directories if needed
    if let Some(parent) = resolved.parent() {
        if let Err(e) = fs::create_dir_all(parent).await {
            return BridgeResponse::err(format!("Failed to create directories: {}", e));
        }
    }

    match fs::write(&resolved, content).await {
        Ok(()) => BridgeResponse::ok_empty(),
        Err(e) => BridgeResponse::err(format!("Failed to write file '{}': {}", path, e)),
    }
}

/// Returns the JavaScript bridge code that gets injected into Canvas HTML files.
/// This script provides the `window.ownai` API object.
pub fn bridge_script() -> &'static str {
    r#"<script>
(function() {
  "use strict";
  var pending = {};
  var nextId = 1;

  function call(method, params) {
    return new Promise(function(resolve, reject) {
      var requestId = String(nextId++);
      pending[requestId] = { resolve: resolve, reject: reject };
      window.parent.postMessage({
        type: "ownai-bridge-request",
        requestId: requestId,
        method: method,
        params: params || {}
      }, "*");
    });
  }

  window.ownai = {
    chat: function(prompt) { return call("chat", { prompt: prompt }); },
    storeData: function(key, value) { return call("storeData", { key: key, value: value }); },
    loadData: function(key) { return call("loadData", { key: key }); },
    notify: function(message, delay_ms) { return call("notify", { message: message, delay_ms: delay_ms }); },
    readFile: function(path) { return call("readFile", { path: path }); },
    writeFile: function(path, content) { return call("writeFile", { path: path, content: content }); }
  };

  window.addEventListener("message", function(event) {
    if (event.data && event.data.type === "ownai-bridge-response") {
      var requestId = event.data.requestId;
      var entry = pending[requestId];
      if (entry) {
        delete pending[requestId];
        if (event.data.success) {
          entry.resolve(event.data.data);
        } else {
          entry.reject(new Error(event.data.error || "Unknown error"));
        }
      }
    }
  });
})();
</script>"#
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use tempfile::TempDir;

    async fn setup_test_db() -> Pool<Sqlite> {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();

        crate::database::schema::create_tables(&pool).await.unwrap();

        pool
    }

    #[tokio::test]
    async fn test_store_and_load_data() {
        let db = setup_test_db().await;

        let value = serde_json::json!({"score": 42, "name": "Alice"});
        store_program_data(&db, "chess", "game_state", &value)
            .await
            .unwrap();

        let loaded = load_program_data(&db, "chess", "game_state").await.unwrap();
        assert_eq!(loaded, Some(value));
    }

    #[tokio::test]
    async fn test_load_nonexistent_key() {
        let db = setup_test_db().await;

        let loaded = load_program_data(&db, "chess", "nonexistent")
            .await
            .unwrap();
        assert_eq!(loaded, None);
    }

    #[tokio::test]
    async fn test_store_overwrites_existing() {
        let db = setup_test_db().await;

        let v1 = serde_json::json!("first");
        store_program_data(&db, "chess", "key1", &v1).await.unwrap();

        let v2 = serde_json::json!("second");
        store_program_data(&db, "chess", "key1", &v2).await.unwrap();

        let loaded = load_program_data(&db, "chess", "key1").await.unwrap();
        assert_eq!(loaded, Some(serde_json::json!("second")));
    }

    #[tokio::test]
    async fn test_data_isolation_between_programs() {
        let db = setup_test_db().await;

        store_program_data(&db, "chess", "score", &serde_json::json!(100))
            .await
            .unwrap();
        store_program_data(&db, "todo", "score", &serde_json::json!(200))
            .await
            .unwrap();

        let chess_score = load_program_data(&db, "chess", "score").await.unwrap();
        let todo_score = load_program_data(&db, "todo", "score").await.unwrap();

        assert_eq!(chess_score, Some(serde_json::json!(100)));
        assert_eq!(todo_score, Some(serde_json::json!(200)));
    }

    #[tokio::test]
    async fn test_handle_store_data() {
        let db = setup_test_db().await;
        let value = serde_json::json!(42);

        let response = handle_store_data(&db, "test", "key", &value).await;
        assert!(response.success);
        assert!(response.data.is_none());
    }

    #[tokio::test]
    async fn test_handle_load_data_exists() {
        let db = setup_test_db().await;
        let value = serde_json::json!("hello");

        store_program_data(&db, "test", "greeting", &value)
            .await
            .unwrap();

        let response = handle_load_data(&db, "test", "greeting").await;
        assert!(response.success);
        assert_eq!(response.data, Some(serde_json::json!("hello")));
    }

    #[tokio::test]
    async fn test_handle_load_data_missing() {
        let db = setup_test_db().await;

        let response = handle_load_data(&db, "test", "missing").await;
        assert!(response.success);
        assert_eq!(response.data, Some(serde_json::Value::Null));
    }

    #[tokio::test]
    async fn test_handle_notify_without_app_handle() {
        let response = handle_notify(None, "ownAI", "Test notification", None).await;
        assert!(response.success);
    }

    #[tokio::test]
    async fn test_handle_read_file() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        // Create a file in the workspace
        std::fs::write(workspace.join("data.json"), r#"{"key":"value"}"#).unwrap();

        let response = handle_read_file(workspace, "data.json").await;
        assert!(response.success);
        assert_eq!(
            response.data,
            Some(serde_json::Value::String(r#"{"key":"value"}"#.to_string()))
        );
    }

    #[tokio::test]
    async fn test_handle_read_file_subdirectory() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        // Create a subdirectory with a file
        std::fs::create_dir_all(workspace.join("sub")).unwrap();
        std::fs::write(workspace.join("sub").join("notes.txt"), "Hello").unwrap();

        let response = handle_read_file(workspace, "sub/notes.txt").await;
        assert!(response.success);
        assert_eq!(
            response.data,
            Some(serde_json::Value::String("Hello".to_string()))
        );
    }

    #[tokio::test]
    async fn test_handle_read_file_not_found() {
        let temp_dir = TempDir::new().unwrap();

        let response = handle_read_file(temp_dir.path(), "nope.txt").await;
        assert!(!response.success);
        assert!(response.error.unwrap().contains("Failed to read file"));
    }

    #[tokio::test]
    async fn test_handle_read_file_blocks_traversal() {
        let temp_dir = TempDir::new().unwrap();

        let response = handle_read_file(temp_dir.path(), "../secret.txt").await;
        assert!(!response.success);
        assert!(response.error.unwrap().contains("traversal"));
    }

    #[tokio::test]
    async fn test_handle_read_file_blocks_absolute() {
        let temp_dir = TempDir::new().unwrap();

        let response = handle_read_file(temp_dir.path(), "/etc/passwd").await;
        assert!(!response.success);
        assert!(response.error.unwrap().contains("Absolute paths"));
    }

    #[tokio::test]
    async fn test_handle_write_file() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        let response = handle_write_file(workspace, "output.txt", "Hello Bridge").await;
        assert!(response.success);

        let content = std::fs::read_to_string(workspace.join("output.txt")).unwrap();
        assert_eq!(content, "Hello Bridge");
    }

    #[tokio::test]
    async fn test_handle_write_file_creates_subdirectory() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        let response = handle_write_file(workspace, "data/save.json", r#"{"ok":true}"#).await;
        assert!(response.success);

        let content = std::fs::read_to_string(workspace.join("data").join("save.json")).unwrap();
        assert_eq!(content, r#"{"ok":true}"#);
    }

    #[tokio::test]
    async fn test_handle_write_file_blocks_traversal() {
        let temp_dir = TempDir::new().unwrap();

        let response = handle_write_file(temp_dir.path(), "../evil.txt", "bad").await;
        assert!(!response.success);
        assert!(response.error.unwrap().contains("traversal"));
    }

    #[tokio::test]
    async fn test_handle_write_file_blocks_absolute() {
        let temp_dir = TempDir::new().unwrap();

        let response = handle_write_file(temp_dir.path(), "/tmp/evil.txt", "bad").await;
        assert!(!response.success);
        assert!(response.error.unwrap().contains("Absolute paths"));
    }

    #[tokio::test]
    async fn test_bridge_script_contains_ownai() {
        let script = bridge_script();
        assert!(script.contains("window.ownai"));
        assert!(script.contains("ownai-bridge-request"));
        assert!(script.contains("ownai-bridge-response"));
        assert!(script.contains("chat"));
        assert!(script.contains("storeData"));
        assert!(script.contains("loadData"));
        assert!(script.contains("notify"));
        assert!(script.contains("readFile"));
        assert!(script.contains("writeFile"));
    }

    #[tokio::test]
    async fn test_bridge_response_ok() {
        let response = BridgeResponse::ok(serde_json::json!(42));
        assert!(response.success);
        assert_eq!(response.data, Some(serde_json::json!(42)));
        assert!(response.error.is_none());
    }

    #[tokio::test]
    async fn test_bridge_response_ok_empty() {
        let response = BridgeResponse::ok_empty();
        assert!(response.success);
        assert!(response.data.is_none());
        assert!(response.error.is_none());
    }

    #[tokio::test]
    async fn test_bridge_response_err() {
        let response = BridgeResponse::err("something failed");
        assert!(!response.success);
        assert!(response.data.is_none());
        assert_eq!(response.error, Some("something failed".to_string()));
    }
}
