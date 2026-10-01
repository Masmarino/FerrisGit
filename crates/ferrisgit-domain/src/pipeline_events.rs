use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::DomainError;
use crate::job::JobStatus;
use crate::pipeline::PipelineStatus;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum PipelineEvent {
    StatusChanged { status: PipelineStatus },
}

impl PipelineEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            PipelineEvent::StatusChanged { .. } => "PipelineStatusChanged",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum JobEvent {
    StatusChanged { status: JobStatus },
}

impl JobEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            JobEvent::StatusChanged { .. } => "JobStatusChanged",
        }
    }
}

#[async_trait]
pub trait PipelineEventPublisherPort: Send + Sync {
    async fn publish_pipeline_event(
        &self,
        pipeline_id: Uuid,
        event: PipelineEvent,
    ) -> Result<(), DomainError>;
    async fn publish_job_event(&self, job_id: Uuid, event: JobEvent) -> Result<(), DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_event_type_matches_the_variant() {
        let event = PipelineEvent::StatusChanged {
            status: PipelineStatus::Running,
        };
        assert_eq!(event.event_type(), "PipelineStatusChanged");
    }

    #[test]
    fn job_event_round_trips_through_json() {
        let event = JobEvent::StatusChanged {
            status: JobStatus::Success,
        };
        let json = serde_json::to_string(&event).unwrap();
        let decoded: JobEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.event_type(), "JobStatusChanged");
    }
}
