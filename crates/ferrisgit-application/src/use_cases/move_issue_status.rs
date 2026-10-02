use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::issue::{Issue, IssueStatus, IssueStorePort};
use uuid::Uuid;

pub struct MoveIssueStatusUseCase {
    issues: Arc<dyn IssueStorePort>,
}

impl MoveIssueStatusUseCase {
    pub fn new(issues: Arc<dyn IssueStorePort>) -> Self {
        Self { issues }
    }

    pub async fn execute(&self, issue_id: Uuid, status: IssueStatus) -> Result<Issue, DomainError> {
        self.issues.update_status(issue_id, status).await?;
        self.issues
            .find_by_id(issue_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("issue".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeIssues;
    use crate::use_cases::fixtures;

    fn issue(id: Uuid) -> Issue {
        Issue {
            id,
            ..fixtures::issue(Uuid::new_v4(), Uuid::new_v4())
        }
    }

    #[tokio::test]
    async fn moving_an_issue_updates_its_status() {
        let issue_id = Uuid::new_v4();
        let store = Arc::new(FakeIssues::new(vec![issue(issue_id)]));
        let use_case = MoveIssueStatusUseCase::new(store);

        let moved = use_case
            .execute(issue_id, IssueStatus::InReview)
            .await
            .unwrap();

        assert_eq!(moved.status, IssueStatus::InReview);
    }

    #[tokio::test]
    async fn moving_an_unknown_issue_is_not_found() {
        let store = Arc::new(FakeIssues::empty());
        let use_case = MoveIssueStatusUseCase::new(store);

        let result = use_case
            .execute(Uuid::new_v4(), IssueStatus::InReview)
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }
}
