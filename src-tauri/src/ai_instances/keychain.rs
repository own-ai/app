use super::models::LLMProvider;
use anyhow::{Context, Result};
use keyring::Entry;

const SERVICE_NAME: &str = "ownai";

/// Secure API key storage using OS keychain
pub struct APIKeyStorage;

impl APIKeyStorage {
    /// Save API key to OS keychain
    pub fn save(provider: &LLMProvider, api_key: &str) -> Result<()> {
        let username = provider.to_string();
        let entry =
            Entry::new(SERVICE_NAME, &username).context("Failed to create keychain entry")?;

        entry
            .set_password(api_key)
            .context("Failed to save API key to keychain")?;

        tracing::info!("Saved API key to keychain for provider: {}", provider);

        Ok(())
    }

    /// Load API key from OS keychain
    pub fn load(provider: &LLMProvider) -> Result<Option<String>> {
        let username = provider.to_string();
        let entry =
            Entry::new(SERVICE_NAME, &username).context("Failed to create keychain entry")?;

        match entry.get_password() {
            Ok(key) => {
                tracing::debug!("Loaded API key from keychain for provider: {}", provider);
                Ok(Some(key))
            }
            Err(keyring::Error::NoEntry) => {
                tracing::debug!("No API key found in keychain for provider: {}", provider);
                Ok(None)
            }
            Err(e) => Err(e).context("Failed to load API key from keychain"),
        }
    }

    /// Delete API key from OS keychain
    pub fn delete(provider: &LLMProvider) -> Result<()> {
        let username = provider.to_string();
        let entry =
            Entry::new(SERVICE_NAME, &username).context("Failed to create keychain entry")?;

        match entry.delete_credential() {
            Ok(_) => {
                tracing::info!("Deleted API key from keychain for provider: {}", provider);
                Ok(())
            }
            Err(keyring::Error::NoEntry) => {
                // Already deleted, that's fine
                Ok(())
            }
            Err(e) => Err(e).context("Failed to delete API key from keychain"),
        }
    }

    /// Check if an API key exists for a provider
    pub fn exists(provider: &LLMProvider) -> Result<bool> {
        Ok(Self::load(provider)?.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_storage() {
        let provider = LLMProvider::Anthropic;
        let api_key = "test-api-key-secret";

        // Save
        APIKeyStorage::save(&provider, api_key).unwrap();

        // Load
        let loaded = APIKeyStorage::load(&provider).unwrap();
        assert_eq!(loaded, Some(api_key.to_string()));

        // Exists
        let exists = APIKeyStorage::exists(&provider).unwrap();
        assert!(exists);

        // Delete
        APIKeyStorage::delete(&provider).unwrap();

        // Verify deleted
        let loaded_after_delete = APIKeyStorage::load(&provider).unwrap();
        assert_eq!(loaded_after_delete, None);

        // Exists after delete
        let exists_after_delete = APIKeyStorage::exists(&provider).unwrap();
        assert!(!exists_after_delete);
    }
}
