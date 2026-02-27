//! Scheduled tasks system for recurring agent actions.
//!
//! Provides a cron-based scheduler that allows the AI agent to create
//! recurring tasks. When a task fires, a temporary agent is created
//! to execute the task prompt autonomously.
//!
//! ## Module Structure
//!
//! - `mod.rs` - Core types (`ScheduledTask`, `Scheduler`, `SharedScheduler`)
//! - `storage.rs` - Database CRUD operations for scheduled tasks
//! - `runner.rs` - Task execution logic (temporary agent creation)
//! - `tools.rs` - rig Tools for the agent to manage scheduled tasks

pub mod runner;
pub mod storage;
pub mod tools;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_cron_scheduler::JobScheduler;

// Re-export tools for convenience
pub use tools::{CreateScheduledTaskTool, DeleteScheduledTaskTool, ListScheduledTasksTool};

/// Shared scheduler reference, managed as Tauri state.
pub type SharedScheduler = Arc<Mutex<Scheduler>>;

/// A scheduled task definition stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: String,
    pub instance_id: String,
    pub name: String,
    pub cron_expression: String,
    pub task_prompt: String,
    pub enabled: bool,
    /// Whether to send OS notifications and show results in the chat on completion.
    /// When false, results are still saved in last_result and as messages in the DB,
    /// but no notification or frontend event is emitted.
    pub notify: bool,
    pub last_run: Option<DateTime<Utc>>,
    pub last_result: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// The scheduler manages cron jobs for all AI instances.
///
/// It wraps `tokio-cron-scheduler`'s `JobScheduler` and maintains a mapping
/// from task IDs to internal job UUIDs so jobs can be removed at runtime.
pub struct Scheduler {
    job_scheduler: JobScheduler,
    /// Maps task_id -> job UUID (for removing jobs at runtime)
    job_ids: HashMap<String, uuid::Uuid>,
}

impl Scheduler {
    /// Create a new Scheduler. Call `start()` after setup to begin processing jobs.
    pub async fn new() -> anyhow::Result<Self> {
        let job_scheduler = JobScheduler::new()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create job scheduler: {}", e))?;

        Ok(Self {
            job_scheduler,
            job_ids: HashMap::new(),
        })
    }

    /// Start the scheduler. Must be called after all initial jobs are registered.
    pub async fn start(&self) -> anyhow::Result<()> {
        self.job_scheduler
            .start()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to start scheduler: {}", e))?;

        tracing::info!("Scheduler started");
        Ok(())
    }

    /// Register a task as a cron job.
    ///
    /// The `on_fire` callback is called each time the cron expression triggers.
    /// Returns the internal job UUID.
    pub async fn add_job(
        &mut self,
        task_id: &str,
        cron_expression: &str,
        on_fire: impl FnMut(
                uuid::Uuid,
                tokio_cron_scheduler::JobScheduler,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
            + Send
            + Sync
            + 'static,
    ) -> anyhow::Result<()> {
        let job = tokio_cron_scheduler::Job::new_async(cron_expression, on_fire)
            .map_err(|e| anyhow::anyhow!("Invalid cron expression '{}': {}", cron_expression, e))?;

        let job_uuid = job.guid();
        self.job_scheduler
            .add(job)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to add job: {}", e))?;

        self.job_ids.insert(task_id.to_string(), job_uuid);

        tracing::info!(
            "Registered cron job for task '{}' (job_uuid: {})",
            task_id,
            job_uuid
        );
        Ok(())
    }

    /// Remove a job by task ID.
    pub async fn remove_job(&mut self, task_id: &str) -> anyhow::Result<()> {
        if let Some(job_uuid) = self.job_ids.remove(task_id) {
            self.job_scheduler
                .remove(&job_uuid)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to remove job: {}", e))?;

            tracing::info!(
                "Removed cron job for task '{}' (job_uuid: {})",
                task_id,
                job_uuid
            );
        } else {
            tracing::debug!(
                "No cron job found for task '{}' (may not be registered)",
                task_id
            );
        }
        Ok(())
    }

    /// Check whether a task is currently registered as a job.
    pub fn has_job(&self, task_id: &str) -> bool {
        self.job_ids.contains_key(task_id)
    }

    /// Return the number of registered jobs.
    pub fn job_count(&self) -> usize {
        self.job_ids.len()
    }
}

/// Validate a cron expression without creating a job.
/// Returns `Ok(())` if valid, or an error message.
pub fn validate_cron_expression(expr: &str) -> Result<(), String> {
    // tokio-cron-scheduler uses 7-field cron: sec min hour day month weekday year
    // but also accepts 5-field (min hour day month weekday) and 6-field formats.
    // We try to parse it via croner (used internally by tokio-cron-scheduler).
    expr.parse::<croner::Cron>()
        .map(|_| ())
        .map_err(|e| format!("Invalid cron expression '{}': {}", expr, e))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_cron_valid() {
        // Every minute
        assert!(validate_cron_expression("* * * * *").is_ok());
        // Every day at 8:00
        assert!(validate_cron_expression("0 8 * * *").is_ok());
        // Every Monday at 9:30
        assert!(validate_cron_expression("30 9 * * 1").is_ok());
        // With seconds (6-field)
        assert!(validate_cron_expression("0 0 8 * * *").is_ok());
    }

    #[test]
    fn test_validate_cron_invalid() {
        assert!(validate_cron_expression("not a cron").is_err());
        assert!(validate_cron_expression("").is_err());
        assert!(validate_cron_expression("99 99 99 99 99").is_err());
    }

    #[test]
    fn test_scheduled_task_serialization() {
        let task = ScheduledTask {
            id: "test-id".to_string(),
            instance_id: "inst-1".to_string(),
            name: "morning-reminder".to_string(),
            cron_expression: "0 8 * * *".to_string(),
            task_prompt: "Remind me to check emails".to_string(),
            enabled: true,
            notify: true,
            last_run: None,
            last_result: None,
            created_at: Utc::now(),
        };

        let json = serde_json::to_string(&task).unwrap();
        let deserialized: ScheduledTask = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "test-id");
        assert_eq!(deserialized.name, "morning-reminder");
        assert!(deserialized.enabled);
    }

    #[tokio::test]
    async fn test_scheduler_creation() {
        let scheduler = Scheduler::new().await.unwrap();
        assert_eq!(scheduler.job_count(), 0);
    }

    #[tokio::test]
    async fn test_scheduler_has_job() {
        // Test the HashMap logic for job tracking
        let scheduler = Scheduler {
            job_scheduler: JobScheduler::new().await.unwrap(),
            job_ids: {
                let mut m = HashMap::new();
                m.insert("task-1".to_string(), uuid::Uuid::new_v4());
                m
            },
        };

        assert!(scheduler.has_job("task-1"));
        assert!(!scheduler.has_job("task-2"));
        assert_eq!(scheduler.job_count(), 1);
    }
}
