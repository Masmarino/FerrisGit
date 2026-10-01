use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::issue::{Issue, IssueKind, IssueStorePort};
use ferrisgit_domain::milestone::MilestoneStorePort;
use ferrisgit_domain::repository::RepositoryStorePort;
use uuid::Uuid;

use crate::scope_check::is_in_repository_scope;

pub struct UpdateIssueUseCase {
    issues: Arc<dyn IssueStorePort>,
    repositories: Arc<dyn RepositoryStorePort>,
    milestones: Arc<dyn MilestoneStorePort>,
    groups: Arc<dyn GroupStorePort>,
}

impl UpdateIssueUseCase {
    pub fn new(
        issues: Arc<dyn IssueStorePort>,
        repositories: Arc<dyn RepositoryStorePort>,
        milestones: Arc<dyn MilestoneStorePort>,
        groups: Arc<dyn GroupStorePort>,
    ) -> Self {
        Self {
            issues,
            repositories,
            milestones,
            groups,
        }
    }

    pub async fn execute(
        &self,
        issue_id: Uuid,
        title: String,
        description: String,
        kind: IssueKind,
        milestone_id: Option<Uuid>,
    ) -> Result<Issue, DomainError> {
        let existing = self
            .issues
            .find_by_id(issue_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("issue".to_string()))?;

        if let Some(milestone_id) = milestone_id {
            let milestone = self
                .milestones
                .find_by_id(milestone_id)
                .await?
                .ok_or_else(|| DomainError::NotFound("milestone".to_string()))?;
            let repository = self
                .repositories
                .find_by_id(existing.repository_id)
                .await?
                .ok_or_else(|| DomainError::NotFound("repository".to_string()))?;
            if !is_in_repository_scope(
                &self.groups,
                &repository,
                milestone.repository_id,
                milestone.group_id,
            )
            .await?
            {
                return Err(DomainError::Validation(format!(
                    "milestone {milestone_id} is not usable on this repository"
                )));
            }
        }

        self.issues
            .update_fields(issue_id, title, description, kind)
            .await?;
        self.issues.set_milestone(issue_id, milestone_id).await?;
        self.issues
            .find_by_id(issue_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("issue".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeGroups, FakeIssues, FakeMilestones, FakeRepositories};
    use chrono::Utc;
    use ferrisgit_domain::group::Group;
    use ferrisgit_domain::issue::IssueStatus;
    use ferrisgit_domain::milestone::NewMilestone;
    use ferrisgit_domain::repository::{Repository, RepositoryVisibility};

    fn repository(id: Uuid) -> Repository {
        Repository {
            id,
            owner_id: Uuid::new_v4(),
            name: "r".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "r.git".to_string(),
            visibility: RepositoryVisibility::Private,
            created_at: Utc::now(),
        }
    }

    fn issue(id: Uuid, repository_id: Uuid) -> Issue {
        Issue {
            id,
            repository_id,
            number: 1,
            author_id: Uuid::new_v4(),
            assignee_id: None,
            milestone_id: None,
            title: "old title".to_string(),
            description: "old description".to_string(),
            status: IssueStatus::Todo,
            kind: IssueKind::Bug,
            parent_issue_id: None,
            created_at: Utc::now(),
            closed_at: None,
        }
    }

    fn use_case(
        issue_store: Arc<FakeIssues>,
        repository: Repository,
        milestones: Arc<FakeMilestones>,
        group_chain: Vec<Group>,
    ) -> UpdateIssueUseCase {
        let repositories: Arc<dyn RepositoryStorePort> =
            Arc::new(FakeRepositories::new(vec![repository]));
        let groups: Arc<dyn GroupStorePort> = Arc::new(FakeGroups::new(group_chain));
        UpdateIssueUseCase::new(issue_store, repositories, milestones, groups)
    }

    #[tokio::test]
    async fn updating_an_issue_changes_title_description_and_kind() {
        let issue_id = Uuid::new_v4();
        let repository_id = Uuid::new_v4();
        let store = Arc::new(FakeIssues::new(vec![issue(issue_id, repository_id)]));
        let use_case = use_case(
            store,
            repository(repository_id),
            Arc::new(FakeMilestones::empty()),
            vec![],
        );

        let updated = use_case
            .execute(
                issue_id,
                "new title".to_string(),
                "new description".to_string(),
                IssueKind::Feature,
                None,
            )
            .await
            .unwrap();

        assert_eq!(updated.title, "new title");
        assert_eq!(updated.description, "new description");
        assert_eq!(updated.kind, IssueKind::Feature);
    }

    #[tokio::test]
    async fn assigning_a_milestone_in_scope_sets_it_on_the_issue() {
        let issue_id = Uuid::new_v4();
        let repository_id = Uuid::new_v4();
        let store = Arc::new(FakeIssues::new(vec![issue(issue_id, repository_id)]));
        let milestones = Arc::new(FakeMilestones::empty());
        let milestone = milestones
            .create(NewMilestone {
                title: "v1".to_string(),
                description: String::new(),
                due_date: None,
                repository_id: Some(repository_id),
                group_id: None,
            })
            .await
            .unwrap();
        let use_case = use_case(store, repository(repository_id), milestones, vec![]);

        let updated = use_case
            .execute(
                issue_id,
                "t".to_string(),
                "d".to_string(),
                IssueKind::Bug,
                Some(milestone.id),
            )
            .await
            .unwrap();

        assert_eq!(updated.milestone_id, Some(milestone.id));
    }

    #[tokio::test]
    async fn assigning_a_milestone_scoped_to_a_different_repository_is_a_validation_error() {
        let issue_id = Uuid::new_v4();
        let repository_id = Uuid::new_v4();
        let other_repository_id = Uuid::new_v4();
        let store = Arc::new(FakeIssues::new(vec![issue(issue_id, repository_id)]));
        let milestones = Arc::new(FakeMilestones::empty());
        let milestone = milestones
            .create(NewMilestone {
                title: "v1".to_string(),
                description: String::new(),
                due_date: None,
                repository_id: Some(other_repository_id),
                group_id: None,
            })
            .await
            .unwrap();
        let use_case = use_case(store, repository(repository_id), milestones, vec![]);

        let result = use_case
            .execute(
                issue_id,
                "t".to_string(),
                "d".to_string(),
                IssueKind::Bug,
                Some(milestone.id),
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn assigning_an_unknown_milestone_id_is_a_not_found_error() {
        let issue_id = Uuid::new_v4();
        let repository_id = Uuid::new_v4();
        let store = Arc::new(FakeIssues::new(vec![issue(issue_id, repository_id)]));
        let use_case = use_case(
            store,
            repository(repository_id),
            Arc::new(FakeMilestones::empty()),
            vec![],
        );

        let result = use_case
            .execute(
                issue_id,
                "t".to_string(),
                "d".to_string(),
                IssueKind::Bug,
                Some(Uuid::new_v4()),
            )
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn passing_none_clears_a_previously_set_milestone() {
        let issue_id = Uuid::new_v4();
        let repository_id = Uuid::new_v4();
        let mut seed = issue(issue_id, repository_id);
        seed.milestone_id = Some(Uuid::new_v4());
        let store = Arc::new(FakeIssues::new(vec![seed]));
        let use_case = use_case(
            store,
            repository(repository_id),
            Arc::new(FakeMilestones::empty()),
            vec![],
        );

        let updated = use_case
            .execute(
                issue_id,
                "t".to_string(),
                "d".to_string(),
                IssueKind::Bug,
                None,
            )
            .await
            .unwrap();

        assert_eq!(updated.milestone_id, None);
    }
}
