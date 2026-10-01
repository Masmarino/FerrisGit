use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::group_membership::GroupMembershipPort;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use ferrisgit_domain::user::UserRepositoryPort;
use uuid::Uuid;

pub struct AddGroupMemberUseCase {
    groups: Arc<dyn GroupStorePort>,
    group_membership: Arc<dyn GroupMembershipPort>,
    users: Arc<dyn UserRepositoryPort>,
}

impl AddGroupMemberUseCase {
    pub fn new(
        groups: Arc<dyn GroupStorePort>,
        group_membership: Arc<dyn GroupMembershipPort>,
        users: Arc<dyn UserRepositoryPort>,
    ) -> Self {
        Self {
            groups,
            group_membership,
            users,
        }
    }

    pub async fn execute(
        &self,
        group_id: Uuid,
        caller_id: Uuid,
        username: &str,
        role: CollaboratorRole,
    ) -> Result<(), DomainError> {
        let chain = self.groups.ancestor_chain(group_id).await?;
        let mut best: Option<CollaboratorRole> = None;
        for group in &chain {
            if let Some(r) = self
                .group_membership
                .get_member_role(group.id, caller_id)
                .await?
            {
                best = Some(best.map_or(r, |b| b.max(r)));
            }
        }
        if best.is_none_or(|r| r < CollaboratorRole::Maintainer) {
            return Err(DomainError::NotFound("group".to_string()));
        }

        let target = self
            .users
            .find_by_username(username)
            .await?
            .ok_or_else(|| DomainError::Validation("no such user".to_string()))?;
        self.group_membership
            .add_member(group_id, target.id, role)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeGroups, FakeUsers};
    use chrono::Utc;
    use ferrisgit_domain::group::NewGroup;
    use ferrisgit_domain::group_membership::GroupMembershipPort;
    use ferrisgit_domain::user::User;

    fn user(username: &str) -> User {
        User {
            id: Uuid::new_v4(),
            username: username.to_string(),
            email: format!("{username}@example.com"),
            password_hash: "h".to_string(),
            is_admin: false,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn a_direct_maintainer_can_add_a_member() {
        let groups = Arc::new(FakeGroups::empty());
        let owner = user("owner");
        let target = user("alice");
        let group = groups
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: owner.id,
            })
            .await
            .unwrap();
        groups
            .add_member(group.id, owner.id, CollaboratorRole::Maintainer)
            .await
            .unwrap();
        let use_case = AddGroupMemberUseCase::new(
            groups.clone(),
            groups.clone(),
            Arc::new(FakeUsers::new(vec![owner.clone(), target.clone()])),
        );

        use_case
            .execute(group.id, owner.id, "alice", CollaboratorRole::Contributor)
            .await
            .unwrap();

        assert!(groups.has_member(group.id, target.id, CollaboratorRole::Contributor));
    }

    #[tokio::test]
    async fn a_maintainer_inherited_from_an_ancestor_two_levels_up_can_add_a_member() {
        let groups = Arc::new(FakeGroups::empty());
        let owner = user("owner");
        let target = user("alice");
        let root = groups
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: owner.id,
            })
            .await
            .unwrap();
        groups
            .add_member(root.id, owner.id, CollaboratorRole::Maintainer)
            .await
            .unwrap();
        let mid = groups
            .create(NewGroup {
                parent_group_id: Some(root.id),
                name: "backend".to_string(),
                description: String::new(),
                created_by: owner.id,
            })
            .await
            .unwrap();
        let leaf = groups
            .create(NewGroup {
                parent_group_id: Some(mid.id),
                name: "infra".to_string(),
                description: String::new(),
                created_by: owner.id,
            })
            .await
            .unwrap();
        let use_case = AddGroupMemberUseCase::new(
            groups.clone(),
            groups.clone(),
            Arc::new(FakeUsers::new(vec![owner.clone(), target.clone()])),
        );

        use_case
            .execute(leaf.id, owner.id, "alice", CollaboratorRole::Reader)
            .await
            .unwrap();

        assert!(groups.has_member(leaf.id, target.id, CollaboratorRole::Reader));
    }

    #[tokio::test]
    async fn a_caller_below_maintainer_anywhere_in_the_chain_is_rejected() {
        let groups = Arc::new(FakeGroups::empty());
        let owner = user("owner");
        let contributor = user("contributor");
        let target = user("alice");
        let group = groups
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: owner.id,
            })
            .await
            .unwrap();
        groups
            .add_member(group.id, owner.id, CollaboratorRole::Maintainer)
            .await
            .unwrap();
        groups
            .add_member(group.id, contributor.id, CollaboratorRole::Contributor)
            .await
            .unwrap();
        let use_case = AddGroupMemberUseCase::new(
            groups.clone(),
            groups.clone(),
            Arc::new(FakeUsers::new(vec![
                owner.clone(),
                contributor.clone(),
                target.clone(),
            ])),
        );

        let result = use_case
            .execute(group.id, contributor.id, "alice", CollaboratorRole::Reader)
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn a_caller_with_no_role_anywhere_in_the_chain_is_rejected() {
        let groups = Arc::new(FakeGroups::empty());
        let owner = user("owner");
        let stranger = user("stranger");
        let target = user("alice");
        let group = groups
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: owner.id,
            })
            .await
            .unwrap();
        groups
            .add_member(group.id, owner.id, CollaboratorRole::Maintainer)
            .await
            .unwrap();
        let use_case = AddGroupMemberUseCase::new(
            groups.clone(),
            groups.clone(),
            Arc::new(FakeUsers::new(vec![
                owner.clone(),
                stranger.clone(),
                target.clone(),
            ])),
        );

        let result = use_case
            .execute(group.id, stranger.id, "alice", CollaboratorRole::Reader)
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn adding_an_unknown_username_is_a_validation_error() {
        let groups = Arc::new(FakeGroups::empty());
        let owner = user("owner");
        let group = groups
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: owner.id,
            })
            .await
            .unwrap();
        groups
            .add_member(group.id, owner.id, CollaboratorRole::Maintainer)
            .await
            .unwrap();
        let use_case = AddGroupMemberUseCase::new(
            groups.clone(),
            groups.clone(),
            Arc::new(FakeUsers::new(vec![owner.clone()])),
        );

        let result = use_case
            .execute(group.id, owner.id, "nobody", CollaboratorRole::Reader)
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }
}
