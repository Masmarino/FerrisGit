use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::repository::Repository;
use uuid::Uuid;

/// True if a label or milestone scoped to `(scope_repository_id, scope_group_id)` may be attached to something in
/// `repository`. Exactly one of the two is `Some` (the DB's XOR constraint). A repository scope must match exactly. A
/// group scope matches when the repository's group is that group or one of its descendants (an ancestor-chain walk, as
/// in `authz.rs`).
pub async fn is_in_repository_scope(
    groups: &Arc<dyn GroupStorePort>,
    repository: &Repository,
    scope_repository_id: Option<Uuid>,
    scope_group_id: Option<Uuid>,
) -> Result<bool, DomainError> {
    if let Some(scope_repository_id) = scope_repository_id {
        return Ok(scope_repository_id == repository.id);
    }
    let Some(scope_group_id) = scope_group_id else {
        return Ok(false);
    };
    let Some(repository_group_id) = repository.group_id else {
        return Ok(false);
    };
    let chain = groups.ancestor_chain(repository_group_id).await?;
    Ok(chain.iter().any(|g| g.id == scope_group_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeGroups;
    use ferrisgit_domain::group::Group;
    use ferrisgit_domain::repository::RepositoryVisibility;

    fn fake_repository(id: Uuid, group_id: Option<Uuid>) -> Repository {
        Repository {
            id,
            owner_id: Uuid::new_v4(),
            name: "r".to_string(),
            group_id,
            description: String::new(),
            disk_path: "r.git".to_string(),
            visibility: RepositoryVisibility::Private,
            created_at: chrono::Utc::now(),
        }
    }

    fn fake_group(id: Uuid, parent_group_id: Option<Uuid>) -> Group {
        Group {
            id,
            parent_group_id,
            name: "g".to_string(),
            description: String::new(),
            created_by: Some(Uuid::new_v4()),
            created_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn a_repository_scoped_resource_is_only_valid_on_that_exact_repository() {
        let groups: Arc<dyn GroupStorePort> = Arc::new(FakeGroups::empty());
        let repo_a = Uuid::new_v4();
        let repo_b = Uuid::new_v4();
        let repository = fake_repository(repo_b, None);

        assert!(
            is_in_repository_scope(&groups, &repository, Some(repo_b), None)
                .await
                .unwrap()
        );
        assert!(
            !is_in_repository_scope(&groups, &repository, Some(repo_a), None)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn a_group_scoped_resource_is_valid_when_the_group_is_in_the_repositorys_ancestor_chain()
    {
        let target_group = Uuid::new_v4();
        let repository_group = Uuid::new_v4();
        // `repository_group` must be a real descendant of `target_group`, so the test exercises an actual hierarchy
        // walk.
        let groups: Arc<dyn GroupStorePort> = Arc::new(FakeGroups::new(vec![
            fake_group(target_group, None),
            fake_group(repository_group, Some(target_group)),
        ]));
        let repository = fake_repository(Uuid::new_v4(), Some(repository_group));

        assert!(
            is_in_repository_scope(&groups, &repository, None, Some(target_group))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn a_group_scoped_resource_is_invalid_when_the_group_is_outside_the_ancestor_chain() {
        let unrelated_group = Uuid::new_v4();
        let repository_group = Uuid::new_v4();
        let groups: Arc<dyn GroupStorePort> =
            Arc::new(FakeGroups::new(vec![fake_group(repository_group, None)]));
        let repository = fake_repository(Uuid::new_v4(), Some(repository_group));

        assert!(
            !is_in_repository_scope(&groups, &repository, None, Some(unrelated_group))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn a_group_scoped_resource_is_invalid_on_a_personal_repository() {
        let groups: Arc<dyn GroupStorePort> = Arc::new(FakeGroups::empty());
        let repository = fake_repository(Uuid::new_v4(), None);

        assert!(
            !is_in_repository_scope(&groups, &repository, None, Some(Uuid::new_v4()))
                .await
                .unwrap()
        );
    }
}
