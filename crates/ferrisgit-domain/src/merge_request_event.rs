use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeRequestEventKind {
    ReviewSubmitted,
    LabelsChanged,
    MilestoneChanged,
    TitleChanged,
    CommitsPushed,
    Merged,
    Closed,
    ThreadResolved,
    ThreadReopened,
}

impl MergeRequestEventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReviewSubmitted => "review_submitted",
            Self::LabelsChanged => "labels_changed",
            Self::MilestoneChanged => "milestone_changed",
            Self::TitleChanged => "title_changed",
            Self::CommitsPushed => "commits_pushed",
            Self::Merged => "merged",
            Self::Closed => "closed",
            Self::ThreadResolved => "thread_resolved",
            Self::ThreadReopened => "thread_reopened",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "review_submitted" => Ok(Self::ReviewSubmitted),
            "labels_changed" => Ok(Self::LabelsChanged),
            "milestone_changed" => Ok(Self::MilestoneChanged),
            "title_changed" => Ok(Self::TitleChanged),
            "commits_pushed" => Ok(Self::CommitsPushed),
            "merged" => Ok(Self::Merged),
            "closed" => Ok(Self::Closed),
            "thread_resolved" => Ok(Self::ThreadResolved),
            "thread_reopened" => Ok(Self::ThreadReopened),
            other => Err(DomainError::Validation(format!(
                "unknown merge request event kind: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MergeRequestEvent {
    pub id: Uuid,
    pub merge_request_id: Uuid,
    pub actor_id: Option<Uuid>,
    pub kind: MergeRequestEventKind,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

pub struct NewMergeRequestEvent {
    pub merge_request_id: Uuid,
    pub actor_id: Option<Uuid>,
    pub kind: MergeRequestEventKind,
    pub payload: serde_json::Value,
}

#[async_trait]
pub trait MergeRequestEventPort: Send + Sync {
    async fn record(&self, event: NewMergeRequestEvent) -> Result<MergeRequestEvent, DomainError>;
    /// Events of one merge request, oldest first (`created_at ASC, id ASC`).
    async fn list(&self, merge_request_id: Uuid) -> Result<Vec<MergeRequestEvent>, DomainError>;
    /// The source-branch head last seen for this merge request, `None` until first recorded.
    async fn head_sha(&self, merge_request_id: Uuid) -> Result<Option<String>, DomainError>;
    async fn set_head_sha(&self, merge_request_id: Uuid, sha: &str) -> Result<(), DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_KINDS: [MergeRequestEventKind; 9] = [
        MergeRequestEventKind::ReviewSubmitted,
        MergeRequestEventKind::LabelsChanged,
        MergeRequestEventKind::MilestoneChanged,
        MergeRequestEventKind::TitleChanged,
        MergeRequestEventKind::CommitsPushed,
        MergeRequestEventKind::Merged,
        MergeRequestEventKind::Closed,
        MergeRequestEventKind::ThreadResolved,
        MergeRequestEventKind::ThreadReopened,
    ];

    #[test]
    fn every_kind_round_trips_through_its_string_form() {
        for kind in ALL_KINDS {
            assert_eq!(MergeRequestEventKind::parse(kind.as_str()).unwrap(), kind);
        }
    }

    #[test]
    fn parsing_an_unknown_kind_is_a_validation_error() {
        assert!(matches!(
            MergeRequestEventKind::parse("bogus"),
            Err(DomainError::Validation(_))
        ));
    }
}
