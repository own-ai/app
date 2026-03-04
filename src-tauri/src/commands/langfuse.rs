use crate::ai_instances::LangfuseKeyStorage;
use serde::Serialize;

/// Response for get_langfuse_config: reveals whether keys exist and the current host,
/// but never exposes the actual secret/public key values.
#[derive(Serialize)]
pub struct LangfuseConfigResponse {
    pub has_keys: bool,
    pub host: String,
}

/// Save Langfuse configuration (public key, secret key, host) to OS keychain.
/// Requires app restart to take effect.
#[tauri::command]
pub async fn save_langfuse_config(
    public_key: String,
    secret_key: String,
    host: String,
) -> Result<(), String> {
    let host = if host.trim().is_empty() {
        crate::ai_instances::langfuse::DEFAULT_LANGFUSE_HOST.to_string()
    } else {
        host.trim().to_string()
    };

    LangfuseKeyStorage::save_all(&public_key, &secret_key, &host)
        .map_err(|e| format!("Failed to save Langfuse config: {}", e))
}

/// Get current Langfuse configuration status (whether keys exist + host URL).
/// Never returns actual key values.
#[tauri::command]
pub async fn get_langfuse_config() -> Result<LangfuseConfigResponse, String> {
    let has_keys = LangfuseKeyStorage::is_configured();
    let host = LangfuseKeyStorage::load_host();
    Ok(LangfuseConfigResponse { has_keys, host })
}

/// Delete all Langfuse credentials from OS keychain.
/// Requires app restart to take effect.
#[tauri::command]
pub async fn delete_langfuse_config() -> Result<(), String> {
    LangfuseKeyStorage::delete_all().map_err(|e| format!("Failed to delete Langfuse config: {}", e))
}
