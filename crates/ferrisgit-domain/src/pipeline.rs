use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::error::DomainError;
use crate::settings::ExecutionEngine;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PipelineStatus {
    Pending,
    Running,
    Success,
    Failed,
    Canceled,
}

impl PipelineStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            PipelineStatus::Pending => "pending",
            PipelineStatus::Running => "running",
            PipelineStatus::Success => "success",
            PipelineStatus::Failed => "failed",
            PipelineStatus::Canceled => "canceled",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "pending" => Ok(PipelineStatus::Pending),
            "running" => Ok(PipelineStatus::Running),
            "success" => Ok(PipelineStatus::Success),
            "failed" => Ok(PipelineStatus::Failed),
            "canceled" => Ok(PipelineStatus::Canceled),
            other => Err(DomainError::Validation(format!(
                "unknown pipeline status: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Pipeline {
    pub id: Uuid,
    pub repository_id: Uuid,
    pub commit_sha: String,
    pub execution_engine: ExecutionEngine,
    pub status: PipelineStatus,
    pub triggered_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    /// Why the pipeline file couldn't become jobs. Only set on a `Failed` pipeline with no jobs.
    pub error: Option<String>,
    /// Set on a merge request pipeline, which runs on the source branch merged into the target one.
    pub merge_request: Option<MergeRequestPipeline>,
}

/// What a merge request pipeline ran on: `commit_sha` is the merge of the two branches, a commit no branch points to,
/// and `source_sha` the tip of the source branch it was made from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MergeRequestPipeline {
    pub merge_request_id: Uuid,
    pub source_sha: String,
}

pub struct NewPipeline {
    pub repository_id: Uuid,
    pub commit_sha: String,
    pub execution_engine: ExecutionEngine,
    pub triggered_by: Uuid,
    pub merge_request: Option<MergeRequestPipeline>,
}

impl PipelineStatus {
    /// Whether the pipeline can still change: `Pending` or `Running`.
    pub fn is_active(&self) -> bool {
        matches!(self, PipelineStatus::Pending | PipelineStatus::Running)
    }
}

#[async_trait]
pub trait PipelineStorePort: Send + Sync {
    async fn create(&self, new_pipeline: NewPipeline) -> Result<Pipeline, DomainError>;
    /// A pipeline that is already failed and finished because its file is invalid, `error` being the parser's message.
    async fn create_failed(
        &self,
        new_pipeline: NewPipeline,
        error: &str,
    ) -> Result<Pipeline, DomainError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Pipeline>, DomainError>;
    async fn list_for_repository(&self, repository_id: Uuid) -> Result<Vec<Pipeline>, DomainError>;
    async fn update_status(&self, id: Uuid, status: PipelineStatus) -> Result<(), DomainError>;
    /// `Pending` to `Running`, and only from `Pending`: a finished or canceled pipeline stays as it is. `true` if it changed,
    /// so the event is published once.
    async fn mark_running(&self, id: Uuid) -> Result<bool, DomainError>;
    async fn count_created_since(&self, since: DateTime<Utc>) -> Result<i64, DomainError>;
    /// The pipelines of a merge request, newest first.
    async fn list_for_merge_request(
        &self,
        merge_request_id: Uuid,
    ) -> Result<Vec<Pipeline>, DomainError>;
    /// The newest pipeline of each of the merge requests that have one.
    async fn latest_for_merge_requests(
        &self,
        merge_request_ids: &[Uuid],
    ) -> Result<Vec<Pipeline>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_round_trips_through_its_string_form() {
        assert_eq!(
            PipelineStatus::parse("running").unwrap(),
            PipelineStatus::Running
        );
        assert_eq!(PipelineStatus::Success.as_str(), "success");
    }

    #[test]
    fn parsing_an_unknown_status_is_a_validation_error() {
        assert!(matches!(
            PipelineStatus::parse("bogus"),
            Err(DomainError::Validation(_))
        ));
    }
}
