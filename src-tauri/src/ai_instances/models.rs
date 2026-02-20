use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// LLM Provider types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LLMProvider {
    Anthropic,
    OpenAI,
    Ollama,
}

impl LLMProvider {
    /// Returns suggested models for this provider (user can also enter custom)
    pub fn suggested_models(&self) -> Vec<&'static str> {
        match self {
            LLMProvider::Anthropic => vec!["claude-sonnet-4-5-20250929"],
            LLMProvider::OpenAI => vec!["gpt-5.2-2025-12-11", "gpt-5-mini-2025-08-07"],
            LLMProvider::Ollama => vec![], // User enters their own
        }
    }

    /// Returns whether this provider requires an API key
    pub fn needs_api_key(&self) -> bool {
        match self {
            LLMProvider::Anthropic => true,
            LLMProvider::OpenAI => true,
            LLMProvider::Ollama => false,
        }
    }

    /// Returns the default model for this provider (if any)
    pub fn default_model(&self) -> Option<&'static str> {
        match self {
            LLMProvider::Anthropic => Some("claude-sonnet-4-5-20250929"),
            LLMProvider::OpenAI => Some("gpt-5.2-2025-12-11"),
            LLMProvider::Ollama => None, // No default
        }
    }
}

impl std::fmt::Display for LLMProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LLMProvider::Anthropic => write!(f, "anthropic"),
            LLMProvider::OpenAI => write!(f, "openai"),
            LLMProvider::Ollama => write!(f, "ollama"),
        }
    }
}

/// Represents an AI instance with its metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIInstance {
    pub id: String,
    pub name: String,
    pub provider: LLMProvider,
    pub model: String,

    /// Optional custom base URL (e.g., for Ollama: http://localhost:11434)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_base_url: Option<String>,

    /// Path to instance database
    #[serde(skip)]
    pub db_path: Option<PathBuf>,

    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
}

/// Request to create a new AI instance
#[derive(Debug, Deserialize)]
pub struct CreateInstanceRequest {
    pub name: String,
    pub provider: String, // "anthropic" | "openai" | "ollama"
    pub model: String,
    /// Optional custom base URL (e.g., for Ollama: http://localhost:11434)
    #[serde(default)]
    pub api_base_url: Option<String>,
}

/// Information about a provider for the frontend
#[derive(Debug, Clone, Serialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub needs_api_key: bool,
    pub has_api_key: bool,
    pub suggested_models: Vec<&'static str>,
    pub default_model: Option<&'static str>,
}
