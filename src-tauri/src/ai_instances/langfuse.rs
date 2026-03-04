use anyhow::{Context, Result};
use keyring::Entry;

const SERVICE_NAME: &str = "ownai";
const LANGFUSE_PUBLIC_KEY_USER: &str = "langfuse-public-key";
const LANGFUSE_SECRET_KEY_USER: &str = "langfuse-secret-key";
const LANGFUSE_HOST_USER: &str = "langfuse-host";

/// Default Langfuse cloud host
pub const DEFAULT_LANGFUSE_HOST: &str = "https://cloud.langfuse.com";

/// Secure Langfuse credential storage using OS keychain
pub struct LangfuseKeyStorage;

impl LangfuseKeyStorage {
    // ========================================================================
    // Public Key
    // ========================================================================

    /// Save Langfuse public key to OS keychain
    pub fn save_public_key(key: &str) -> Result<()> {
        let entry = Entry::new(SERVICE_NAME, LANGFUSE_PUBLIC_KEY_USER)
            .context("Failed to create keychain entry for Langfuse public key")?;
        entry
            .set_password(key)
            .context("Failed to save Langfuse public key to keychain")?;
        tracing::info!("Saved Langfuse public key to keychain");
        Ok(())
    }

    /// Load Langfuse public key from OS keychain
    pub fn load_public_key() -> Result<Option<String>> {
        let entry = Entry::new(SERVICE_NAME, LANGFUSE_PUBLIC_KEY_USER)
            .context("Failed to create keychain entry for Langfuse public key")?;
        match entry.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e).context("Failed to load Langfuse public key from keychain"),
        }
    }

    /// Delete Langfuse public key from OS keychain
    pub fn delete_public_key() -> Result<()> {
        let entry = Entry::new(SERVICE_NAME, LANGFUSE_PUBLIC_KEY_USER)
            .context("Failed to create keychain entry for Langfuse public key")?;
        match entry.delete_credential() {
            Ok(_) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e).context("Failed to delete Langfuse public key from keychain"),
        }
    }

    // ========================================================================
    // Secret Key
    // ========================================================================

    /// Save Langfuse secret key to OS keychain
    pub fn save_secret_key(key: &str) -> Result<()> {
        let entry = Entry::new(SERVICE_NAME, LANGFUSE_SECRET_KEY_USER)
            .context("Failed to create keychain entry for Langfuse secret key")?;
        entry
            .set_password(key)
            .context("Failed to save Langfuse secret key to keychain")?;
        tracing::info!("Saved Langfuse secret key to keychain");
        Ok(())
    }

    /// Load Langfuse secret key from OS keychain
    pub fn load_secret_key() -> Result<Option<String>> {
        let entry = Entry::new(SERVICE_NAME, LANGFUSE_SECRET_KEY_USER)
            .context("Failed to create keychain entry for Langfuse secret key")?;
        match entry.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e).context("Failed to load Langfuse secret key from keychain"),
        }
    }

    /// Delete Langfuse secret key from OS keychain
    pub fn delete_secret_key() -> Result<()> {
        let entry = Entry::new(SERVICE_NAME, LANGFUSE_SECRET_KEY_USER)
            .context("Failed to create keychain entry for Langfuse secret key")?;
        match entry.delete_credential() {
            Ok(_) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e).context("Failed to delete Langfuse secret key from keychain"),
        }
    }

    // ========================================================================
    // Host
    // ========================================================================

    /// Save Langfuse host URL to OS keychain
    pub fn save_host(host: &str) -> Result<()> {
        let entry = Entry::new(SERVICE_NAME, LANGFUSE_HOST_USER)
            .context("Failed to create keychain entry for Langfuse host")?;
        entry
            .set_password(host)
            .context("Failed to save Langfuse host to keychain")?;
        tracing::info!("Saved Langfuse host to keychain");
        Ok(())
    }

    /// Load Langfuse host URL from OS keychain, returning default if not set
    pub fn load_host() -> String {
        let entry = match Entry::new(SERVICE_NAME, LANGFUSE_HOST_USER) {
            Ok(e) => e,
            Err(_) => return DEFAULT_LANGFUSE_HOST.to_string(),
        };
        match entry.get_password() {
            Ok(host) if !host.is_empty() => host,
            _ => DEFAULT_LANGFUSE_HOST.to_string(),
        }
    }

    /// Delete Langfuse host from OS keychain
    pub fn delete_host() -> Result<()> {
        let entry = Entry::new(SERVICE_NAME, LANGFUSE_HOST_USER)
            .context("Failed to create keychain entry for Langfuse host")?;
        match entry.delete_credential() {
            Ok(_) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e).context("Failed to delete Langfuse host from keychain"),
        }
    }

    // ========================================================================
    // Convenience
    // ========================================================================

    /// Save all Langfuse credentials at once
    pub fn save_all(public_key: &str, secret_key: &str, host: &str) -> Result<()> {
        Self::save_public_key(public_key)?;
        Self::save_secret_key(secret_key)?;
        Self::save_host(host)?;
        Ok(())
    }

    /// Delete all Langfuse credentials
    pub fn delete_all() -> Result<()> {
        Self::delete_public_key()?;
        Self::delete_secret_key()?;
        Self::delete_host()?;
        Ok(())
    }

    /// Check if Langfuse is configured (both public and secret key present)
    pub fn is_configured() -> bool {
        matches!(
            (Self::load_public_key(), Self::load_secret_key()),
            (Ok(Some(_)), Ok(Some(_)))
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Requires OS keychain access (not available in headless CI)
    fn test_langfuse_key_storage() {
        let public_key = "pk-lf-test-key";
        let secret_key = "sk-lf-test-key";
        let host = "http://example.com";

        // Save all
        LangfuseKeyStorage::save_all(public_key, secret_key, host).unwrap();

        // Check configured
        assert!(LangfuseKeyStorage::is_configured());

        // Load
        assert_eq!(
            LangfuseKeyStorage::load_public_key().unwrap(),
            Some(public_key.to_string())
        );
        assert_eq!(
            LangfuseKeyStorage::load_secret_key().unwrap(),
            Some(secret_key.to_string())
        );
        assert_eq!(LangfuseKeyStorage::load_host(), host);

        // Delete all
        LangfuseKeyStorage::delete_all().unwrap();

        // Verify deleted
        assert!(!LangfuseKeyStorage::is_configured());
        assert_eq!(
            LangfuseKeyStorage::load_host(),
            DEFAULT_LANGFUSE_HOST.to_string()
        );
    }
}
