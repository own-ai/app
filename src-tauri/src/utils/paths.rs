use anyhow::{Context, Result};
use std::path::PathBuf;

/// Get the main application directory (~/.ownai)
pub fn get_app_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .context("Failed to determine home directory")?;

    let path = PathBuf::from(home).join(".ownai");
    std::fs::create_dir_all(&path).context("Failed to create .ownai directory")?;

    Ok(path)
}

/// Get the instances directory (~/.ownai/instances)
pub fn get_instances_path() -> Result<PathBuf> {
    let path = get_app_dir()?.join("instances");
    std::fs::create_dir_all(&path).context("Failed to create instances directory")?;
    Ok(path)
}

/// Get the config file path (~/.ownai/instances.json)
pub fn get_config_path() -> Result<PathBuf> {
    Ok(get_app_dir()?.join("instances.json"))
}

/// Get the database path for a specific instance
pub fn get_instance_db_path(instance_id: &str) -> Result<PathBuf> {
    Ok(get_instances_path()?.join(instance_id).join("ownai.db"))
}

/// Get the tools directory for a specific instance
pub fn get_instance_tools_path(instance_id: &str) -> Result<PathBuf> {
    let path = get_instances_path()?.join(instance_id).join("tools");
    std::fs::create_dir_all(&path).context("Failed to create tools directory")?;
    Ok(path)
}

/// Get the workspace directory for a specific instance
pub fn get_instance_workspace_path(instance_id: &str) -> Result<PathBuf> {
    let path = get_instances_path()?.join(instance_id).join("workspace");
    std::fs::create_dir_all(&path).context("Failed to create workspace directory")?;
    Ok(path)
}

/// Get the programs directory for a specific instance
pub fn get_instance_programs_path(instance_id: &str) -> Result<PathBuf> {
    let path = get_instances_path()?.join(instance_id).join("programs");
    std::fs::create_dir_all(&path).context("Failed to create programs directory")?;
    Ok(path)
}

/// Get the directory for a specific program within an instance
pub fn get_program_path(instance_id: &str, program_name: &str) -> Result<PathBuf> {
    let path = get_instance_programs_path(instance_id)?.join(program_name);
    Ok(path)
}
