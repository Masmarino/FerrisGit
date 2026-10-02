use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeRequestStatus {
    Open,
    Merged,
    Closed,
}

impl MergeRequestStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            MergeRequestStatus::Open => "open",
            MergeRequestStatus::Merged => "merged",
            MergeRequestStatus::Closed => "closed",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "open" => Ok(MergeRequestStatus::Open),
            "merged" => Ok(MergeRequestStatus::Merged),
            "closed" => Ok(MergeRequestStatus::Closed),
            other => Err(DomainError::Validation(format!(
                "unknown merge request status: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MergeRequest {
    pub id: Uuid,
    pub repository_id: Uuid,
    /// `None` once the author's account was deleted: the merge request outlives them.
    pub author_id: Option<Uuid>,
    pub source_branch: String,
    pub target_branch: String,
    pub title: String,
    pub description: String,
    pub status: MergeRequestStatus,
    pub merge_commit_sha: Option<String>,
    pub milestone_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

pub struct NewMergeRequest {
    pub repository_id: Uuid,
    pub author_id: Uuid,
    pub source_branch: String,
    pub target_branch: String,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Approved,
    ChangesRequested,
}

impl ReviewDecision {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "approved" => Ok(ReviewDecision::Approved),
            "changes_requested" => Ok(ReviewDecision::ChangesRequested),
            other => Err(DomainError::Validation(format!(
                "unknown review decision: {other}"
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ReviewDecision::Approved => "approved",
            ReviewDecision::ChangesRequested => "changes_requested",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MergeRequestReview {
    pub merge_request_id: Uuid,
    pub user_id: Uuid,
    pub username: String,
    pub decision: ReviewDecision,
    pub source_sha: String,
    pub created_at: DateTime<Utc>,
}

#[async_trait]
pub trait MergeRequestStorePort: Send + Sync {
    async fn create(&self, new_mr: NewMergeRequest) -> Result<MergeRequest, DomainError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<MergeRequest>, DomainError>;
    /// Optionally narrowed to those carrying at least one of `label_ids` and/or in `milestone_id`; `None` doesn't
    /// narrow by that dimension.
    async fn list_for_repository_filtered(
        &self,
        repository_id: Uuid,
        label_ids: Option<Vec<Uuid>>,
        milestone_id: Option<Uuid>,
    ) -> Result<Vec<MergeRequest>, DomainError>;
    async fn update_fields(
        &self,
        id: Uuid,
        title: String,
        description: String,
    ) -> Result<(), DomainError>;
    async fn set_milestone(&self, id: Uuid, milestone_id: Option<Uuid>) -> Result<(), DomainError>;
    async fn mark_merged(&self, id: Uuid, merge_commit_sha: &str) -> Result<(), DomainError>;
    async fn mark_closed(&self, id: Uuid) -> Result<(), DomainError>;
    async fn search(
        &self,
        repository_ids: &[Uuid],
        query: &str,
        limit: i64,
    ) -> Result<Vec<MergeRequest>, DomainError>;

    /// Open merge requests authored by `user_id`, restricted to `repository_ids`, most recent first.
    async fn list_authored_by(
        &self,
        user_id: Uuid,
        repository_ids: &[Uuid],
        limit: i64,
    ) -> Result<Vec<MergeRequest>, DomainError>;

    /// Open merge requests `user_id` did not author and has not reviewed, in `repository_ids`, most recent first.
    /// There is no "assigned reviewer" concept; this is the heuristic for "awaiting my review".
    async fn list_awaiting_review_by(
        &self,
        user_id: Uuid,
        repository_ids: &[Uuid],
        limit: i64,
    ) -> Result<Vec<MergeRequest>, DomainError>;
}

#[async_trait]
pub trait MergeRequestReviewPort: Send + Sync {
    async fn upsert_review(
        &self,
        merge_request_id: Uuid,
        user_id: Uuid,
        decision: ReviewDecision,
        source_sha: &str,
    ) -> Result<MergeRequestReview, DomainError>;
    async fn list_reviews(
        &self,
        merge_request_id: Uuid,
    ) -> Result<Vec<MergeRequestReview>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_round_trips_through_its_string_form() {
        assert_eq!(
            MergeRequestStatus::parse("merged").unwrap(),
            MergeRequestStatus::Merged
        );
        assert_eq!(MergeRequestStatus::Closed.as_str(), "closed");
    }

    #[test]
    fn parsing_an_unknown_status_is_a_validation_error() {
        assert!(matches!(
            MergeRequestStatus::parse("bogus"),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn review_decision_round_trips_through_its_string_form() {
        assert_eq!(
            ReviewDecision::parse("changes_requested").unwrap(),
            ReviewDecision::ChangesRequested
        );
        assert_eq!(ReviewDecision::Approved.as_str(), "approved");
    }

    #[test]
    fn parsing_an_unknown_review_decision_is_a_validation_error() {
        assert!(matches!(
            ReviewDecision::parse("bogus"),
            Err(DomainError::Validation(_))
        ));
    }
}
