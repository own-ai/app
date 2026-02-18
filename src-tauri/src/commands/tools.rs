//! Tauri commands for dynamic tool management (Rhai tools).
//!
//! All commands access the per-instance tool registry through the AgentCache,
//! ensuring each AI instance has its own isolated set of dynamic tools.

use serde::{Deserialize, Serialize};
use tauri::State;

use super::chat::AgentCache;
use crate::tools::registry::ParameterDef;

/// Serializable tool info for the frontend.
#[derive(Debug, Serialize, Deserialize)]
pub struct ToolInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub status: String,
    pub usage_count: i32,
    pub success_count: i32,
    pub failure_count: i32,
    pub parameters: Vec<ParameterDef>,
    pub created_at: String,
    pub last_used: Option<String>,
}

/// List all active dynamic tools for an instance.
#[tauri::command]
pub async fn list_dynamic_tools(
    instance_id: String,
    agent_cache: State<'_, AgentCache>,
) -> Result<Vec<ToolInfo>, String> {
    let cache = agent_cache.lock().await;

    let agent = cache
        .get(&instance_id)
        .ok_or_else(|| "Agent not in cache - please send a message first".to_string())?;

    let registry = agent.tool_registry().read().await;
    let tools = registry
        .list_tools(None)
        .await
        .map_err(|e| format!("Failed to list tools: {}", e))?;

    Ok(tools
        .into_iter()
        .map(|t| ToolInfo {
            id: t.id,
            name: t.name,
            description: t.description,
            version: t.version,
            status: t.status.to_string(),
            usage_count: t.usage_count,
            success_count: t.success_count,
            failure_count: t.failure_count,
            parameters: t.parameters,
            created_at: t.created_at.to_rfc3339(),
            last_used: t.last_used.map(|d| d.to_rfc3339()),
        })
        .collect())
}

/// Create a new dynamic tool with a Rhai script.
#[tauri::command]
pub async fn create_dynamic_tool(
    instance_id: String,
    name: String,
    description: String,
    script_content: String,
    parameters: Vec<ParameterDef>,
    agent_cache: State<'_, AgentCache>,
) -> Result<ToolInfo, String> {
    let cache = agent_cache.lock().await;

    let agent = cache
        .get(&instance_id)
        .ok_or_else(|| "Agent not in cache - please send a message first".to_string())?;

    let mut registry = agent.tool_registry().write().await;
    let tool = registry
        .register_tool(&name, &description, &script_content, parameters)
        .await
        .map_err(|e| format!("Failed to create tool: {}", e))?;

    tracing::info!(
        "Created dynamic tool '{}' for instance {}",
        name,
        instance_id
    );

    Ok(ToolInfo {
        id: tool.id,
        name: tool.name,
        description: tool.description,
        version: tool.version,
        status: tool.status.to_string(),
        usage_count: tool.usage_count,
        success_count: tool.success_count,
        failure_count: tool.failure_count,
        parameters: tool.parameters,
        created_at: tool.created_at.to_rfc3339(),
        last_used: tool.last_used.map(|d| d.to_rfc3339()),
    })
}

/// Delete (deprecate) a dynamic tool.
#[tauri::command]
pub async fn delete_dynamic_tool(
    instance_id: String,
    name: String,
    agent_cache: State<'_, AgentCache>,
) -> Result<(), String> {
    let cache = agent_cache.lock().await;

    let agent = cache
        .get(&instance_id)
        .ok_or_else(|| "Agent not in cache - please send a message first".to_string())?;

    let mut registry = agent.tool_registry().write().await;
    registry
        .delete_tool(&name)
        .await
        .map_err(|e| format!("Failed to delete tool: {}", e))?;

    tracing::info!(
        "Deleted dynamic tool '{}' for instance {}",
        name,
        instance_id
    );

    Ok(())
}

/// Execute a dynamic tool with the given parameters.
#[tauri::command]
pub async fn execute_dynamic_tool(
    instance_id: String,
    name: String,
    params: serde_json::Value,
    agent_cache: State<'_, AgentCache>,
) -> Result<String, String> {
    let cache = agent_cache.lock().await;

    let agent = cache
        .get(&instance_id)
        .ok_or_else(|| "Agent not in cache - please send a message first".to_string())?;

    let mut registry = agent.tool_registry().write().await;
    registry
        .execute_tool(&name, params)
        .await
        .map_err(|e| format!("Tool execution failed: {}", e))
}
