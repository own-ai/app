use crate::ai_instances::{APIKeyStorage, AIInstance, AIInstanceManager, CreateInstanceRequest, LLMProvider, ProviderInfo};
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

// ============================================================================
// Provider & API Key Commands
// ============================================================================

/// Get information about all available providers
#[tauri::command]
pub fn get_providers() -> Result<Vec<ProviderInfo>, String> {
    let providers = vec![
        LLMProvider::Anthropic,
        LLMProvider::OpenAI,
        LLMProvider::Ollama,
    ];
    
    let provider_infos: Vec<ProviderInfo> = providers
        .into_iter()
        .map(|p| {
            let has_key = if p.needs_api_key() {
                APIKeyStorage::exists(&p).unwrap_or(false)
            } else {
                true // Ollama doesn't need a key
            };
            
            ProviderInfo {
                id: p.to_string(),
                name: match &p {
                    LLMProvider::Anthropic => "Anthropic".to_string(),
                    LLMProvider::OpenAI => "OpenAI".to_string(),
                    LLMProvider::Ollama => "Ollama".to_string(),
                },
                needs_api_key: p.needs_api_key(),
                has_api_key: has_key,
                suggested_models: p.suggested_models(),
                default_model: p.default_model(),
            }
        })
        .collect();
    
    Ok(provider_infos)
}

/// Save an API key for a provider
#[tauri::command]
pub fn save_api_key(provider: String, api_key: String) -> Result<(), String> {
    let provider = parse_provider(&provider)?;
    
    if !provider.needs_api_key() {
        return Err(format!("Provider {} does not require an API key", provider));
    }
    
    APIKeyStorage::save(&provider, &api_key)
        .map_err(|e| format!("Failed to save API key: {}", e))?;
    
    tracing::info!("Saved API key for provider: {}", provider);
    Ok(())
}

/// Check if an API key exists for a provider
#[tauri::command]
pub fn has_api_key(provider: String) -> Result<bool, String> {
    let provider = parse_provider(&provider)?;
    
    if !provider.needs_api_key() {
        return Ok(true); // Ollama doesn't need a key, so it's always "available"
    }
    
    APIKeyStorage::exists(&provider)
        .map_err(|e| format!("Failed to check API key: {}", e))
}

/// Delete an API key for a provider
#[tauri::command]
pub fn delete_api_key(provider: String) -> Result<(), String> {
    let provider = parse_provider(&provider)?;
    
    APIKeyStorage::delete(&provider)
        .map_err(|e| format!("Failed to delete API key: {}", e))?;
    
    tracing::info!("Deleted API key for provider: {}", provider);
    Ok(())
}

/// Helper function to parse provider string
fn parse_provider(provider: &str) -> Result<LLMProvider, String> {
    match provider.to_lowercase().as_str() {
        "anthropic" => Ok(LLMProvider::Anthropic),
        "openai" => Ok(LLMProvider::OpenAI),
        "ollama" => Ok(LLMProvider::Ollama),
        _ => Err(format!("Invalid provider: {}", provider)),
    }
}

// ============================================================================
// AI Instance Commands
// ============================================================================

/// Create a new AI instance
#[tauri::command]
pub async fn create_ai_instance(
    request: CreateInstanceRequest,
    manager: State<'_, Arc<Mutex<AIInstanceManager>>>,
) -> Result<AIInstance, String> {
    // Parse provider
    let provider = match request.provider.to_lowercase().as_str() {
        "anthropic" => LLMProvider::Anthropic,
        "openai" => LLMProvider::OpenAI,
        "ollama" => LLMProvider::Ollama,
        _ => return Err(format!("Invalid provider: {}", request.provider)),
    };

    // Validate API key exists for providers that need it
    // API keys are stored per provider, not per instance
    if provider.needs_api_key() {
        let has_key = APIKeyStorage::exists(&provider)
            .map_err(|e| format!("Failed to check API key: {}", e))?;
        
        if !has_key {
            return Err(format!(
                "API key not configured for provider: {}. Please add it in Settings.",
                provider
            ));
        }
    }

    // Create instance
    let mut manager = manager.lock().await;
    let instance = manager
        .create_instance(
            request.name,
            provider.clone(),
            request.model,
            request.api_base_url,
        )
        .map_err(|e| e.to_string())?;

    tracing::info!(
        "Created AI instance '{}' with provider: {:?}",
        instance.name,
        provider
    );

    Ok(instance)
}

/// List all AI instances
#[tauri::command]
pub async fn list_ai_instances(
    manager: State<'_, Arc<Mutex<AIInstanceManager>>>,
) -> Result<Vec<AIInstance>, String> {
    let manager = manager.lock().await;
    Ok(manager.list_instances())
}

/// Set the active AI instance
#[tauri::command]
pub async fn set_active_instance(
    instance_id: String,
    manager: State<'_, Arc<Mutex<AIInstanceManager>>>,
) -> Result<(), String> {
    let mut manager = manager.lock().await;
    manager.set_active(instance_id).map_err(|e| e.to_string())
}

/// Get the currently active AI instance
#[tauri::command]
pub async fn get_active_instance(
    manager: State<'_, Arc<Mutex<AIInstanceManager>>>,
) -> Result<Option<AIInstance>, String> {
    let manager = manager.lock().await;
    Ok(manager.get_active_instance().cloned())
}

/// Delete an AI instance
#[tauri::command]
pub async fn delete_ai_instance(
    instance_id: String,
    manager: State<'_, Arc<Mutex<AIInstanceManager>>>,
) -> Result<(), String> {
    let mut manager = manager.lock().await;
    
    // Delete the instance
    manager
        .delete_instance(&instance_id)
        .map_err(|e| e.to_string())
}
