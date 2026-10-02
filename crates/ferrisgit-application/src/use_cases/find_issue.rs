use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::issue::{Issue, IssueStorePort};
use uuid::Uuid;

/// The issue, or `NotFound`. Use cases that change an issue call it again afterwards to return the stored result.
pub(crate) async fn find_issue(
    issues: &dyn IssueStorePort,
    issue_id: Uuid,
) -> Result<Issue, DomainError> {
    issues
        .find_by_id(issue_id)
        .await?
        .ok_or_else(|| DomainError::NotFound("issue".to_string()))
}
