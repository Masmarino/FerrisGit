use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Running,
    Success,
    Failed,
    Canceled,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Pending => "pending",
            JobStatus::Running => "running",
            JobStatus::Success => "success",
            JobStatus::Failed => "failed",
            JobStatus::Canceled => "canceled",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "pending" => Ok(JobStatus::Pending),
            "running" => Ok(JobStatus::Running),
            "success" => Ok(JobStatus::Success),
            "failed" => Ok(JobStatus::Failed),
            "canceled" => Ok(JobStatus::Canceled),
            other => Err(DomainError::Validation(format!(
                "unknown job status: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Job {
    pub id: Uuid,
    pub pipeline_id: Uuid,
    pub stage: String,
    pub name: String,
    pub image: String,
    pub script: Vec<String>,
    pub variables: std::collections::BTreeMap<String, String>,
    pub needs: Vec<String>,
    pub tags: Vec<String>,
    pub cache: Vec<String>,
    pub status: JobStatus,
    pub runner_id: Option<Uuid>,
    pub logs: String,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

pub struct NewJob {
    pub pipeline_id: Uuid,
    pub stage: String,
    pub name: String,
    pub image: String,
    pub script: Vec<String>,
    pub variables: std::collections::BTreeMap<String, String>,
    pub needs: Vec<String>,
    pub tags: Vec<String>,
    pub cache: Vec<String>,
}

#[async_trait]
pub trait JobStorePort: Send + Sync {
    async fn create(&self, new_job: NewJob) -> Result<Job, DomainError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Job>, DomainError>;
    async fn list_for_pipeline(&self, pipeline_id: Uuid) -> Result<Vec<Job>, DomainError>;
    /// Eligible if the job's `tags` is empty or a subset of `runner_tags` and all its `needs` are `success`. Sets it
    /// `running`. `None` if nothing is claimable.
    async fn claim_next(
        &self,
        runner_id: Uuid,
        runner_tags: &[String],
    ) -> Result<Option<Job>, DomainError>;
    async fn append_logs(&self, id: Uuid, chunk: &str) -> Result<(), DomainError>;
    /// Moves a job to `status` only from a non-terminal state. A terminal job is left untouched and `false` is returned
    /// (also when the job doesn't exist). This keeps a cancellation final: a late runner report must not bring the job
    /// back.
    async fn update_status(&self, id: Uuid, status: JobStatus) -> Result<bool, DomainError>;
    /// Puts every `running` job claimed by `runner_id` back to `pending` (runner revoked): `jobs.runner_id` is `ON
    /// DELETE SET NULL` and `claim_next` only looks at `pending`, so such a job would be stranded. Returns how many
    /// were released.
    async fn release_jobs_claimed_by(&self, runner_id: Uuid) -> Result<u64, DomainError>;
    /// Across every pipeline; enforces `system_settings.max_concurrent_jobs`.
    async fn count_running(&self) -> Result<i64, DomainError>;
    /// Pending jobs whose `needs` are all `success`, without claiming them (unlike `claim_next`). Lets an engine with
    /// no polling of its own, like Kubernetes, progress.
    async fn list_runnable(&self, pipeline_id: Uuid) -> Result<Vec<Job>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_round_trips_through_its_string_form() {
        assert_eq!(JobStatus::parse("failed").unwrap(), JobStatus::Failed);
        assert_eq!(JobStatus::Canceled.as_str(), "canceled");
    }

    #[test]
    fn parsing_an_unknown_status_is_a_validation_error() {
        assert!(matches!(
            JobStatus::parse("bogus"),
            Err(DomainError::Validation(_))
        ));
    }
}
