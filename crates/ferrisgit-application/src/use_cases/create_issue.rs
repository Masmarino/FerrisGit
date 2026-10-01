use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::issue::{Issue, IssueKind, IssueStorePort, NewIssue};
use uuid::Uuid;

pub struct CreateIssueUseCase {
    issues: Arc<dyn IssueStorePort>,
}

impl CreateIssueUseCase {
    pub fn new(issues: Arc<dyn IssueStorePort>) -> Self {
        Self { issues }
    }

    pub async fn execute(
        &self,
        repository_id: Uuid,
        author_id: Uuid,
        title: String,
        description: String,
        kind: IssueKind,
    ) -> Result<Issue, DomainError> {
        self.issues
            .create(NewIssue {
                repository_id,
                author_id,
                title,
                description,
                kind,
                parent_issue_id: None,
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeIssues;
    use ferrisgit_domain::issue::IssueStatus;

    #[tokio::test]
    async fn creating_an_issue_stores_it_as_open_with_no_assignee() {
        let repository_id = Uuid::new_v4();
        let author_id = Uuid::new_v4();
        let use_case = CreateIssueUseCase::new(Arc::new(FakeIssues::empty()));

        let issue = use_case
            .execute(
                repository_id,
                author_id,
                "Bug in login".to_string(),
                "Steps to repro...".to_string(),
                IssueKind::Bug,
            )
            .await
            .unwrap();

        assert_eq!(issue.repository_id, repository_id);
        assert_eq!(issue.author_id, author_id);
        assert_eq!(issue.title, "Bug in login");
        assert_eq!(issue.status, IssueStatus::Todo);
        assert_eq!(issue.kind, IssueKind::Bug);
        assert_eq!(issue.assignee_id, None);
        assert_eq!(issue.number, 1);
    }
}
