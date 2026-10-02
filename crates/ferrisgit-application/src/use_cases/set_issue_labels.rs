use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::issue::IssueStorePort;
use ferrisgit_domain::label::{Label, LabelStorePort};
use ferrisgit_domain::repository::Repository;
use uuid::Uuid;

use super::require_in_scope::require_labels_in_scope;

pub struct SetIssueLabelsUseCase {
    issues: Arc<dyn IssueStorePort>,
    labels: Arc<dyn LabelStorePort>,
    groups: Arc<dyn GroupStorePort>,
}

impl SetIssueLabelsUseCase {
    pub fn new(
        issues: Arc<dyn IssueStorePort>,
        labels: Arc<dyn LabelStorePort>,
        groups: Arc<dyn GroupStorePort>,
    ) -> Self {
        Self {
            issues,
            labels,
            groups,
        }
    }

    pub async fn execute(
        &self,
        issue_id: Uuid,
        repository: &Repository,
        label_ids: Vec<Uuid>,
    ) -> Result<Vec<Label>, DomainError> {
        self.issues
            .find_by_id(issue_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("issue".to_string()))?;

        require_labels_in_scope(self.labels.as_ref(), &self.groups, repository, &label_ids).await?;
        self.labels
            .set_labels_for_issue(issue_id, &label_ids)
            .await?;
        self.labels.list_for_issue(issue_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeGroups, FakeIssues, FakeLabels};
    use crate::use_cases::fixtures;
    use ferrisgit_domain::issue::Issue;
    use ferrisgit_domain::label::NewLabel;

    fn repository(id: Uuid) -> Repository {
        fixtures::repository_with_id(id, Uuid::new_v4())
    }

    fn issue(id: Uuid, repository_id: Uuid) -> Issue {
        Issue {
            id,
            ..fixtures::issue(repository_id, Uuid::new_v4())
        }
    }

    #[tokio::test]
    async fn setting_labels_in_scope_replaces_the_issues_label_set() {
        let repository = repository(Uuid::new_v4());
        let issue_id = Uuid::new_v4();
        let issues = Arc::new(FakeIssues::new(vec![issue(issue_id, repository.id)]));
        let labels = Arc::new(FakeLabels::empty());
        let label = labels
            .create(NewLabel {
                name: "Bug".to_string(),
                color: "#dc2626".to_string(),
                repository_id: Some(repository.id),
                group_id: None,
            })
            .await
            .unwrap();
        let groups: Arc<dyn GroupStorePort> = Arc::new(FakeGroups::empty());
        let use_case = SetIssueLabelsUseCase::new(issues, labels.clone(), groups);

        let result = use_case
            .execute(issue_id, &repository, vec![label.id])
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, label.id);
    }

    #[tokio::test]
    async fn setting_a_label_scoped_to_a_different_repository_is_a_validation_error() {
        let repository = repository(Uuid::new_v4());
        let other_repository_id = Uuid::new_v4();
        let issue_id = Uuid::new_v4();
        let issues = Arc::new(FakeIssues::new(vec![issue(issue_id, repository.id)]));
        let labels = Arc::new(FakeLabels::empty());
        let label = labels
            .create(NewLabel {
                name: "Bug".to_string(),
                color: "#dc2626".to_string(),
                repository_id: Some(other_repository_id),
                group_id: None,
            })
            .await
            .unwrap();
        let groups: Arc<dyn GroupStorePort> = Arc::new(FakeGroups::empty());
        let use_case = SetIssueLabelsUseCase::new(issues, labels, groups);

        let result = use_case
            .execute(issue_id, &repository, vec![label.id])
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn a_mix_of_in_scope_and_out_of_scope_labels_writes_nothing_and_leaves_the_existing_set_intact()
     {
        let repository = repository(Uuid::new_v4());
        let other_repository_id = Uuid::new_v4();
        let issue_id = Uuid::new_v4();
        let issues = Arc::new(FakeIssues::new(vec![issue(issue_id, repository.id)]));
        let labels = Arc::new(FakeLabels::empty());
        let existing = labels
            .create(NewLabel {
                name: "Existing".to_string(),
                color: "#2563eb".to_string(),
                repository_id: Some(repository.id),
                group_id: None,
            })
            .await
            .unwrap();
        let good = labels
            .create(NewLabel {
                name: "Good".to_string(),
                color: "#16a34a".to_string(),
                repository_id: Some(repository.id),
                group_id: None,
            })
            .await
            .unwrap();
        let bad = labels
            .create(NewLabel {
                name: "Bad".to_string(),
                color: "#dc2626".to_string(),
                repository_id: Some(other_repository_id),
                group_id: None,
            })
            .await
            .unwrap();
        let groups: Arc<dyn GroupStorePort> = Arc::new(FakeGroups::empty());
        let use_case = SetIssueLabelsUseCase::new(issues, labels.clone(), groups);

        use_case
            .execute(issue_id, &repository, vec![existing.id])
            .await
            .unwrap();

        // `good` is validated before `bad` fails, it must not be written in the meantime.
        let result = use_case
            .execute(issue_id, &repository, vec![good.id, bad.id])
            .await;
        assert!(
            matches!(result, Err(DomainError::Validation(_))),
            "a mixed batch must be rejected, got: {result:?}"
        );

        let after = labels.list_for_issue(issue_id).await.unwrap();
        assert_eq!(
            after.len(),
            1,
            "nothing must have been written — not even the in-scope label: {after:?}"
        );
        assert_eq!(
            after[0].id, existing.id,
            "the original label set must be exactly as it was"
        );
    }

    #[tokio::test]
    async fn setting_an_unknown_label_id_is_a_not_found_error() {
        let repository = repository(Uuid::new_v4());
        let issue_id = Uuid::new_v4();
        let issues = Arc::new(FakeIssues::new(vec![issue(issue_id, repository.id)]));
        let labels = Arc::new(FakeLabels::empty());
        let groups: Arc<dyn GroupStorePort> = Arc::new(FakeGroups::empty());
        let use_case = SetIssueLabelsUseCase::new(issues, labels, groups);

        let result = use_case
            .execute(issue_id, &repository, vec![Uuid::new_v4()])
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }
}
