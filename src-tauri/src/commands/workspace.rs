use crate::utils::paths;

/// Open the workspace directory for the given instance in the system file manager.
#[tauri::command]
pub async fn open_workspace(instance_id: String) -> Result<String, String> {
    let workspace = paths::get_instance_workspace_path(&instance_id).map_err(|e| e.to_string())?;

    // Ensure the directory exists
    std::fs::create_dir_all(&workspace).map_err(|e| {
        format!(
            "Failed to create workspace directory '{}': {}",
            workspace.display(),
            e
        )
    })?;

    // Use the open crate (via tauri's opener) or std::process::Command
    // to open the directory in the system file manager.
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&workspace)
            .spawn()
            .map_err(|e| format!("Failed to open Finder: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&workspace)
            .spawn()
            .map_err(|e| format!("Failed to open Explorer: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&workspace)
            .spawn()
            .map_err(|e| format!("Failed to open file manager: {}", e))?;
    }

    Ok(workspace.to_string_lossy().to_string())
}
