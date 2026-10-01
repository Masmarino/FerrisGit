use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::repository::RepositoryStorePort;
use uuid::Uuid;

pub struct DeleteGroupUseCase {
    groups: Arc<dyn GroupStorePort>,
    repositories: Arc<dyn RepositoryStorePort>,
}

impl DeleteGroupUseCase {
    pub fn new(
        groups: Arc<dyn GroupStorePort>,
        repositories: Arc<dyn RepositoryStorePort>,
    ) -> Self {
        Self {
            groups,
            repositories,
        }
    }

    /// Rejects a non-empty group (child group or repository) with `Conflict` rather than cascading, although the FKs
    /// are `ON DELETE CASCADE`.
    ///
    /// The `list_children`/`list_for_group` checks and the delete are separate round-trips. The checks give the
    /// specific errors for the common case, and the delete goes through `delete_if_empty`, which re-checks emptiness
    /// atomically. A `false` from it means the rare race, reported as its own `Conflict`.
    pub async fn execute(&self, group_id: Uuid) -> Result<(), DomainError> {
        let children = self.groups.list_children(Some(group_id)).await?;
        if !children.is_empty() {
            return Err(DomainError::Conflict(
                "this group still has at least one subgroup — remove it first".to_string(),
            ));
        }
        let repos = self.repositories.list_for_group(group_id).await?;
        if !repos.is_empty() {
            return Err(DomainError::Conflict(
                "this group still has at least one repository — remove it first".to_string(),
            ));
        }
        let deleted = self.groups.delete_if_empty(group_id).await?;
        if !deleted {
            return Err(DomainError::Conflict(
                "the group changed while being deleted — try again".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeGroups, FakeRepositories};
    use chrono::Utc;
    use ferrisgit_domain::group::Group;
    use ferrisgit_domain::repository::{Repository, RepositoryVisibility};

    fn repository() -> Repository {
        Repository {
            id: Uuid::new_v4(),
            owner_id: Uuid::new_v4(),
            name: "hello".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "g/hello.git".to_string(),
            visibility: RepositoryVisibility::Private,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn deletes_an_empty_group() {
        let group_id = Uuid::new_v4();
        let groups = Arc::new(FakeGroups::empty());
        let use_case =
            DeleteGroupUseCase::new(groups.clone(), Arc::new(FakeRepositories::new(vec![])));

        use_case.execute(group_id).await.unwrap();

        assert_eq!(groups.deleted_ids().as_slice(), &[group_id]);
    }

    #[tokio::test]
    async fn refuses_to_delete_a_group_with_a_child_group() {
        let group_id = Uuid::new_v4();
        // A real child of `group_id`, so the assertion shows that `list_children` filters by the group being deleted.
        let child = Group {
            id: Uuid::new_v4(),
            parent_group_id: Some(group_id),
            name: "child".to_string(),
            description: String::new(),
            created_by: Some(Uuid::new_v4()),
            created_at: Utc::now(),
        };
        let groups = Arc::new(FakeGroups::new(vec![child]));
        let use_case =
            DeleteGroupUseCase::new(groups.clone(), Arc::new(FakeRepositories::new(vec![])));

        let result = use_case.execute(group_id).await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
        assert!(groups.deleted_ids().is_empty());
    }

    #[tokio::test]
    async fn refuses_to_delete_a_group_with_a_repository() {
        let group_id = Uuid::new_v4();
        let groups = Arc::new(FakeGroups::empty());
        let use_case = DeleteGroupUseCase::new(
            groups.clone(),
            Arc::new(FakeRepositories::new(vec![Repository {
                group_id: Some(group_id),
                ..repository()
            }])),
        );

        let result = use_case.execute(group_id).await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
        assert!(groups.deleted_ids().is_empty());
    }

    /// When `delete_if_empty` reports the group was no longer empty (something was created between the checks and the
    /// delete), a dedicated race `Conflict` is returned instead of cascading.
    #[tokio::test]
    async fn surfaces_a_race_detected_conflict_when_the_group_stopped_being_empty_just_before_the_atomic_delete()
     {
        let group_id = Uuid::new_v4();
        let groups = Arc::new(FakeGroups::empty());
        // The fake cannot see `RepositoryStorePort`, so the race is simulated by forcing this call's result.
        groups.force_delete_if_empty(false);
        let use_case =
            DeleteGroupUseCase::new(groups.clone(), Arc::new(FakeRepositories::new(vec![])));

        let result = use_case.execute(group_id).await;

        match result {
            Err(DomainError::Conflict(message)) => {
                assert!(
                    message.contains("changed") || message.contains("try again"),
                    "expected a race-detected message distinct from the non-empty-group message, got: {message}"
                );
            }
            other => panic!(
                "expected a Conflict when delete_if_empty reports it did not delete, got {other:?}"
            ),
        }
        assert!(
            groups.deleted_ids().is_empty(),
            "delete_if_empty returning false must not be recorded as a successful delete"
        );
    }
}
