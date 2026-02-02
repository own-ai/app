// Modules
pub mod ai_instances;
pub mod commands;
pub mod utils;

use tauri::Manager;

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
            
            app.manage(std::sync::Arc::new(tokio::sync::Mutex::new(manager)));
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::instances::create_ai_instance,
            commands::instances::list_ai_instances,
            commands::instances::set_active_instance,
            commands::instances::get_active_instance,
            commands::instances::delete_ai_instance,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
