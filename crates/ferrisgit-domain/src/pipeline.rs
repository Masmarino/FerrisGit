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
}

pub struct NewPipeline {
    pub repository_id: Uuid,
    pub commit_sha: String,
    pub execution_engine: ExecutionEngine,
    pub triggered_by: Uuid,
}

#[async_trait]
pub trait PipelineStorePort: Send + Sync {
    async fn create(&self, new_pipeline: NewPipeline) -> Result<Pipeline, DomainError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Pipeline>, DomainError>;
    async fn list_for_repository(&self, repository_id: Uuid) -> Result<Vec<Pipeline>, DomainError>;
    async fn update_status(&self, id: Uuid, status: PipelineStatus) -> Result<(), DomainError>;
    async fn count_created_since(&self, since: DateTime<Utc>) -> Result<i64, DomainError>;
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
