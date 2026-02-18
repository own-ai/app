//! Tool Registry for managing dynamic Rhai tools.
//!
//! Stores tool definitions in SQLite, compiles and caches Rhai ASTs,
//! executes scripts within the sandboxed engine, and logs every execution.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rhai::{Engine, AST};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Row, Sqlite};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use super::rhai_engine::create_sandboxed_engine;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A dynamic tool record stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub script_content: String,
    pub parameters: Vec<ParameterDef>,
    pub status: ToolStatus,
    pub created_at: DateTime<Utc>,
    pub last_used: Option<DateTime<Utc>>,
    pub usage_count: i32,
    pub success_count: i32,
    pub failure_count: i32,
    pub parent_tool_id: Option<String>,
}

/// Lifecycle status of a tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Active,
    Deprecated,
    Testing,
}

impl std::fmt::Display for ToolStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolStatus::Active => write!(f, "active"),
            ToolStatus::Deprecated => write!(f, "deprecated"),
            ToolStatus::Testing => write!(f, "testing"),
        }
    }
}

impl ToolStatus {
    fn from_str(s: &str) -> Self {
        match s {
            "deprecated" => Self::Deprecated,
            "testing" => Self::Testing,
            _ => Self::Active,
        }
    }
}

/// A parameter definition for a dynamic tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterDef {
    pub name: String,
    pub type_hint: String,
    pub description: String,
    pub required: bool,
}

/// Record of a single tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionRecord {
    pub id: String,
    pub tool_id: String,
    pub timestamp: DateTime<Utc>,
    pub success: bool,
    pub execution_time_ms: i64,
    pub error_message: Option<String>,
    pub input_params: serde_json::Value,
    pub output: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Increment a semver-like version string (e.g. "1.0.0" -> "1.1.0").
/// Increments the minor version. Falls back to appending ".1" on parse failure.
fn increment_version(version: &str) -> String {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() == 3 {
        let major = parts[0];
        let minor: u32 = parts[1].parse().unwrap_or(0);
        let _patch = parts[2];
        format!("{}.{}.0", major, minor + 1)
    } else {
        format!("{}.1", version)
    }
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// Manages dynamic Rhai tool lifecycle: register, compile, cache, execute.
pub struct RhaiToolRegistry {
    engine: Engine,
    compiled_cache: HashMap<String, Arc<AST>>,
    db: Pool<Sqlite>,
}

impl RhaiToolRegistry {
    /// Create a new registry backed by the given database and workspace path.
    pub fn new(db: Pool<Sqlite>, workspace: PathBuf) -> Self {
        let engine = create_sandboxed_engine(workspace);
        Self {
            engine,
            compiled_cache: HashMap::new(),
            db,
        }
    }

    /// Register a new tool: validate the script, store in DB, and cache the compiled AST.
    pub async fn register_tool(
        &mut self,
        name: &str,
        description: &str,
        script_content: &str,
        parameters: Vec<ParameterDef>,
    ) -> Result<ToolRecord> {
        // Validate: compile the script to check for syntax errors
        let ast = self
            .engine
            .compile(script_content)
            .map_err(|e| anyhow::anyhow!("Script compilation failed: {}", e))?;

        let id = uuid::Uuid::new_v4().to_string();
        let params_json =
            serde_json::to_string(&parameters).context("Failed to serialize parameters")?;
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO tools (id, name, description, version, script_content, parameters, status, created_at)
            VALUES (?, ?, ?, '1.0.0', ?, ?, 'active', ?)
            "#,
        )
        .bind(&id)
        .bind(name)
        .bind(description)
        .bind(script_content)
        .bind(&params_json)
        .bind(now)
        .execute(&self.db)
        .await
        .context("Failed to insert tool into database")?;

        // Cache compiled AST
        self.compiled_cache.insert(name.to_string(), Arc::new(ast));

        tracing::info!("Registered dynamic tool '{}' (id: {})", name, id);

        Ok(ToolRecord {
            id,
            name: name.to_string(),
            description: description.to_string(),
            version: "1.0.0".to_string(),
            script_content: script_content.to_string(),
            parameters,
            status: ToolStatus::Active,
            created_at: now,
            last_used: None,
            usage_count: 0,
            success_count: 0,
            failure_count: 0,
            parent_tool_id: None,
        })
    }

    /// Execute a tool by name with the given JSON parameters.
    ///
    /// The parameters are injected as a Rhai scope variable named `params`.
    /// The script's last expression value is returned as a string.
    pub async fn execute_tool(&mut self, name: &str, params: serde_json::Value) -> Result<String> {
        let start = std::time::Instant::now();

        // Look up the tool in DB
        let tool = self
            .get_tool(name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Tool not found: {}", name))?;

        if tool.status == ToolStatus::Deprecated {
            return Err(anyhow::anyhow!("Tool '{}' is deprecated", name));
        }

        // Get or compile the AST
        let ast = if let Some(cached) = self.compiled_cache.get(name) {
            cached.clone()
        } else {
            let compiled = self
                .engine
                .compile(&tool.script_content)
                .map_err(|e| anyhow::anyhow!("Script compilation failed: {}", e))?;
            let arc = Arc::new(compiled);
            self.compiled_cache.insert(name.to_string(), arc.clone());
            arc
        };

        // Create a scope with the params variable
        let mut scope = rhai::Scope::new();
        let params_str = serde_json::to_string(&params)?;
        scope.push("params_json", params_str);

        // Execute the script inside block_in_place so that synchronous
        // blocking operations (e.g. reqwest::blocking in Rhai HTTP helpers)
        // do not panic when they create/drop their own tokio runtime.
        let result = tokio::task::block_in_place(|| {
            self.engine
                .eval_ast_with_scope::<rhai::Dynamic>(&mut scope, &ast)
        });

        let elapsed_ms = start.elapsed().as_millis() as i64;

        // Build execution record
        let (success, output, error_message) = match &result {
            Ok(val) => {
                let output_str = format!("{}", val);
                (true, Some(output_str), None)
            }
            Err(e) => (false, None, Some(format!("{}", e))),
        };

        // Log execution
        self.log_execution(
            &tool.id,
            success,
            elapsed_ms,
            &params,
            &output,
            &error_message,
        )
        .await?;

        // Update usage stats
        self.update_usage_stats(&tool.id, success).await?;

        match result {
            Ok(val) => Ok(format!("{}", val)),
            Err(e) => Err(anyhow::anyhow!("Script execution failed: {}", e)),
        }
    }

    /// List all tools with the given status filter. If `None`, lists all active tools.
    pub async fn list_tools(&self, status: Option<ToolStatus>) -> Result<Vec<ToolRecord>> {
        let status_filter = status.unwrap_or(ToolStatus::Active).to_string();

        let rows = sqlx::query(
            r#"
            SELECT id, name, description, version, script_content, parameters, status,
                   created_at, last_used, usage_count, success_count, failure_count, parent_tool_id
            FROM tools
            WHERE status = ?
            ORDER BY name
            "#,
        )
        .bind(&status_filter)
        .fetch_all(&self.db)
        .await
        .context("Failed to list tools")?;

        let tools = rows
            .into_iter()
            .map(|row| self.row_to_tool_record(row))
            .collect::<Result<Vec<_>>>()?;

        Ok(tools)
    }

    /// Get a single tool by name.
    pub async fn get_tool(&self, name: &str) -> Result<Option<ToolRecord>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, description, version, script_content, parameters, status,
                   created_at, last_used, usage_count, success_count, failure_count, parent_tool_id
            FROM tools
            WHERE name = ?
            "#,
        )
        .bind(name)
        .fetch_optional(&self.db)
        .await
        .context("Failed to get tool")?;

        match row {
            Some(row) => Ok(Some(self.row_to_tool_record(row)?)),
            None => Ok(None),
        }
    }

    /// Soft-delete a tool by setting its status to deprecated.
    /// Also removes it from the compilation cache.
    pub async fn delete_tool(&mut self, name: &str) -> Result<()> {
        sqlx::query("UPDATE tools SET status = 'deprecated' WHERE name = ?")
            .bind(name)
            .execute(&self.db)
            .await
            .context("Failed to deprecate tool")?;

        self.compiled_cache.remove(name);
        tracing::info!("Deprecated dynamic tool '{}'", name);
        Ok(())
    }

    /// Clear the compilation cache and force re-compilation on next use.
    pub fn clear_cache(&mut self) {
        self.compiled_cache.clear();
        tracing::debug!("Cleared Rhai AST compilation cache");
    }

    /// Update an existing tool's script content and optionally its description.
    /// Validates the new script, increments the version, updates the DB, and
    /// invalidates the compilation cache so the next execution uses the new code.
    pub async fn update_tool(
        &mut self,
        name: &str,
        script_content: &str,
        description: Option<&str>,
        parameters: Option<Vec<ParameterDef>>,
    ) -> Result<ToolRecord> {
        // Validate: compile the new script to check for syntax errors
        let ast = self
            .engine
            .compile(script_content)
            .map_err(|e| anyhow::anyhow!("Script compilation failed: {}", e))?;

        // Look up existing tool
        let existing = self
            .get_tool(name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Tool not found: {}", name))?;

        if existing.status == ToolStatus::Deprecated {
            return Err(anyhow::anyhow!(
                "Cannot update deprecated tool '{}'. Create a new tool instead.",
                name
            ));
        }

        // Increment version (e.g. "1.0.0" -> "1.1.0")
        let new_version = increment_version(&existing.version);

        // Build update query
        let new_description = description.unwrap_or(&existing.description);
        let new_params = match &parameters {
            Some(p) => serde_json::to_string(p).context("Failed to serialize parameters")?,
            None => serde_json::to_string(&existing.parameters)
                .context("Failed to serialize parameters")?,
        };

        sqlx::query(
            r#"
            UPDATE tools
            SET script_content = ?, description = ?, parameters = ?, version = ?
            WHERE name = ? AND status != 'deprecated'
            "#,
        )
        .bind(script_content)
        .bind(new_description)
        .bind(&new_params)
        .bind(&new_version)
        .bind(name)
        .execute(&self.db)
        .await
        .context("Failed to update tool in database")?;

        // Invalidate cache and store new AST
        self.compiled_cache.insert(name.to_string(), Arc::new(ast));

        tracing::info!("Updated dynamic tool '{}' to version {}", name, new_version);

        // Return updated record
        self.get_tool(name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Tool disappeared after update"))
    }

    /// Get a summary of available tool names and descriptions (for system prompt).
    pub async fn tool_summary(&self) -> Result<Vec<(String, String)>> {
        let rows = sqlx::query(
            "SELECT name, description FROM tools WHERE status = 'active' ORDER BY name",
        )
        .fetch_all(&self.db)
        .await
        .context("Failed to get tool summary")?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let name: String = row.get("name");
                let desc: String = row.get("description");
                (name, desc)
            })
            .collect())
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    fn row_to_tool_record(&self, row: sqlx::sqlite::SqliteRow) -> Result<ToolRecord> {
        let params_str: String = row.get("parameters");
        let parameters: Vec<ParameterDef> = serde_json::from_str(&params_str).unwrap_or_default();

        Ok(ToolRecord {
            id: row.get("id"),
            name: row.get("name"),
            description: row.get("description"),
            version: row.get("version"),
            script_content: row.get("script_content"),
            parameters,
            status: ToolStatus::from_str(row.get("status")),
            created_at: row.get("created_at"),
            last_used: row.get("last_used"),
            usage_count: row.get("usage_count"),
            success_count: row.get("success_count"),
            failure_count: row.get("failure_count"),
            parent_tool_id: row.get("parent_tool_id"),
        })
    }

    async fn log_execution(
        &self,
        tool_id: &str,
        success: bool,
        execution_time_ms: i64,
        input_params: &serde_json::Value,
        output: &Option<String>,
        error_message: &Option<String>,
    ) -> Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let params_json = serde_json::to_string(input_params)?;

        sqlx::query(
            r#"
            INSERT INTO tool_executions (id, tool_id, timestamp, success, execution_time_ms, error_message, input_params, output)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(tool_id)
        .bind(Utc::now())
        .bind(success as i32)
        .bind(execution_time_ms)
        .bind(error_message)
        .bind(&params_json)
        .bind(output)
        .execute(&self.db)
        .await
        .context("Failed to log tool execution")?;

        Ok(())
    }

    async fn update_usage_stats(&self, tool_id: &str, success: bool) -> Result<()> {
        if success {
            sqlx::query(
                "UPDATE tools SET usage_count = usage_count + 1, success_count = success_count + 1, last_used = ? WHERE id = ?",
            )
            .bind(Utc::now())
            .bind(tool_id)
            .execute(&self.db)
            .await
            .context("Failed to update tool usage stats")?;
        } else {
            sqlx::query(
                "UPDATE tools SET usage_count = usage_count + 1, failure_count = failure_count + 1, last_used = ? WHERE id = ?",
            )
            .bind(Utc::now())
            .bind(tool_id)
            .execute(&self.db)
            .await
            .context("Failed to update tool usage stats")?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    /// Create an in-memory SQLite pool for testing.
    async fn test_db() -> Pool<Sqlite> {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory database");

        // Create tables
        crate::database::schema::create_tables(&pool)
            .await
            .expect("Failed to create tables");

        pool
    }

    #[tokio::test]
    async fn test_register_tool() {
        let db = test_db().await;
        let mut registry = RhaiToolRegistry::new(db, PathBuf::from("/tmp"));

        let tool = registry
            .register_tool(
                "greet",
                "Returns a greeting",
                r#"let name = "World"; "Hello, " + name + "!""#,
                vec![],
            )
            .await
            .unwrap();

        assert_eq!(tool.name, "greet");
        assert_eq!(tool.status, ToolStatus::Active);
        assert_eq!(tool.version, "1.0.0");
    }

    #[tokio::test]
    async fn test_register_invalid_script() {
        let db = test_db().await;
        let mut registry = RhaiToolRegistry::new(db, PathBuf::from("/tmp"));

        let result = registry
            .register_tool("bad", "A broken tool", "let x = ;; invalid", vec![])
            .await;

        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_execute_tool() {
        let db = test_db().await;
        let mut registry = RhaiToolRegistry::new(db, PathBuf::from("/tmp"));

        registry
            .register_tool("add", "Adds two numbers", "40 + 2", vec![])
            .await
            .unwrap();

        let result = registry
            .execute_tool("add", serde_json::json!({}))
            .await
            .unwrap();

        assert_eq!(result, "42");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_execute_tool_with_params() {
        let db = test_db().await;
        let mut registry = RhaiToolRegistry::new(db, PathBuf::from("/tmp"));

        let script = r#"
            let params = json_parse(params_json);
            let a = params["a"];
            let b = params["b"];
            a + b
        "#;

        registry
            .register_tool("add_params", "Add two numbers from params", script, vec![])
            .await
            .unwrap();

        let result = registry
            .execute_tool("add_params", serde_json::json!({"a": 10, "b": 32}))
            .await
            .unwrap();

        assert_eq!(result, "42");
    }

    #[tokio::test]
    async fn test_execute_nonexistent_tool() {
        let db = test_db().await;
        let mut registry = RhaiToolRegistry::new(db, PathBuf::from("/tmp"));

        let result = registry
            .execute_tool("nonexistent", serde_json::json!({}))
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_tools() {
        let db = test_db().await;
        let mut registry = RhaiToolRegistry::new(db, PathBuf::from("/tmp"));

        registry
            .register_tool("tool_a", "First tool", "42", vec![])
            .await
            .unwrap();
        registry
            .register_tool("tool_b", "Second tool", "43", vec![])
            .await
            .unwrap();

        let tools = registry.list_tools(None).await.unwrap();
        assert_eq!(tools.len(), 2);
    }

    #[tokio::test]
    async fn test_get_tool() {
        let db = test_db().await;
        let mut registry = RhaiToolRegistry::new(db, PathBuf::from("/tmp"));

        registry
            .register_tool("my_tool", "A tool", "1 + 1", vec![])
            .await
            .unwrap();

        let tool = registry.get_tool("my_tool").await.unwrap();
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().name, "my_tool");

        let missing = registry.get_tool("no_such_tool").await.unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_delete_tool() {
        let db = test_db().await;
        let mut registry = RhaiToolRegistry::new(db, PathBuf::from("/tmp"));

        registry
            .register_tool("to_delete", "Will be deleted", "0", vec![])
            .await
            .unwrap();

        registry.delete_tool("to_delete").await.unwrap();

        // Should no longer appear in active tools
        let active = registry.list_tools(None).await.unwrap();
        assert!(active.is_empty());

        // Should appear in deprecated list
        let deprecated = registry
            .list_tools(Some(ToolStatus::Deprecated))
            .await
            .unwrap();
        assert_eq!(deprecated.len(), 1);
    }

    #[tokio::test]
    async fn test_execute_deprecated_tool() {
        let db = test_db().await;
        let mut registry = RhaiToolRegistry::new(db, PathBuf::from("/tmp"));

        registry
            .register_tool("old_tool", "Deprecated", "0", vec![])
            .await
            .unwrap();

        registry.delete_tool("old_tool").await.unwrap();

        let result = registry
            .execute_tool("old_tool", serde_json::json!({}))
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("deprecated"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_usage_stats_updated() {
        let db = test_db().await;
        let mut registry = RhaiToolRegistry::new(db, PathBuf::from("/tmp"));

        registry
            .register_tool("counter", "Counting tool", "42", vec![])
            .await
            .unwrap();

        // Execute twice
        registry
            .execute_tool("counter", serde_json::json!({}))
            .await
            .unwrap();
        registry
            .execute_tool("counter", serde_json::json!({}))
            .await
            .unwrap();

        let tool = registry.get_tool("counter").await.unwrap().unwrap();
        assert_eq!(tool.usage_count, 2);
        assert_eq!(tool.success_count, 2);
        assert!(tool.last_used.is_some());
    }

    #[tokio::test]
    async fn test_tool_summary() {
        let db = test_db().await;
        let mut registry = RhaiToolRegistry::new(db, PathBuf::from("/tmp"));

        registry
            .register_tool("alpha", "First tool", "1", vec![])
            .await
            .unwrap();
        registry
            .register_tool("beta", "Second tool", "2", vec![])
            .await
            .unwrap();

        let summary = registry.tool_summary().await.unwrap();
        assert_eq!(summary.len(), 2);
        assert_eq!(summary[0].0, "alpha");
        assert_eq!(summary[1].0, "beta");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_clear_cache() {
        let db = test_db().await;
        let mut registry = RhaiToolRegistry::new(db, PathBuf::from("/tmp"));

        registry
            .register_tool("cached", "Cached tool", "42", vec![])
            .await
            .unwrap();

        assert!(registry.compiled_cache.contains_key("cached"));

        registry.clear_cache();
        assert!(registry.compiled_cache.is_empty());

        // Should still work after cache clear (re-compiles from DB)
        let result = registry
            .execute_tool("cached", serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(result, "42");
    }
}
