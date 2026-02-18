// Modules
pub mod agent;
pub mod ai_instances;
pub mod commands;
pub mod database;
pub mod memory;
pub mod tools;
pub mod utils;

use std::collections::HashMap;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Initialize AI Instance Manager
            let manager = ai_instances::AIInstanceManager::new()
                .expect("Failed to initialize AI Instance Manager");

            app.manage(Arc::new(Mutex::new(manager)));

            // Initialize Agent Cache
            let agent_cache: commands::chat::AgentCache = Arc::new(Mutex::new(HashMap::new()));
            app.manage(agent_cache);

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
            commands::tools::delete_dynamic_tool,
            commands::tools::execute_dynamic_tool,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
