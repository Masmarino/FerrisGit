use uuid::Uuid;

use crate::error::DomainError;
use crate::group::GroupStorePort;
use crate::group_membership::GroupMembershipPort;
use crate::repository::Repository;
use crate::repository_collaborator::{CollaboratorRole, RepositoryCollaboratorStorePort};

/// The highest role `user_id` effectively holds on `repo`, ignoring the public read-only bypass, which callers apply
/// first in their own terms (a role ceiling for the API, `GitAccess` for git).
///
/// On a personal repo the owner gets the top role and everyone else their collaborator role. On a group repo it's the
/// higher of the user's role up the group chain and any direct collaborator grant. `owner_id` is just the creator there,
/// not a role.
///
/// One policy shared by the API's `require_role_by_id` and `authenticate_git_request`, kept in step by
/// `authz_parity_flow.rs`.
pub async fn effective_repository_role(
    collaborators: &dyn RepositoryCollaboratorStorePort,
    groups: &dyn GroupStorePort,
    group_membership: &dyn GroupMembershipPort,
    repo: &Repository,
    user_id: Uuid,
) -> Result<Option<CollaboratorRole>, DomainError> {
    match repo.group_id {
        None => {
            if repo.owner_id == user_id {
                return Ok(Some(CollaboratorRole::Maintainer));
            }
            collaborators.get_role(repo.id, user_id).await
        }
        Some(group_id) => {
            let chain = groups.ancestor_chain(group_id).await?;
            let mut best: Option<CollaboratorRole> = None;
            for group in &chain {
                if let Some(role) = group_membership.get_member_role(group.id, user_id).await? {
                    best = Some(best.map_or(role, |b| b.max(role)));
                }
            }
            let direct = collaborators.get_role(repo.id, user_id).await?;
            Ok(match (best, direct) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (Some(a), None) | (None, Some(a)) => Some(a),
                (None, None) => None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::group::{Group, GroupMember, GroupWithPath, NewGroup};
    use crate::group_membership::GroupMembershipPort;
    use crate::repository::RepositoryVisibility;
    use crate::repository_collaborator::RepositoryCollaborator;
    use async_trait::async_trait;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::Mutex;

    fn repo(owner_id: Uuid, group_id: Option<Uuid>) -> Repository {
        Repository {
            id: Uuid::new_v4(),
            owner_id,
            name: "hello".to_string(),
            group_id,
            description: String::new(),
            disk_path: "hello.git".to_string(),
            visibility: RepositoryVisibility::Private,
            created_at: Utc::now(),
        }
    }

    struct FakeCollaborators(Mutex<HashMap<(Uuid, Uuid), CollaboratorRole>>);
    #[async_trait]
    impl RepositoryCollaboratorStorePort for FakeCollaborators {
        async fn add(
            &self,
            _repository_id: Uuid,
            _user_id: Uuid,
            _role: CollaboratorRole,
        ) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn set_role(
            &self,
            _repository_id: Uuid,
            _user_id: Uuid,
            _role: CollaboratorRole,
        ) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn remove(&self, _repository_id: Uuid, _user_id: Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn get_role(
            &self,
            repository_id: Uuid,
            user_id: Uuid,
        ) -> Result<Option<CollaboratorRole>, DomainError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(&(repository_id, user_id))
                .copied())
        }
        async fn list_for_repository(
            &self,
            _repository_id: Uuid,
        ) -> Result<Vec<RepositoryCollaborator>, DomainError> {
            unimplemented!()
        }
        async fn list_repositories_for_collaborator(
            &self,
            _user_id: Uuid,
        ) -> Result<Vec<Uuid>, DomainError> {
            unimplemented!()
        }
    }

    struct FakeGroups {
        chains: Mutex<HashMap<Uuid, Vec<Group>>>,
        member_roles: Mutex<HashMap<(Uuid, Uuid), CollaboratorRole>>,
    }
    #[async_trait]
    impl GroupStorePort for FakeGroups {
        async fn create(&self, _new_group: NewGroup) -> Result<Group, DomainError> {
            unimplemented!()
        }
        async fn find_by_id(&self, _id: Uuid) -> Result<Option<Group>, DomainError> {
            unimplemented!()
        }
        async fn find_child_by_name(
            &self,
            _parent_id: Option<Uuid>,
            _name: &str,
        ) -> Result<Option<Group>, DomainError> {
            unimplemented!()
        }
        async fn list_children(&self, _parent_id: Option<Uuid>) -> Result<Vec<Group>, DomainError> {
            unimplemented!()
        }
        async fn ancestor_chain(&self, group_id: Uuid) -> Result<Vec<Group>, DomainError> {
            Ok(self
                .chains
                .lock()
                .unwrap()
                .get(&group_id)
                .cloned()
                .unwrap_or_default())
        }
        async fn list_writable_groups(
            &self,
            _user_id: Uuid,
        ) -> Result<Vec<GroupWithPath>, DomainError> {
            unimplemented!()
        }
        async fn list_member_group_ids(&self, _user_id: Uuid) -> Result<Vec<Uuid>, DomainError> {
            unimplemented!()
        }
    }

    #[async_trait]
    impl GroupMembershipPort for FakeGroups {
        async fn add_member(
            &self,
            _group_id: Uuid,
            _user_id: Uuid,
            _role: CollaboratorRole,
        ) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn set_member_role(
            &self,
            _group_id: Uuid,
            _user_id: Uuid,
            _role: CollaboratorRole,
        ) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn remove_member(&self, _group_id: Uuid, _user_id: Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn get_member_role(
            &self,
            group_id: Uuid,
            user_id: Uuid,
        ) -> Result<Option<CollaboratorRole>, DomainError> {
            Ok(self
                .member_roles
                .lock()
                .unwrap()
                .get(&(group_id, user_id))
                .copied())
        }
        async fn list_members(&self, _group_id: Uuid) -> Result<Vec<GroupMember>, DomainError> {
            unimplemented!()
        }
    }

    fn empty_groups() -> FakeGroups {
        FakeGroups {
            chains: Mutex::new(HashMap::new()),
            member_roles: Mutex::new(HashMap::new()),
        }
    }

    #[tokio::test]
    async fn a_personal_repos_owner_gets_the_top_role_with_no_collaborator_row() {
        let owner_id = Uuid::new_v4();
        let repo = repo(owner_id, None);
        let collaborators = FakeCollaborators(Mutex::new(HashMap::new()));
        let groups = empty_groups();

        let role = effective_repository_role(&collaborators, &groups, &groups, &repo, owner_id)
            .await
            .unwrap();

        assert_eq!(role, Some(CollaboratorRole::Maintainer));
    }

    #[tokio::test]
    async fn a_personal_repos_non_owner_gets_their_direct_collaborator_role() {
        let owner_id = Uuid::new_v4();
        let collaborator_id = Uuid::new_v4();
        let repo = repo(owner_id, None);
        let mut rows = HashMap::new();
        rows.insert((repo.id, collaborator_id), CollaboratorRole::Contributor);
        let collaborators = FakeCollaborators(Mutex::new(rows));
        let groups = empty_groups();

        let role =
            effective_repository_role(&collaborators, &groups, &groups, &repo, collaborator_id)
                .await
                .unwrap();

        assert_eq!(role, Some(CollaboratorRole::Contributor));
    }

    #[tokio::test]
    async fn a_personal_repos_stranger_gets_no_role() {
        let repo = repo(Uuid::new_v4(), None);
        let collaborators = FakeCollaborators(Mutex::new(HashMap::new()));
        let groups = empty_groups();

        let role =
            effective_repository_role(&collaborators, &groups, &groups, &repo, Uuid::new_v4())
                .await
                .unwrap();

        assert_eq!(role, None);
    }

    #[tokio::test]
    async fn a_group_repos_creator_gets_no_implicit_bypass() {
        let group_id = Uuid::new_v4();
        let creator_id = Uuid::new_v4();
        let repo = repo(creator_id, Some(group_id));
        let collaborators = FakeCollaborators(Mutex::new(HashMap::new()));
        let groups = FakeGroups {
            chains: Mutex::new(HashMap::from([(
                group_id,
                vec![Group {
                    id: group_id,
                    parent_group_id: None,
                    name: "g".to_string(),
                    description: String::new(),
                    created_by: Some(creator_id),
                    created_at: Utc::now(),
                }],
            )])),
            member_roles: Mutex::new(HashMap::new()),
        };

        let role = effective_repository_role(&collaborators, &groups, &groups, &repo, creator_id)
            .await
            .unwrap();

        assert_eq!(
            role, None,
            "a group repository's owner_id is 'created by' only, not an access bypass"
        );
    }

    #[tokio::test]
    async fn a_group_repos_role_is_the_max_of_the_ancestor_chain_and_any_direct_grant() {
        let root_id = Uuid::new_v4();
        let sub_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let repo = repo(Uuid::new_v4(), Some(sub_id));
        let mut rows = HashMap::new();
        rows.insert((repo.id, user_id), CollaboratorRole::Reader);
        let collaborators = FakeCollaborators(Mutex::new(rows));
        let groups = FakeGroups {
            chains: Mutex::new(HashMap::from([(
                sub_id,
                vec![
                    Group {
                        id: root_id,
                        parent_group_id: None,
                        name: "root".to_string(),
                        description: String::new(),
                        created_by: Some(Uuid::new_v4()),
                        created_at: Utc::now(),
                    },
                    Group {
                        id: sub_id,
                        parent_group_id: Some(root_id),
                        name: "sub".to_string(),
                        description: String::new(),
                        created_by: Some(Uuid::new_v4()),
                        created_at: Utc::now(),
                    },
                ],
            )])),
            member_roles: Mutex::new(HashMap::from([(
                (root_id, user_id),
                CollaboratorRole::Maintainer,
            )])),
        };

        let role = effective_repository_role(&collaborators, &groups, &groups, &repo, user_id)
            .await
            .unwrap();

        assert_eq!(
            role,
            Some(CollaboratorRole::Maintainer),
            "Maintainer on the root ancestor must win over a lesser direct Reader grant"
        );
    }
}
