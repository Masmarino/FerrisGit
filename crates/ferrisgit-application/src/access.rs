use std::collections::HashSet;
use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::repository_collaborator::RepositoryCollaboratorStorePort;
use uuid::Uuid;

/// Every repository the user can see: owned, collaborated on, reached through a group (inherited roles included) or
/// public. Deduplicated, in no particular order, ids only.
///
/// Public ones are the difference with `GET /repositories`, which leaves them to search instead of filling every
/// dashboard. For negative filters such as "awaiting my review" use `member_repository_ids`, or the public set would
/// defeat the filter.
pub async fn visible_repository_ids(
    repositories: &Arc<dyn RepositoryStorePort>,
    repository_collaborators: &Arc<dyn RepositoryCollaboratorStorePort>,
    groups: &Arc<dyn GroupStorePort>,
    user_id: Uuid,
) -> Result<Vec<Uuid>, DomainError> {
    let mut ids =
        member_repository_ids(repositories, repository_collaborators, groups, user_id).await?;
    ids.extend(repositories.list_public().await?);

    let mut seen = HashSet::new();
    ids.retain(|id| seen.insert(*id));
    Ok(ids)
}

/// Repositories the user is tied to directly: owned, collaborated on or through a group (inherited included). Public
/// ones are left out, unlike `visible_repository_ids`, because a negative filter like "not yet reviewed" over the public
/// set means "everything on the instance I haven't touched". Deduplicated, in no particular order.
pub async fn member_repository_ids(
    repositories: &Arc<dyn RepositoryStorePort>,
    repository_collaborators: &Arc<dyn RepositoryCollaboratorStorePort>,
    groups: &Arc<dyn GroupStorePort>,
    user_id: Uuid,
) -> Result<Vec<Uuid>, DomainError> {
    let mut ids: Vec<Uuid> = repositories
        .list_for_owner(user_id)
        .await?
        .into_iter()
        .map(|r| r.id)
        .collect();
    ids.extend(
        repository_collaborators
            .list_repositories_for_collaborator(user_id)
            .await?,
    );

    for group_id in groups.list_member_group_ids(user_id).await? {
        let repos = repositories.list_for_group(group_id).await?;
        ids.extend(repos.into_iter().map(|r| r.id));
    }

    let mut seen = HashSet::new();
    ids.retain(|id| seen.insert(*id));
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeCollaborators, FakeGroups, FakeRepositories};
    use chrono::Utc;
    use ferrisgit_domain::group::Group;
    use ferrisgit_domain::group_membership::GroupMembershipPort;
    use ferrisgit_domain::repository::{Repository, RepositoryVisibility};
    use ferrisgit_domain::repository_collaborator::CollaboratorRole;

    /// The user has a direct role (any) on each of `group_ids`, and no other groups exist.
    async fn groups_with_member(user_id: Uuid, group_ids: &[Uuid]) -> Arc<dyn GroupStorePort> {
        let groups = FakeGroups::new(
            group_ids
                .iter()
                .map(|&id| Group {
                    id,
                    parent_group_id: None,
                    name: "g".to_string(),
                    description: String::new(),
                    created_by: Some(Uuid::new_v4()),
                    created_at: Utc::now(),
                })
                .collect(),
        );
        for &id in group_ids {
            groups
                .add_member(id, user_id, CollaboratorRole::Reader)
                .await
                .unwrap();
        }
        Arc::new(groups)
    }

    fn repo(id: Uuid, owner_id: Uuid, group_id: Option<Uuid>) -> Repository {
        Repository {
            id,
            owner_id,
            name: "r".to_string(),
            group_id,
            description: String::new(),
            disk_path: "r.git".to_string(),
            visibility: RepositoryVisibility::Private,
            created_at: Utc::now(),
        }
    }

    fn public_repo(id: Uuid, owner_id: Uuid) -> Repository {
        Repository {
            visibility: RepositoryVisibility::Public,
            ..repo(id, owner_id, None)
        }
    }

    #[tokio::test]
    async fn includes_owned_collaborated_and_group_repositories() {
        let user_id = Uuid::new_v4();
        let owned_id = Uuid::new_v4();
        let collaborated_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();
        let group_repo_id = Uuid::new_v4();
        let other_owner = Uuid::new_v4();

        let repositories: Arc<dyn RepositoryStorePort> = Arc::new(FakeRepositories::new(vec![
            repo(owned_id, user_id, None),
            repo(collaborated_id, other_owner, None),
            repo(group_repo_id, other_owner, Some(group_id)),
        ]));
        let collaborators: Arc<dyn RepositoryCollaboratorStorePort> =
            Arc::new(FakeCollaborators::new(vec![(
                collaborated_id,
                user_id,
                CollaboratorRole::Contributor,
            )]));
        let groups = groups_with_member(user_id, &[group_id]).await;

        let mut ids = visible_repository_ids(&repositories, &collaborators, &groups, user_id)
            .await
            .unwrap();
        ids.sort();
        let mut expected = vec![owned_id, collaborated_id, group_repo_id];
        expected.sort();
        assert_eq!(ids, expected);
    }

    #[tokio::test]
    async fn a_repository_reachable_both_as_a_direct_collaborator_and_through_a_group_appears_once()
    {
        let user_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();
        let shared_id = Uuid::new_v4();
        let other_owner = Uuid::new_v4();

        let repositories: Arc<dyn RepositoryStorePort> =
            Arc::new(FakeRepositories::new(vec![repo(
                shared_id,
                other_owner,
                Some(group_id),
            )]));
        let collaborators: Arc<dyn RepositoryCollaboratorStorePort> = Arc::new(
            FakeCollaborators::new(vec![(shared_id, user_id, CollaboratorRole::Contributor)]),
        );
        let groups = groups_with_member(user_id, &[group_id]).await;

        let ids = visible_repository_ids(&repositories, &collaborators, &groups, user_id)
            .await
            .unwrap();
        assert_eq!(ids, vec![shared_id]);
    }

    #[tokio::test]
    async fn a_user_with_no_access_gets_an_empty_list() {
        let user_id = Uuid::new_v4();
        let repositories: Arc<dyn RepositoryStorePort> = Arc::new(FakeRepositories::new(vec![]));
        let collaborators: Arc<dyn RepositoryCollaboratorStorePort> =
            Arc::new(FakeCollaborators::empty());
        let groups = groups_with_member(user_id, &[]).await;

        let ids = visible_repository_ids(&repositories, &collaborators, &groups, user_id)
            .await
            .unwrap();
        assert!(ids.is_empty());
    }

    #[tokio::test]
    async fn a_public_repository_is_visible_with_no_owned_collaborated_or_group_relationship() {
        let user_id = Uuid::new_v4();
        let public_id = Uuid::new_v4();
        let other_owner = Uuid::new_v4();

        let repositories: Arc<dyn RepositoryStorePort> =
            Arc::new(FakeRepositories::new(vec![public_repo(
                public_id,
                other_owner,
            )]));
        let collaborators: Arc<dyn RepositoryCollaboratorStorePort> =
            Arc::new(FakeCollaborators::empty());
        let groups = groups_with_member(user_id, &[]).await;

        let ids = visible_repository_ids(&repositories, &collaborators, &groups, user_id)
            .await
            .unwrap();
        assert_eq!(ids, vec![public_id]);
    }

    #[tokio::test]
    async fn member_repository_ids_excludes_a_public_repository_with_no_membership_relationship() {
        let user_id = Uuid::new_v4();
        let public_id = Uuid::new_v4();
        let other_owner = Uuid::new_v4();

        let repositories: Arc<dyn RepositoryStorePort> =
            Arc::new(FakeRepositories::new(vec![public_repo(
                public_id,
                other_owner,
            )]));
        let collaborators: Arc<dyn RepositoryCollaboratorStorePort> =
            Arc::new(FakeCollaborators::empty());
        let groups = groups_with_member(user_id, &[]).await;

        let ids = member_repository_ids(&repositories, &collaborators, &groups, user_id)
            .await
            .unwrap();
        assert!(
            ids.is_empty(),
            "a public repo with no owned/collaborated/group relationship must not appear in the membership-only set"
        );
    }
}
