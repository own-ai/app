// Modules
pub mod agent;
pub mod ai_instances;
pub mod canvas;
pub mod commands;
pub mod database;
pub mod memory;
pub mod scheduler;
pub mod tools;
pub mod utils;

use std::collections::HashMap;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;

use canvas::protocol;
use utils::paths;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .register_asynchronous_uri_scheme_protocol("ownai-program", |_ctx, request, responder| {
            // Custom protocol handler for serving Canvas program files.
            // All file I/O here is synchronous (std::fs::read), which is fine
            // for serving local program files.
            let url = request.uri().to_string();

            let (instance_id, program_name, file_path) = match protocol::parse_protocol_url(&url) {
                Ok(parsed) => parsed,
                Err(e) => {
                    tracing::warn!("Invalid protocol URL '{}': {}", url, e);
                    let response = tauri::http::Response::builder()
                        .status(400)
                        .body(format!("Bad Request: {}", e).into_bytes())
                        .unwrap();
                    responder.respond(response);
                    return;
                }
            };

            let programs_root = match paths::get_instance_programs_path(&instance_id) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!("Failed to get programs path: {}", e);
                    let response = tauri::http::Response::builder()
                        .status(500)
                        .body(b"Internal Server Error".to_vec())
                        .unwrap();
                    responder.respond(response);
                    return;
                }
            };

            match protocol::load_program_file(&programs_root, &program_name, &file_path) {
                Ok((bytes, mime)) => {
                    let response = tauri::http::Response::builder()
                        .status(200)
                        .header("Content-Type", &mime)
                        .header("Access-Control-Allow-Origin", "*")
                        .body(bytes)
                        .unwrap();
                    responder.respond(response);
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to load program file '{}/{}': {}",
                        program_name,
                        file_path,
                        e
                    );
                    let response = tauri::http::Response::builder()
                        .status(404)
                        .body(format!("Not Found: {}", e).into_bytes())
                        .unwrap();
                    responder.respond(response);
                }
            }
        })
        .setup(|app| {
            // Initialize AI Instance Manager
            let manager = ai_instances::AIInstanceManager::new()
                .expect("Failed to initialize AI Instance Manager");
            let shared_manager = Arc::new(Mutex::new(manager));
            app.manage(shared_manager.clone());

            // Initialize Agent Cache
            let agent_cache: commands::chat::AgentCache = Arc::new(Mutex::new(HashMap::new()));
            app.manage(agent_cache);

            // Initialize Scheduler
            let app_handle = app.handle().clone();
            let manager_for_scheduler = shared_manager.clone();
            tauri::async_runtime::spawn(async move {
                match scheduler::Scheduler::new().await {
                    Ok(sched) => {
                        let shared_scheduler: scheduler::SharedScheduler =
                            Arc::new(Mutex::new(sched));

                        // Load and register tasks for all instances
                        {
                            let mgr = manager_for_scheduler.lock().await;
                            let instance_ids: Vec<String> =
                                mgr.list_instances().iter().map(|i| i.id.clone()).collect();
                            drop(mgr);

                            for instance_id in instance_ids {
                                if let Ok(db) = database::init_database(&instance_id).await {
                                    if let Err(e) =
                                        scheduler::runner::load_and_register_instance_tasks(
                                            &shared_scheduler,
                                            &instance_id,
                                            &db,
                                            manager_for_scheduler.clone(),
                                            app_handle.clone(),
                                        )
                                        .await
                                    {
                                        tracing::warn!(
                                            "Failed to load scheduled tasks for instance '{}': {}",
                                            instance_id,
                                            e
                                        );
                                    }
                                }
                            }
                        }

                        // Start the scheduler
                        if let Err(e) = shared_scheduler.lock().await.start().await {
                            tracing::error!("Failed to start scheduler: {}", e);
                        }

                        // Manage as Tauri state
                        app_handle.manage(shared_scheduler);
                    }
                    Err(e) => {
                        tracing::error!("Failed to create scheduler: {}", e);
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Provider & API Key Management
            commands::instances::get_providers,
            commands::instances::save_api_key,
            commands::instances::has_api_key,
            commands::instances::delete_api_key,
            // AI Instance Management
            commands::instances::create_ai_instance,
            commands::instances::list_ai_instances,
            commands::instances::set_active_instance,
            commands::instances::get_active_instance,
            commands::instances::delete_ai_instance,
            // Chat Commands
            commands::chat::send_message,
            commands::chat::stream_message,
            commands::chat::load_messages,
            commands::chat::clear_agent_cache,
            // Memory
            commands::memory::get_memory_stats,
            commands::memory::search_memory,
            commands::memory::add_memory_entry,
            commands::memory::delete_memory_entry,
            // Dynamic Tools (Rhai)
            commands::tools::list_dynamic_tools,
            commands::tools::create_dynamic_tool,
            commands::tools::update_dynamic_tool,
            commands::tools::delete_dynamic_tool,
            commands::tools::execute_dynamic_tool,
            // Canvas Programs
            commands::canvas::list_programs,
            commands::canvas::delete_program,
            commands::canvas::get_program_url,
            commands::canvas::bridge_request,
            // Workspace
            commands::workspace::open_workspace,
            // Scheduled Tasks
            commands::scheduler::list_scheduled_tasks,
            commands::scheduler::delete_scheduled_task,
            commands::scheduler::toggle_scheduled_task,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
