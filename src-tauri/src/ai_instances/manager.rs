use super::models::{AIInstance, LLMProvider};
use crate::utils::paths::{
    get_config_path, get_instance_db_path, get_instance_tools_path, get_instance_workspace_path,
    get_instances_path,
};
use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use std::fs;
use uuid::Uuid;

/// Manages all AI instances
pub struct AIInstanceManager {
    pub instances: HashMap<String, AIInstance>,
    pub active_instance_id: Option<String>,
}

impl AIInstanceManager {
    /// Create a new AIInstanceManager and load existing instances
    pub fn new() -> Result<Self> {
        let instances = Self::load_instances()?;

        Ok(Self {
            instances,
            active_instance_id: None,
        })
    }

    /// Create a new AI instance
    pub fn create_instance(
        &mut self,
        name: String,
        provider: LLMProvider,
        model: String,
        api_base_url: Option<String>,
    ) -> Result<AIInstance> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();

        // Get paths for this instance
        let db_path = get_instance_db_path(&id)?;
        let tools_path = get_instance_tools_path(&id)?;

        let instance = AIInstance {
            id: id.clone(),
            name,
            provider,
            model,
            api_base_url,
            db_path: Some(db_path),
            tools_path: Some(tools_path),
            created_at: now,
            last_active: now,
        };

        // Create instance directories
        self.create_instance_directories(&id)?;

        // Add to instances map
        self.instances.insert(id.clone(), instance.clone());

        // Save to disk
        self.save_instances()?;

        // Set as active instance
        self.active_instance_id = Some(id.clone());

        tracing::info!(
            "Created new AI instance: {} ({}) with provider: {:?}",
            instance.name,
            instance.id,
            instance.provider
        );

        Ok(instance)
    }

    /// List all AI instances
    pub fn list_instances(&self) -> Vec<AIInstance> {
        self.instances.values().cloned().collect()
    }

    /// Get a specific instance by ID
    pub fn get_instance(&self, id: &str) -> Option<&AIInstance> {
        self.instances.get(id)
    }

    /// Set the active instance
    pub fn set_active(&mut self, id: String) -> Result<()> {
        if !self.instances.contains_key(&id) {
            anyhow::bail!("Instance not found: {}", id);
        }

        // Update last_active timestamp
        if let Some(instance) = self.instances.get_mut(&id) {
            instance.last_active = Utc::now();
        }

        self.active_instance_id = Some(id.clone());
        self.save_instances()?;

        tracing::info!("Switched to AI instance: {}", id);

        Ok(())
    }

    /// Get the currently active instance
    pub fn get_active_instance(&self) -> Option<&AIInstance> {
        self.active_instance_id
            .as_ref()
            .and_then(|id| self.instances.get(id))
    }

    /// Delete an AI instance
    pub fn delete_instance(&mut self, id: &str) -> Result<()> {
        if !self.instances.contains_key(id) {
            anyhow::bail!("Instance not found: {}", id);
        }

        // Remove from active if it was active
        if self.active_instance_id.as_deref() == Some(id) {
            self.active_instance_id = None;
        }

        // Remove from instances
        self.instances.remove(id);

        // Delete directory
        let instance_path = get_instances_path()?.join(id);
        if instance_path.exists() {
            fs::remove_dir_all(&instance_path).context("Failed to delete instance directory")?;
        }

        // Save config
        self.save_instances()?;

        tracing::info!("Deleted AI instance: {}", id);

        Ok(())
    }

    /// Create necessary directories for a new instance
    fn create_instance_directories(&self, id: &str) -> Result<()> {
        let base_path = get_instances_path()?.join(id);

        // Create base directory
        fs::create_dir_all(&base_path).context("Failed to create instance directory")?;

        // Create subdirectories
        get_instance_tools_path(id)?;
        get_instance_workspace_path(id)?;

        // Create tools/scripts subdirectory
        fs::create_dir_all(base_path.join("tools/scripts"))
            .context("Failed to create tools/scripts directory")?;

        tracing::debug!("Created directories for instance: {}", id);

        Ok(())
    }

    /// Load instances from config file
    fn load_instances() -> Result<HashMap<String, AIInstance>> {
        let config_path = get_config_path()?;

        if !config_path.exists() {
            return Ok(HashMap::new());
        }

        let contents =
            fs::read_to_string(&config_path).context("Failed to read instances config")?;

        let instances: HashMap<String, AIInstance> =
            serde_json::from_str(&contents).context("Failed to parse instances config")?;

        tracing::debug!("Loaded {} AI instances", instances.len());

        Ok(instances)
    }

    /// Save instances to config file
    fn save_instances(&self) -> Result<()> {
        let config_path = get_config_path()?;

        let contents = serde_json::to_string_pretty(&self.instances)
            .context("Failed to serialize instances")?;

        fs::write(&config_path, contents).context("Failed to write instances config")?;

        tracing::debug!("Saved {} AI instances to config", self.instances.len());

        Ok(())
    }
}
