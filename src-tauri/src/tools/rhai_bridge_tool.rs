//! RhaiExecuteTool -- rig Tool implementation that executes dynamic Rhai scripts.
//!
//! This tool is registered with the rig-core agent and allows the LLM to invoke
//! any dynamic tool from the RhaiToolRegistry by name. It acts as the bridge
//! between the LLM's tool-calling capability and the Rhai scripting engine.

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::registry::RhaiToolRegistry;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct RhaiToolError(String);

// ---------------------------------------------------------------------------
// Tool arguments
// ---------------------------------------------------------------------------

/// Arguments the LLM passes when invoking a dynamic tool.
#[derive(Debug, Deserialize)]
pub struct ExecuteDynamicToolArgs {
    /// Name of the registered dynamic tool to execute.
    tool_name: String,
    /// JSON object of parameters to pass to the tool script.
    #[serde(default = "default_params")]
    parameters: serde_json::Value,
}

fn default_params() -> serde_json::Value {
    serde_json::json!({})
}

// ---------------------------------------------------------------------------
// RhaiExecuteTool
// ---------------------------------------------------------------------------

/// A shared reference to the tool registry, safe for concurrent access.
pub type SharedRegistry = Arc<RwLock<RhaiToolRegistry>>;

/// rig Tool that delegates execution to the Rhai tool registry.
///
/// When the LLM calls this tool, it specifies a `tool_name` and `parameters`.
/// The tool looks up the named Rhai script in the registry and executes it.
#[derive(Clone, Serialize, Deserialize)]
pub struct RhaiExecuteTool {
    /// Available tool names and descriptions, cached for the tool definition.
    /// Updated when the agent is created.
    #[serde(default)]
    available_tools: Vec<(String, String)>,

    /// The shared registry reference (skipped during serialization).
    #[serde(skip)]
    registry: Option<SharedRegistry>,
}

impl RhaiExecuteTool {
    /// Create a new RhaiExecuteTool with a reference to the shared registry.
    pub fn new(registry: SharedRegistry, available_tools: Vec<(String, String)>) -> Self {
        Self {
            available_tools,
            registry: Some(registry),
        }
    }
}

impl Tool for RhaiExecuteTool {
    const NAME: &'static str = "execute_dynamic_tool";
    type Error = RhaiToolError;
    type Args = ExecuteDynamicToolArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        // Build a description that lists available tools
        let mut description = String::from(
            "Execute a dynamic tool by name. These are custom tools that have been \
             created to extend the agent's capabilities. Pass the tool_name and any \
             required parameters as a JSON object.\n\n\
             Available dynamic tools:\n",
        );

        if self.available_tools.is_empty() {
            description.push_str("  (no dynamic tools registered yet)\n");
        } else {
            for (name, desc) in &self.available_tools {
                description.push_str(&format!("  - {}: {}\n", name, desc));
            }
        }

        ToolDefinition {
            name: "execute_dynamic_tool".to_string(),
            description,
            parameters: json!({
                "type": "object",
                "properties": {
                    "tool_name": {
                        "type": "string",
                        "description": "Name of the dynamic tool to execute"
                    },
                    "parameters": {
                        "type": "object",
                        "description": "JSON object with parameters for the tool"
                    }
                },
                "required": ["tool_name"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let registry = self
            .registry
            .as_ref()
            .ok_or_else(|| RhaiToolError("Tool registry not initialized".to_string()))?;

        let mut registry_guard = registry.write().await;

        tracing::info!(
            "Executing dynamic tool '{}' with params: {}",
            args.tool_name,
            args.parameters
        );

        let result = registry_guard
            .execute_tool(&args.tool_name, args.parameters)
            .await
            .map_err(|e| RhaiToolError(format!("Dynamic tool execution failed: {}", e)))?;

        tracing::info!("Dynamic tool '{}' returned: {}", args.tool_name, result);

        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::path::PathBuf;

    async fn test_registry() -> SharedRegistry {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory database");

        crate::database::schema::create_tables(&pool)
            .await
            .expect("Failed to create tables");

        let mut registry = RhaiToolRegistry::new(pool, PathBuf::from("/tmp"));

        // Register a test tool
        registry
            .register_tool("test_add", "Adds 1 + 1", "1 + 1", vec![])
            .await
            .unwrap();

        Arc::new(RwLock::new(registry))
    }

    #[tokio::test]
    async fn test_execute_dynamic_tool() {
        let registry = test_registry().await;
        let tool = RhaiExecuteTool::new(registry, vec![("test_add".into(), "Adds 1 + 1".into())]);

        let result = tool
            .call(ExecuteDynamicToolArgs {
                tool_name: "test_add".to_string(),
                parameters: json!({}),
            })
            .await
            .unwrap();

        assert_eq!(result, "2");
    }

    #[tokio::test]
    async fn test_execute_nonexistent_dynamic_tool() {
        let registry = test_registry().await;
        let tool = RhaiExecuteTool::new(registry, vec![]);

        let result = tool
            .call(ExecuteDynamicToolArgs {
                tool_name: "no_such_tool".to_string(),
                parameters: json!({}),
            })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_tool_definition_lists_available_tools() {
        let registry = test_registry().await;
        let tool = RhaiExecuteTool::new(
            registry,
            vec![
                ("alpha".into(), "First tool".into()),
                ("beta".into(), "Second tool".into()),
            ],
        );

        let def = tool.definition("test".to_string()).await;
        assert!(def.description.contains("alpha"));
        assert!(def.description.contains("beta"));
    }

    #[tokio::test]
    async fn test_tool_definition_empty_registry() {
        let registry = test_registry().await;
        let tool = RhaiExecuteTool::new(registry, vec![]);

        let def = tool.definition("test".to_string()).await;
        assert!(def.description.contains("no dynamic tools registered yet"));
    }
}
