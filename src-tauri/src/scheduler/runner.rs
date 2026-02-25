//! Task execution logic for scheduled tasks.
//!
//! When a cron job fires, this module creates a temporary agent (similar to
//! sub-agents) that executes the task prompt and returns the result.

use anyhow::Result;
use rig::client::{CompletionClient, Nothing};
use rig::completion::Prompt;
use rig::providers::{anthropic, ollama, openai};
use sqlx::{Pool, Sqlite};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

use crate::ai_instances::{AIInstance, AIInstanceManager, APIKeyStorage, LLMProvider};
use crate::database::init_database;
use crate::memory::{LongTermMemory, SharedLongTermMemory};
use crate::tools::registry::RhaiToolRegistry;
use crate::tools::rhai_bridge_tool::SharedRegistry;
use crate::tools::subagents::{base_tools_prompt, build_sub_agent_tools};
use crate::utils::paths;

use super::storage;
use super::SharedScheduler;

/// Maximum number of multi-turn iterations for scheduled task agents.
const TASK_AGENT_MAX_TURNS: usize = 25;

/// Register a scheduled task as a cron job in the scheduler.
///
/// The job closure captures all necessary context to create a temporary agent
/// when the cron expression triggers.
#[allow(clippy::too_many_arguments)]
pub async fn register_task_job(
    scheduler: &SharedScheduler,
    task_id: String,
    cron_expression: &str,
    task_name: String,
    task_prompt: String,
    instance_id: String,
    manager: Arc<Mutex<AIInstanceManager>>,
    app_handle: AppHandle,
) -> Result<()> {
    let task_id_for_closure = task_id.clone();
    let manager_clone = manager.clone();
    let app_handle_clone = app_handle.clone();

    let mut sched = scheduler.lock().await;
    sched
        .add_job(&task_id, cron_expression, move |_uuid, _scheduler| {
            let task_id = task_id_for_closure.clone();
            let task_name = task_name.clone();
            let task_prompt = task_prompt.clone();
            let instance_id = instance_id.clone();
            let manager = manager_clone.clone();
            let app_handle = app_handle_clone.clone();

            Box::pin(async move {
                tracing::info!(
                    "Scheduled task '{}' ({}) firing for instance '{}'",
                    task_name,
                    task_id,
                    instance_id
                );

                match execute_task(&instance_id, &task_prompt, &manager, &app_handle).await {
                    Ok(result) => {
                        tracing::info!(
                            "Scheduled task '{}' completed (result length: {} chars)",
                            task_name,
                            result.len()
                        );

                        // Update last_run in database
                        if let Ok(db) = init_database(&instance_id).await {
                            let truncated = if result.len() > 2000 {
                                format!("{}...", &result[..2000])
                            } else {
                                result.clone()
                            };
                            if let Err(e) =
                                storage::update_task_last_run(&db, &task_id, &truncated).await
                            {
                                tracing::warn!("Failed to update task last_run: {}", e);
                            }
                        }

                        // Emit event to frontend
                        let payload = serde_json::json!({
                            "task_id": task_id,
                            "task_name": task_name,
                            "instance_id": instance_id,
                            "result": result,
                        });
                        if let Err(e) = app_handle.emit("scheduler:task_completed", payload) {
                            tracing::warn!("Failed to emit task_completed event: {}", e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Scheduled task '{}' failed: {}", task_name, e);

                        // Update last_run with error
                        if let Ok(db) = init_database(&instance_id).await {
                            let error_msg = format!("Error: {}", e);
                            let _ = storage::update_task_last_run(&db, &task_id, &error_msg).await;
                        }

                        // Emit error event
                        let payload = serde_json::json!({
                            "task_id": task_id,
                            "task_name": task_name,
                            "instance_id": instance_id,
                            "error": e.to_string(),
                        });
                        if let Err(e) = app_handle.emit("scheduler:task_failed", payload) {
                            tracing::warn!("Failed to emit task_failed event: {}", e);
                        }
                    }
                }
            })
        })
        .await?;

    Ok(())
}

/// Execute a scheduled task by creating a temporary agent and running the prompt.
///
/// This is similar to how `DelegateTaskTool` runs sub-agents: a lightweight
/// agent is created with tools but without the full memory/extraction overhead.
async fn execute_task(
    instance_id: &str,
    task_prompt: &str,
    manager: &Arc<Mutex<AIInstanceManager>>,
    app_handle: &AppHandle,
) -> Result<String> {
    // 1. Get instance configuration
    let instance = {
        let mgr = manager.lock().await;
        mgr.get_instance(instance_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Instance not found: {}", instance_id))?
    };

    // 2. Load API key
    let api_key = if instance.provider.needs_api_key() {
        APIKeyStorage::load(&instance.provider)?.ok_or_else(|| {
            anyhow::anyhow!("API key not found for provider: {}", instance.provider)
        })?
    } else {
        String::new()
    };

    // 3. Initialize instance database
    let db = init_database(instance_id).await?;

    // 4. Build tools (same as sub-agents, without delegate_task)
    let workspace =
        paths::get_instance_workspace_path(instance_id).unwrap_or_else(|_| PathBuf::from("."));
    let programs_root = paths::get_instance_programs_path(instance_id)
        .unwrap_or_else(|_| PathBuf::from("./programs"));

    let rhai_registry = RhaiToolRegistry::new(
        db.clone(),
        workspace,
        Some(app_handle.clone()),
        Some(instance.name.clone()),
    );
    let available_dynamic_tools = rhai_registry.tool_summary().await.unwrap_or_default();
    let registry: SharedRegistry = Arc::new(tokio::sync::RwLock::new(rhai_registry));

    let long_term_memory = LongTermMemory::new(db.clone()).await?;
    let shared_ltm: SharedLongTermMemory = Arc::new(tokio::sync::Mutex::new(long_term_memory));

    let tools = build_sub_agent_tools(
        instance_id,
        registry,
        available_dynamic_tools,
        db,
        programs_root,
        shared_ltm,
        Some(app_handle.clone()),
    );

    // 5. Build system prompt for scheduled task agent
    let system_prompt = format!(
        "You are a scheduled task agent for '{}'. \
         You are running autonomously as part of a recurring scheduled task. \
         Complete the task described below using the tools available to you. \
         Be concise in your response -- summarize what you did and any important results.\n\n{}",
        instance.name,
        base_tools_prompt()
    );

    // 6. Create and run provider-specific agent
    let result = run_task_agent(&instance, &api_key, &system_prompt, task_prompt, tools).await?;

    Ok(result)
}

/// Create a temporary rig agent and run the task prompt.
async fn run_task_agent(
    instance: &AIInstance,
    api_key: &str,
    system_prompt: &str,
    task_prompt: &str,
    tools: Vec<Box<dyn rig::tool::ToolDyn>>,
) -> Result<String> {
    match instance.provider {
        LLMProvider::Anthropic => {
            let client: anthropic::Client =
                anthropic::Client::builder().api_key(api_key).build()?;
            let agent = client
                .agent(&instance.model)
                .preamble(system_prompt)
                .max_tokens(32768)
                .temperature(0.7)
                .tools(tools)
                .build();
            let result = agent
                .prompt(task_prompt)
                .max_turns(TASK_AGENT_MAX_TURNS)
                .await?;
            Ok(result)
        }
        LLMProvider::OpenAI => {
            let client: openai::Client = openai::Client::builder().api_key(api_key).build()?;
            let agent = client
                .completions_api()
                .agent(&instance.model)
                .preamble(system_prompt)
                .temperature(0.7)
                .tools(tools)
                .build();
            let result = agent
                .prompt(task_prompt)
                .max_turns(TASK_AGENT_MAX_TURNS)
                .await?;
            Ok(result)
        }
        LLMProvider::Ollama => {
            let ollama_client: ollama::Client = if let Some(url) = &instance.api_base_url {
                ollama::Client::builder()
                    .api_key(Nothing)
                    .base_url(url)
                    .build()?
            } else {
                ollama::Client::new(Nothing)?
            };
            let agent = ollama_client
                .agent(&instance.model)
                .preamble(system_prompt)
                .tools(tools)
                .build();
            let result = agent
                .prompt(task_prompt)
                .max_turns(TASK_AGENT_MAX_TURNS)
                .await?;
            Ok(result)
        }
    }
}

/// Load all enabled tasks for an instance and register them with the scheduler.
pub async fn load_and_register_instance_tasks(
    scheduler: &SharedScheduler,
    instance_id: &str,
    db: &Pool<Sqlite>,
    manager: Arc<Mutex<AIInstanceManager>>,
    app_handle: AppHandle,
) -> Result<usize> {
    let tasks = storage::load_tasks(db, instance_id).await?;
    let mut registered = 0;

    for task in tasks {
        if !task.enabled {
            continue;
        }

        if let Err(e) = register_task_job(
            scheduler,
            task.id.clone(),
            &task.cron_expression,
            task.name.clone(),
            task.task_prompt.clone(),
            task.instance_id.clone(),
            manager.clone(),
            app_handle.clone(),
        )
        .await
        {
            tracing::warn!(
                "Failed to register scheduled task '{}' ({}): {}",
                task.name,
                task.id,
                e
            );
        } else {
            registered += 1;
        }
    }

    if registered > 0 {
        tracing::info!(
            "Registered {} scheduled tasks for instance '{}'",
            registered,
            instance_id
        );
    }

    Ok(registered)
}
