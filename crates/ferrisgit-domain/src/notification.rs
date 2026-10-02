use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    MergeRequestApproved,
    MergeRequestChangesRequested,
    MergeRequestCommented,
    MergeRequestMerged,
    MergeRequestClosed,
    CollaboratorAdded,
    CollaboratorRoleChanged,
    CollaboratorRemoved,
    PipelineFailed,
    IssueAssigned,
    IssueCommented,
    IssueClosed,
}

impl NotificationKind {
    pub fn parse(s: &str) -> Result<Self, DomainError> {
        match s {
            "merge_request_approved" => Ok(NotificationKind::MergeRequestApproved),
            "merge_request_changes_requested" => Ok(NotificationKind::MergeRequestChangesRequested),
            "merge_request_commented" => Ok(NotificationKind::MergeRequestCommented),
            "merge_request_merged" => Ok(NotificationKind::MergeRequestMerged),
            "merge_request_closed" => Ok(NotificationKind::MergeRequestClosed),
            "collaborator_added" => Ok(NotificationKind::CollaboratorAdded),
            "collaborator_role_changed" => Ok(NotificationKind::CollaboratorRoleChanged),
            "collaborator_removed" => Ok(NotificationKind::CollaboratorRemoved),
            "pipeline_failed" => Ok(NotificationKind::PipelineFailed),
            "issue_assigned" => Ok(NotificationKind::IssueAssigned),
            "issue_commented" => Ok(NotificationKind::IssueCommented),
            "issue_closed" => Ok(NotificationKind::IssueClosed),
            other => Err(DomainError::Validation(format!(
                "unknown notification kind: {other}"
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            NotificationKind::MergeRequestApproved => "merge_request_approved",
            NotificationKind::MergeRequestChangesRequested => "merge_request_changes_requested",
            NotificationKind::MergeRequestCommented => "merge_request_commented",
            NotificationKind::MergeRequestMerged => "merge_request_merged",
            NotificationKind::MergeRequestClosed => "merge_request_closed",
            NotificationKind::CollaboratorAdded => "collaborator_added",
            NotificationKind::CollaboratorRoleChanged => "collaborator_role_changed",
            NotificationKind::CollaboratorRemoved => "collaborator_removed",
            NotificationKind::PipelineFailed => "pipeline_failed",
            NotificationKind::IssueAssigned => "issue_assigned",
            NotificationKind::IssueCommented => "issue_commented",
            NotificationKind::IssueClosed => "issue_closed",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Notification {
    pub id: Uuid,
    pub recipient_id: Uuid,
    pub kind: NotificationKind,
    pub repository_owner: String,
    pub repository_name: String,
    pub actor_username: Option<String>,
    pub merge_request_id: Option<Uuid>,
    pub merge_request_title: Option<String>,
    pub pipeline_id: Option<Uuid>,
    pub commit_sha: Option<String>,
    pub issue_id: Option<Uuid>,
    pub issue_number: Option<i32>,
    pub issue_title: Option<String>,
    pub role: Option<String>,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

pub struct NewNotification {
    pub recipient_id: Uuid,
    pub kind: NotificationKind,
    pub repository_owner: String,
    pub repository_name: String,
    pub actor_username: Option<String>,
    pub merge_request_id: Option<Uuid>,
    pub merge_request_title: Option<String>,
    pub pipeline_id: Option<Uuid>,
    pub commit_sha: Option<String>,
    pub issue_id: Option<Uuid>,
    pub issue_number: Option<i32>,
    pub issue_title: Option<String>,
    pub role: Option<String>,
}

/// Fire and forget: callers do `create(..).await.ok()`, since a notification that fails to write shouldn't fail the
/// action behind it.
#[async_trait]
pub trait NotificationStorePort: Send + Sync {
    async fn create(&self, notification: NewNotification) -> Result<(), DomainError>;
    /// Most recent first.
    async fn list_for_recipient(
        &self,
        recipient_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Notification>, DomainError>;
    async fn unread_count(&self, recipient_id: Uuid) -> Result<i64, DomainError>;
    /// Scoped by `recipient_id` so nobody can mark someone else's notification read by guessing an id. Succeeds even if no
    /// row matched.
    async fn mark_read(&self, notification_id: Uuid, recipient_id: Uuid)
    -> Result<(), DomainError>;
    async fn mark_all_read(&self, recipient_id: Uuid) -> Result<(), DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_kind_parse_and_as_str_round_trip() {
        for (s, kind) in [
            (
                "merge_request_approved",
                NotificationKind::MergeRequestApproved,
            ),
            (
                "merge_request_changes_requested",
                NotificationKind::MergeRequestChangesRequested,
            ),
            (
                "merge_request_commented",
                NotificationKind::MergeRequestCommented,
            ),
            ("merge_request_merged", NotificationKind::MergeRequestMerged),
            ("merge_request_closed", NotificationKind::MergeRequestClosed),
            ("collaborator_added", NotificationKind::CollaboratorAdded),
            (
                "collaborator_role_changed",
                NotificationKind::CollaboratorRoleChanged,
            ),
            (
                "collaborator_removed",
                NotificationKind::CollaboratorRemoved,
            ),
            ("pipeline_failed", NotificationKind::PipelineFailed),
            ("issue_assigned", NotificationKind::IssueAssigned),
            ("issue_commented", NotificationKind::IssueCommented),
            ("issue_closed", NotificationKind::IssueClosed),
        ] {
            assert_eq!(NotificationKind::parse(s).unwrap(), kind);
            assert_eq!(kind.as_str(), s);
        }
        assert!(matches!(
            NotificationKind::parse("bogus"),
            Err(DomainError::Validation(_))
        ));
    }
}
