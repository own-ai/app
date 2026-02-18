//! Code generation tools for self-programming.
//!
//! Provides three rig Tools that allow the agent to manage dynamic Rhai tools:
//! - `CreateToolTool`: Create a new Rhai tool from code
//! - `ReadToolTool`: Read the source code of an existing tool
//! - `UpdateToolTool`: Update/fix an existing tool's code
//!
//! The agent writes the Rhai code itself and uses these tools to register,
//! inspect, and iterate on dynamic tools stored in the Tool Registry.

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};

use super::registry::ParameterDef;
use super::rhai_bridge_tool::SharedRegistry;
use super::rhai_engine::create_sandboxed_engine;

// ---------------------------------------------------------------------------
// Script validation
// ---------------------------------------------------------------------------

/// Validate a Rhai script by compiling it in a sandboxed engine.
///
/// Returns `Ok(warnings)` where `warnings` is a (possibly empty) list of
/// advisory messages. Returns `Err` if the script fails to compile.
pub fn validate_script(script: &str, workspace: &Path) -> Result<Vec<String>, String> {
    let engine = create_sandboxed_engine(workspace.to_path_buf());
    engine
        .compile(script)
        .map_err(|e| format!("Compilation error: {}", e))?;

    let mut warnings = Vec::new();

    // Warn about potential infinite loops (heuristic)
    if script.contains("loop {") && !script.contains("break") {
        warnings.push(
            "Script contains 'loop' without 'break'. \
             It will be terminated after max operations limit."
                .to_string(),
        );
    }

    Ok(warnings)
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct CodeGenError(String);

// ---------------------------------------------------------------------------
// CreateToolTool
// ---------------------------------------------------------------------------

/// Arguments for creating a new dynamic tool.
#[derive(Debug, Deserialize)]
pub struct CreateToolArgs {
    /// A unique, snake_case name for the tool.
    name: String,
    /// A human-readable description of what the tool does.
    description: String,
    /// The Rhai script source code.
    script_content: String,
    /// Optional parameter definitions for the tool.
    #[serde(default)]
    parameters: Vec<ParameterDefArg>,
}

/// Parameter definition as provided by the LLM.
#[derive(Debug, Deserialize)]
pub struct ParameterDefArg {
    pub name: String,
    #[serde(default = "default_type_hint")]
    pub type_hint: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_required")]
    pub required: bool,
}

fn default_type_hint() -> String {
    "string".to_string()
}

fn default_required() -> bool {
    true
}

/// rig Tool that creates a new dynamic Rhai tool in the registry.
#[derive(Clone, Serialize, Deserialize)]
pub struct CreateToolTool {
    #[serde(skip)]
    registry: Option<SharedRegistry>,
    #[serde(skip)]
    workspace: Option<PathBuf>,
}

impl CreateToolTool {
    pub fn new(registry: SharedRegistry, workspace: PathBuf) -> Self {
        Self {
            registry: Some(registry),
            workspace: Some(workspace),
        }
    }
}

impl Tool for CreateToolTool {
    const NAME: &'static str = "create_tool";
    type Error = CodeGenError;
    type Args = CreateToolArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "create_tool".to_string(),
            description: "Create a new dynamic tool by providing a Rhai script. \
                The script will be validated (compiled) before registration. \
                Once created, the tool can be executed via execute_dynamic_tool. \
                Scripts receive parameters through the `params_json` variable \
                (use `let params = json_parse(params_json);` to access them)."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Unique snake_case name for the tool (e.g. 'fetch_weather', 'calculate_tax')"
                    },
                    "description": {
                        "type": "string",
                        "description": "Human-readable description of what the tool does"
                    },
                    "script_content": {
                        "type": "string",
                        "description": "The Rhai script source code. Use params_json to receive parameters."
                    },
                    "parameters": {
                        "type": "array",
                        "description": "Parameter definitions for the tool",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string" },
                                "type_hint": { "type": "string", "description": "e.g. 'string', 'number', 'boolean'" },
                                "description": { "type": "string" },
                                "required": { "type": "boolean" }
                            },
                            "required": ["name"]
                        }
                    }
                },
                "required": ["name", "description", "script_content"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let registry = self
            .registry
            .as_ref()
            .ok_or_else(|| CodeGenError("Tool registry not initialized".to_string()))?;

        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| CodeGenError("Workspace not initialized".to_string()))?;

        // Validate the script first
        let warnings = validate_script(&args.script_content, workspace)
            .map_err(|e| CodeGenError(format!("Script validation failed: {}", e)))?;

        // Convert parameter definitions
        let params: Vec<ParameterDef> = args
            .parameters
            .into_iter()
            .map(|p| ParameterDef {
                name: p.name,
                type_hint: p.type_hint,
                description: p.description,
                required: p.required,
            })
            .collect();

        // Register in the registry
        let mut registry_guard = registry.write().await;
        let tool = registry_guard
            .register_tool(&args.name, &args.description, &args.script_content, params)
            .await
            .map_err(|e| CodeGenError(format!("Failed to register tool: {}", e)))?;

        let mut result = format!(
            "Tool '{}' created successfully (version {}).\n\
             You can now use it via execute_dynamic_tool with tool_name='{}'.",
            tool.name, tool.version, tool.name
        );

        if !warnings.is_empty() {
            result.push_str("\n\nWarnings:\n");
            for w in &warnings {
                result.push_str(&format!("- {}\n", w));
            }
        }

        tracing::info!("Agent created dynamic tool '{}'", tool.name);
        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// ReadToolTool
// ---------------------------------------------------------------------------

/// Arguments for reading an existing tool's source code.
#[derive(Debug, Deserialize)]
pub struct ReadToolArgs {
    /// Name of the tool to read.
    tool_name: String,
}

/// rig Tool that reads the source code and metadata of an existing dynamic tool.
#[derive(Clone, Serialize, Deserialize)]
pub struct ReadToolTool {
    #[serde(skip)]
    registry: Option<SharedRegistry>,
}

impl ReadToolTool {
    pub fn new(registry: SharedRegistry) -> Self {
        Self {
            registry: Some(registry),
        }
    }
}

impl Tool for ReadToolTool {
    const NAME: &'static str = "read_tool";
    type Error = CodeGenError;
    type Args = ReadToolArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "read_tool".to_string(),
            description: "Read the source code, metadata, and usage statistics of an existing \
                dynamic tool. Use this to inspect a tool before updating it."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "tool_name": {
                        "type": "string",
                        "description": "Name of the dynamic tool to read"
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
            .ok_or_else(|| CodeGenError("Tool registry not initialized".to_string()))?;

        let registry_guard = registry.read().await;
        let tool = registry_guard
            .get_tool(&args.tool_name)
            .await
            .map_err(|e| CodeGenError(format!("Failed to read tool: {}", e)))?
            .ok_or_else(|| CodeGenError(format!("Tool '{}' not found", args.tool_name)))?;

        // Format comprehensive output
        let params_info = if tool.parameters.is_empty() {
            "  (none)".to_string()
        } else {
            tool.parameters
                .iter()
                .map(|p| {
                    format!(
                        "  - {} ({}{}): {}",
                        p.name,
                        p.type_hint,
                        if p.required { ", required" } else { "" },
                        p.description
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        let output = format!(
            "Tool: {name}\n\
             Description: {desc}\n\
             Version: {version}\n\
             Status: {status}\n\
             Usage: {usage} calls ({success} successful, {failure} failed)\n\
             \n\
             Parameters:\n\
             {params}\n\
             \n\
             Script:\n\
             ```rhai\n\
             {script}\n\
             ```",
            name = tool.name,
            desc = tool.description,
            version = tool.version,
            status = tool.status,
            usage = tool.usage_count,
            success = tool.success_count,
            failure = tool.failure_count,
            params = params_info,
            script = tool.script_content,
        );

        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// UpdateToolTool
// ---------------------------------------------------------------------------

/// Arguments for updating an existing tool's source code.
#[derive(Debug, Deserialize)]
pub struct UpdateToolArgs {
    /// Name of the tool to update.
    tool_name: String,
    /// The new Rhai script source code.
    script_content: String,
    /// Optional updated description. If omitted, the existing description is kept.
    #[serde(default)]
    description: Option<String>,
    /// Optional updated parameter definitions.
    #[serde(default)]
    parameters: Option<Vec<ParameterDefArg>>,
}

/// rig Tool that updates an existing dynamic tool's code and metadata.
#[derive(Clone, Serialize, Deserialize)]
pub struct UpdateToolTool {
    #[serde(skip)]
    registry: Option<SharedRegistry>,
    #[serde(skip)]
    workspace: Option<PathBuf>,
}

impl UpdateToolTool {
    pub fn new(registry: SharedRegistry, workspace: PathBuf) -> Self {
        Self {
            registry: Some(registry),
            workspace: Some(workspace),
        }
    }
}

impl Tool for UpdateToolTool {
    const NAME: &'static str = "update_tool";
    type Error = CodeGenError;
    type Args = UpdateToolArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "update_tool".to_string(),
            description: "Update an existing dynamic tool's Rhai script code. Use this to fix \
                bugs, improve functionality, or extend capabilities. The script will be \
                validated before the update is applied. The version number is incremented \
                automatically."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "tool_name": {
                        "type": "string",
                        "description": "Name of the existing tool to update"
                    },
                    "script_content": {
                        "type": "string",
                        "description": "The new Rhai script source code"
                    },
                    "description": {
                        "type": "string",
                        "description": "Updated description (optional, keeps existing if omitted)"
                    },
                    "parameters": {
                        "type": "array",
                        "description": "Updated parameter definitions (optional)",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string" },
                                "type_hint": { "type": "string" },
                                "description": { "type": "string" },
                                "required": { "type": "boolean" }
                            },
                            "required": ["name"]
                        }
                    }
                },
                "required": ["tool_name", "script_content"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let registry = self
            .registry
            .as_ref()
            .ok_or_else(|| CodeGenError("Tool registry not initialized".to_string()))?;

        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| CodeGenError("Workspace not initialized".to_string()))?;

        // Validate the new script first
        let warnings = validate_script(&args.script_content, workspace)
            .map_err(|e| CodeGenError(format!("Script validation failed: {}", e)))?;

        // Convert optional parameter definitions
        let params: Option<Vec<ParameterDef>> = args.parameters.map(|p_args| {
            p_args
                .into_iter()
                .map(|p| ParameterDef {
                    name: p.name,
                    type_hint: p.type_hint,
                    description: p.description,
                    required: p.required,
                })
                .collect()
        });

        // Update in the registry
        let mut registry_guard = registry.write().await;
        let tool = registry_guard
            .update_tool(
                &args.tool_name,
                &args.script_content,
                args.description.as_deref(),
                params,
            )
            .await
            .map_err(|e| CodeGenError(format!("Failed to update tool: {}", e)))?;

        let mut result = format!(
            "Tool '{}' updated successfully to version {}.\n\
             You can test it via execute_dynamic_tool with tool_name='{}'.",
            tool.name, tool.version, tool.name
        );

        if !warnings.is_empty() {
            result.push_str("\n\nWarnings:\n");
            for w in &warnings {
                result.push_str(&format!("- {}\n", w));
            }
        }

        tracing::info!(
            "Agent updated dynamic tool '{}' to version {}",
            tool.name,
            tool.version
        );
        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::registry::RhaiToolRegistry;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::{Pool, Sqlite};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    async fn test_db() -> Pool<Sqlite> {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory database");

        crate::database::schema::create_tables(&pool)
            .await
            .expect("Failed to create tables");

        pool
    }

    fn test_workspace() -> PathBuf {
        PathBuf::from("/tmp/test_workspace")
    }

    async fn test_registry() -> SharedRegistry {
        let db = test_db().await;
        let registry = RhaiToolRegistry::new(db, test_workspace());
        Arc::new(RwLock::new(registry))
    }

    // -- validate_script tests --

    #[test]
    fn test_validate_valid_script() {
        let ws = test_workspace();
        let result = validate_script("let x = 42; x + 1", &ws);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_validate_invalid_script() {
        let ws = test_workspace();
        let result = validate_script("let x = ;; invalid", &ws);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Compilation error"));
    }

    #[test]
    fn test_validate_script_with_loop_warning() {
        let ws = test_workspace();
        let result = validate_script("loop { let x = 1; }", &ws);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("loop"));
    }

    #[test]
    fn test_validate_script_loop_with_break_no_warning() {
        let ws = test_workspace();
        let result = validate_script("let i = 0; loop { i += 1; if i > 10 { break; } }", &ws);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    // -- CreateToolTool tests --

    #[tokio::test]
    async fn test_create_tool_success() {
        let registry = test_registry().await;
        let tool = CreateToolTool::new(registry.clone(), test_workspace());

        let result = tool
            .call(CreateToolArgs {
                name: "my_adder".to_string(),
                description: "Adds two numbers".to_string(),
                script_content: r#"
                    let params = json_parse(params_json);
                    params["a"] + params["b"]
                "#
                .to_string(),
                parameters: vec![
                    ParameterDefArg {
                        name: "a".to_string(),
                        type_hint: "number".to_string(),
                        description: "First number".to_string(),
                        required: true,
                    },
                    ParameterDefArg {
                        name: "b".to_string(),
                        type_hint: "number".to_string(),
                        description: "Second number".to_string(),
                        required: true,
                    },
                ],
            })
            .await
            .unwrap();

        assert!(result.contains("created successfully"));
        assert!(result.contains("my_adder"));

        // Verify it exists in registry
        let guard = registry.read().await;
        let stored = guard.get_tool("my_adder").await.unwrap();
        assert!(stored.is_some());
    }

    #[tokio::test]
    async fn test_create_tool_invalid_script() {
        let registry = test_registry().await;
        let tool = CreateToolTool::new(registry, test_workspace());

        let result = tool
            .call(CreateToolArgs {
                name: "bad_tool".to_string(),
                description: "Will fail".to_string(),
                script_content: "let x = ;; broken".to_string(),
                parameters: vec![],
            })
            .await;

        assert!(result.is_err());
    }

    // -- ReadToolTool tests --

    #[tokio::test]
    async fn test_read_tool_success() {
        let registry = test_registry().await;

        // Create a tool first
        {
            let mut guard = registry.write().await;
            guard
                .register_tool("readable", "A readable tool", "40 + 2", vec![])
                .await
                .unwrap();
        }

        let tool = ReadToolTool::new(registry);
        let result = tool
            .call(ReadToolArgs {
                tool_name: "readable".to_string(),
            })
            .await
            .unwrap();

        assert!(result.contains("Tool: readable"));
        assert!(result.contains("A readable tool"));
        assert!(result.contains("40 + 2"));
        assert!(result.contains("Version: 1.0.0"));
    }

    #[tokio::test]
    async fn test_read_tool_not_found() {
        let registry = test_registry().await;
        let tool = ReadToolTool::new(registry);

        let result = tool
            .call(ReadToolArgs {
                tool_name: "nonexistent".to_string(),
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    // -- UpdateToolTool tests --

    #[tokio::test]
    async fn test_update_tool_success() {
        let registry = test_registry().await;

        // Create a tool first
        {
            let mut guard = registry.write().await;
            guard
                .register_tool("updatable", "Original description", "1 + 1", vec![])
                .await
                .unwrap();
        }

        let tool = UpdateToolTool::new(registry.clone(), test_workspace());
        let result = tool
            .call(UpdateToolArgs {
                tool_name: "updatable".to_string(),
                script_content: "2 + 2".to_string(),
                description: Some("Updated description".to_string()),
                parameters: None,
            })
            .await
            .unwrap();

        assert!(result.contains("updated successfully"));
        assert!(result.contains("1.1.0"));

        // Verify the update
        let guard = registry.read().await;
        let stored = guard.get_tool("updatable").await.unwrap().unwrap();
        assert_eq!(stored.script_content, "2 + 2");
        assert_eq!(stored.description, "Updated description");
        assert_eq!(stored.version, "1.1.0");
    }

    #[tokio::test]
    async fn test_update_tool_invalid_script() {
        let registry = test_registry().await;

        {
            let mut guard = registry.write().await;
            guard
                .register_tool("will_fail_update", "A tool", "42", vec![])
                .await
                .unwrap();
        }

        let tool = UpdateToolTool::new(registry, test_workspace());
        let result = tool
            .call(UpdateToolArgs {
                tool_name: "will_fail_update".to_string(),
                script_content: "let x = ;; broken".to_string(),
                description: None,
                parameters: None,
            })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_nonexistent_tool() {
        let registry = test_registry().await;
        let tool = UpdateToolTool::new(registry, test_workspace());

        let result = tool
            .call(UpdateToolArgs {
                tool_name: "ghost".to_string(),
                script_content: "42".to_string(),
                description: None,
                parameters: None,
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}
