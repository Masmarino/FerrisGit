use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueStatus {
    Todo,
    InProgress,
    InReview,
    Done,
}

impl IssueStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            IssueStatus::Todo => "todo",
            IssueStatus::InProgress => "in_progress",
            IssueStatus::InReview => "in_review",
            IssueStatus::Done => "done",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "todo" => Ok(IssueStatus::Todo),
            "in_progress" => Ok(IssueStatus::InProgress),
            "in_review" => Ok(IssueStatus::InReview),
            "done" => Ok(IssueStatus::Done),
            other => Err(DomainError::Validation(format!(
                "unknown issue status: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueKind {
    Bug,
    Feature,
    Task,
    Epic,
}

impl IssueKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            IssueKind::Bug => "bug",
            IssueKind::Feature => "feature",
            IssueKind::Task => "task",
            IssueKind::Epic => "epic",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "bug" => Ok(IssueKind::Bug),
            "feature" => Ok(IssueKind::Feature),
            "task" => Ok(IssueKind::Task),
            "epic" => Ok(IssueKind::Epic),
            other => Err(DomainError::Validation(format!(
                "unknown issue kind: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Issue {
    pub id: Uuid,
    pub repository_id: Uuid,
    pub number: i32,
    pub author_id: Uuid,
    pub assignee_id: Option<Uuid>,
    pub milestone_id: Option<Uuid>,
    pub title: String,
    pub description: String,
    pub status: IssueStatus,
    pub kind: IssueKind,
    pub parent_issue_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

pub struct NewIssue {
    pub repository_id: Uuid,
    pub author_id: Uuid,
    pub title: String,
    pub description: String,
    pub kind: IssueKind,
    pub parent_issue_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IssueComment {
    pub id: Uuid,
    pub issue_id: Uuid,
    pub author_id: Uuid,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

pub struct NewIssueComment {
    pub issue_id: Uuid,
    pub author_id: Uuid,
    pub body: String,
}

#[async_trait]
pub trait IssueStorePort: Send + Sync {
    /// Allocates the next sequential `number` for `new_issue.repository_id` and creates
    /// the issue. Callers never choose `number` themselves.
    async fn create(&self, new_issue: NewIssue) -> Result<Issue, DomainError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Issue>, DomainError>;
    async fn find_by_number(
        &self,
        repository_id: Uuid,
        number: i32,
    ) -> Result<Option<Issue>, DomainError>;
    async fn list_for_repository(&self, repository_id: Uuid) -> Result<Vec<Issue>, DomainError>;
    /// Issues in `repository_id`, optionally narrowed to those carrying at least one of
    /// `label_ids` and/or belonging to `milestone_id`. `None` for either filter means
    /// "don't narrow by that dimension".
    async fn list_for_repository_filtered(
        &self,
        repository_id: Uuid,
        label_ids: Option<Vec<Uuid>>,
        milestone_id: Option<Uuid>,
    ) -> Result<Vec<Issue>, DomainError>;
    async fn update_fields(
        &self,
        id: Uuid,
        title: String,
        description: String,
        kind: IssueKind,
    ) -> Result<(), DomainError>;
    async fn update_status(&self, id: Uuid, status: IssueStatus) -> Result<(), DomainError>;
    async fn assign(&self, id: Uuid, assignee_id: Option<Uuid>) -> Result<(), DomainError>;
    async fn set_milestone(&self, id: Uuid, milestone_id: Option<Uuid>) -> Result<(), DomainError>;
    async fn close(&self, id: Uuid) -> Result<(), DomainError>;
    async fn reopen(&self, id: Uuid) -> Result<(), DomainError>;
    async fn search(
        &self,
        repository_ids: &[Uuid],
        query: &str,
        limit: i64,
    ) -> Result<Vec<Issue>, DomainError>;

    /// Open issues (`status != Done`) assigned to `user_id`, restricted to `repository_ids`, most recent first.
    async fn list_assigned_to(
        &self,
        user_id: Uuid,
        repository_ids: &[Uuid],
        limit: i64,
    ) -> Result<Vec<Issue>, DomainError>;

    /// Open issues (`status != Done`) authored by `user_id`, restricted to `repository_ids`, most recent first.
    async fn list_authored_by(
        &self,
        user_id: Uuid,
        repository_ids: &[Uuid],
        limit: i64,
    ) -> Result<Vec<Issue>, DomainError>;
}
