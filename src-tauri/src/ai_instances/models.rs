use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents an AI instance with its metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIInstance {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
}

/// Request to create a new AI instance
#[derive(Debug, Deserialize)]
pub struct CreateInstanceRequest {
    pub name: String,
}
