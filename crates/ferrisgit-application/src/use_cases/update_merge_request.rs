use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::merge_request::{MergeRequest, MergeRequestStorePort};
use ferrisgit_domain::milestone::MilestoneStorePort;
use ferrisgit_domain::repository::Repository;
use uuid::Uuid;

use super::require_in_scope::require_milestone_in_scope;

pub struct UpdateMergeRequestUseCase {
    merge_requests: Arc<dyn MergeRequestStorePort>,
    milestones: Arc<dyn MilestoneStorePort>,
    groups: Arc<dyn GroupStorePort>,
}

impl UpdateMergeRequestUseCase {
    pub fn new(
        merge_requests: Arc<dyn MergeRequestStorePort>,
        milestones: Arc<dyn MilestoneStorePort>,
        groups: Arc<dyn GroupStorePort>,
    ) -> Self {
        Self {
            merge_requests,
            milestones,
            groups,
        }
    }

    pub async fn execute(
        &self,
        id: Uuid,
        repository: &Repository,
        title: String,
        description: String,
        milestone_id: Option<Uuid>,
    ) -> Result<MergeRequest, DomainError> {
        if let Some(milestone_id) = milestone_id {
            require_milestone_in_scope(
                self.milestones.as_ref(),
                &self.groups,
                repository,
                milestone_id,
            )
            .await?;
        }
        self.merge_requests
            .update_fields(id, title, description)
            .await?;
        self.merge_requests.set_milestone(id, milestone_id).await?;
        self.merge_requests
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::NotFound("merge request".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeGroups, FakeMergeRequests, FakeMilestones};
    use crate::use_cases::fixtures;
    use chrono::Utc;
    use ferrisgit_domain::group::Group;
    use ferrisgit_domain::merge_request::MergeRequestStatus;
    use ferrisgit_domain::milestone::NewMilestone;

    fn repository(id: Uuid, group_id: Option<Uuid>) -> Repository {
        Repository {
            group_id,
            ..fixtures::repository_with_id(id, Uuid::new_v4())
        }
    }

    fn merge_request(id: Uuid, repository_id: Uuid) -> MergeRequest {
        MergeRequest {
            id,
            repository_id,
            author_id: Some(Uuid::new_v4()),
            source_branch: "feature".to_string(),
            target_branch: "main".to_string(),
            title: "old title".to_string(),
            description: "old description".to_string(),
            status: MergeRequestStatus::Open,
            merge_commit_sha: None,
            milestone_id: None,
            created_at: Utc::now(),
            closed_at: None,
        }
    }

    fn use_case(
        store: Arc<FakeMergeRequests>,
        repository: Repository,
        milestones: Arc<FakeMilestones>,
        group_chain: Vec<Group>,
    ) -> (UpdateMergeRequestUseCase, Repository) {
        let groups: Arc<dyn GroupStorePort> = Arc::new(FakeGroups::new(group_chain));
        (
            UpdateMergeRequestUseCase::new(store, milestones, groups),
            repository,
        )
    }

    #[tokio::test]
    async fn updating_a_merge_request_changes_title_and_description() {
        let mr_id = Uuid::new_v4();
        let repository_id = Uuid::new_v4();
        let store = Arc::new(FakeMergeRequests::new(vec![merge_request(
            mr_id,
            repository_id,
        )]));
        let (use_case, repository) = use_case(
            store,
            repository(repository_id, None),
            Arc::new(FakeMilestones::empty()),
            vec![],
        );

        let updated = use_case
            .execute(
                mr_id,
                &repository,
                "new title".to_string(),
                "new description".to_string(),
                None,
            )
            .await
            .unwrap();

        assert_eq!(updated.title, "new title");
        assert_eq!(updated.description, "new description");
    }

    #[tokio::test]
    async fn assigning_a_milestone_in_scope_sets_it_on_the_merge_request() {
        let mr_id = Uuid::new_v4();
        let repository_id = Uuid::new_v4();
        let store = Arc::new(FakeMergeRequests::new(vec![merge_request(
            mr_id,
            repository_id,
        )]));
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
        let (use_case, repository) =
            use_case(store, repository(repository_id, None), milestones, vec![]);

        let updated = use_case
            .execute(
                mr_id,
                &repository,
                "t".to_string(),
                "d".to_string(),
                Some(milestone.id),
            )
            .await
            .unwrap();

        assert_eq!(updated.milestone_id, Some(milestone.id));
    }

    #[tokio::test]
    async fn assigning_a_milestone_scoped_to_a_different_repository_is_a_validation_error() {
        let mr_id = Uuid::new_v4();
        let repository_id = Uuid::new_v4();
        let other_repository_id = Uuid::new_v4();
        let store = Arc::new(FakeMergeRequests::new(vec![merge_request(
            mr_id,
            repository_id,
        )]));
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
        let (use_case, repository) =
            use_case(store, repository(repository_id, None), milestones, vec![]);

        let result = use_case
            .execute(
                mr_id,
                &repository,
                "t".to_string(),
                "d".to_string(),
                Some(milestone.id),
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn assigning_an_unknown_milestone_id_is_a_not_found_error() {
        let mr_id = Uuid::new_v4();
        let repository_id = Uuid::new_v4();
        let store = Arc::new(FakeMergeRequests::new(vec![merge_request(
            mr_id,
            repository_id,
        )]));
        let (use_case, repository) = use_case(
            store,
            repository(repository_id, None),
            Arc::new(FakeMilestones::empty()),
            vec![],
        );

        let result = use_case
            .execute(
                mr_id,
                &repository,
                "t".to_string(),
                "d".to_string(),
                Some(Uuid::new_v4()),
            )
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn passing_none_clears_a_previously_set_milestone() {
        let mr_id = Uuid::new_v4();
        let repository_id = Uuid::new_v4();
        let mut seed = merge_request(mr_id, repository_id);
        seed.milestone_id = Some(Uuid::new_v4());
        let store = Arc::new(FakeMergeRequests::new(vec![seed]));
        let (use_case, repository) = use_case(
            store,
            repository(repository_id, None),
            Arc::new(FakeMilestones::empty()),
            vec![],
        );

        let updated = use_case
            .execute(mr_id, &repository, "t".to_string(), "d".to_string(), None)
            .await
            .unwrap();

        assert_eq!(updated.milestone_id, None);
    }
}
