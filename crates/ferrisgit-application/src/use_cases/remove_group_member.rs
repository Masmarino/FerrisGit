use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::group_membership::GroupMembershipPort;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use ferrisgit_domain::user::UserRepositoryPort;
use uuid::Uuid;

use super::group_maintainer_guard::would_leave_chain_without_a_maintainer;

pub struct RemoveGroupMemberUseCase {
    groups: Arc<dyn GroupStorePort>,
    group_membership: Arc<dyn GroupMembershipPort>,
    users: Arc<dyn UserRepositoryPort>,
}

impl RemoveGroupMemberUseCase {
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

        let target_role = self
            .group_membership
            .get_member_role(group_id, target.id)
            .await?;
        if target_role == Some(CollaboratorRole::Maintainer)
            && would_leave_chain_without_a_maintainer(
                self.group_membership.as_ref(),
                &chain,
                group_id,
                target.id,
            )
            .await?
        {
            return Err(DomainError::Validation(
                "cannot remove the last maintainer of this group hierarchy".to_string(),
            ));
        }

        self.group_membership
            .remove_member(group_id, target.id)
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
    async fn a_direct_maintainer_can_remove_a_member() {
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
        groups
            .add_member(group.id, target.id, CollaboratorRole::Reader)
            .await
            .unwrap();
        let use_case = RemoveGroupMemberUseCase::new(
            groups.clone(),
            groups.clone(),
            Arc::new(FakeUsers::new(vec![owner.clone(), target.clone()])),
        );

        use_case.execute(group.id, owner.id, "alice").await.unwrap();

        assert!(!groups.is_member(group.id, target.id));
    }

    #[tokio::test]
    async fn a_maintainer_inherited_from_an_ancestor_two_levels_up_can_remove_a_member() {
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
        groups
            .add_member(leaf.id, target.id, CollaboratorRole::Reader)
            .await
            .unwrap();
        let use_case = RemoveGroupMemberUseCase::new(
            groups.clone(),
            groups.clone(),
            Arc::new(FakeUsers::new(vec![owner.clone(), target.clone()])),
        );

        use_case.execute(leaf.id, owner.id, "alice").await.unwrap();

        assert!(!groups.is_member(leaf.id, target.id));
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
        groups
            .add_member(group.id, target.id, CollaboratorRole::Reader)
            .await
            .unwrap();
        let use_case = RemoveGroupMemberUseCase::new(
            groups.clone(),
            groups.clone(),
            Arc::new(FakeUsers::new(vec![
                owner.clone(),
                contributor.clone(),
                target.clone(),
            ])),
        );

        let result = use_case.execute(group.id, contributor.id, "alice").await;

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
        groups
            .add_member(group.id, target.id, CollaboratorRole::Reader)
            .await
            .unwrap();
        let use_case = RemoveGroupMemberUseCase::new(
            groups.clone(),
            groups.clone(),
            Arc::new(FakeUsers::new(vec![
                owner.clone(),
                stranger.clone(),
                target.clone(),
            ])),
        );

        let result = use_case.execute(group.id, stranger.id, "alice").await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn removing_a_maintainer_when_another_maintainer_remains_on_the_same_group_succeeds() {
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
        groups
            .add_member(group.id, target.id, CollaboratorRole::Maintainer)
            .await
            .unwrap();
        let use_case = RemoveGroupMemberUseCase::new(
            groups.clone(),
            groups.clone(),
            Arc::new(FakeUsers::new(vec![owner.clone(), target.clone()])),
        );

        use_case.execute(group.id, owner.id, "alice").await.unwrap();

        assert!(!groups.is_member(group.id, target.id));
    }

    #[tokio::test]
    async fn removing_the_last_maintainer_of_a_root_group_is_rejected() {
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
        let use_case = RemoveGroupMemberUseCase::new(
            groups.clone(),
            groups.clone(),
            Arc::new(FakeUsers::new(vec![owner.clone()])),
        );

        let result = use_case.execute(group.id, owner.id, "owner").await;

        assert!(
            matches!(result, Err(DomainError::Validation(_))),
            "expected Validation, got {result:?}"
        );
        assert!(
            groups.is_member(group.id, owner.id),
            "the last maintainer must not actually be removed"
        );
    }

    /// `infra` has no direct Maintainer left but its ancestor `acme` has one, so removing infra's last direct
    /// Maintainer succeeds.
    #[tokio::test]
    async fn removing_a_subgroups_last_direct_maintainer_succeeds_when_an_ancestor_still_has_one() {
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
        let leaf = groups
            .create(NewGroup {
                parent_group_id: Some(root.id),
                name: "infra".to_string(),
                description: String::new(),
                created_by: owner.id,
            })
            .await
            .unwrap();
        groups
            .add_member(leaf.id, target.id, CollaboratorRole::Maintainer)
            .await
            .unwrap();
        let use_case = RemoveGroupMemberUseCase::new(
            groups.clone(),
            groups.clone(),
            Arc::new(FakeUsers::new(vec![owner.clone(), target.clone()])),
        );

        use_case.execute(leaf.id, owner.id, "alice").await.unwrap();

        assert!(!groups.is_member(leaf.id, target.id));
    }

    /// Neither `infra` nor `acme` has another Maintainer, so the guard has to walk the whole chain.
    #[tokio::test]
    async fn removing_a_subgroups_last_maintainer_is_rejected_when_no_ancestor_has_one_either() {
        let groups = Arc::new(FakeGroups::empty());
        let owner = user("owner");
        let root = groups
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: owner.id,
            })
            .await
            .unwrap();
        let leaf = groups
            .create(NewGroup {
                parent_group_id: Some(root.id),
                name: "infra".to_string(),
                description: String::new(),
                created_by: owner.id,
            })
            .await
            .unwrap();
        groups
            .add_member(leaf.id, owner.id, CollaboratorRole::Maintainer)
            .await
            .unwrap();
        let use_case = RemoveGroupMemberUseCase::new(
            groups.clone(),
            groups.clone(),
            Arc::new(FakeUsers::new(vec![owner.clone()])),
        );

        let result = use_case.execute(leaf.id, owner.id, "owner").await;

        assert!(
            matches!(result, Err(DomainError::Validation(_))),
            "expected Validation, got {result:?}"
        );
    }

    #[tokio::test]
    async fn removing_an_unknown_username_is_a_validation_error() {
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
        let use_case = RemoveGroupMemberUseCase::new(
            groups.clone(),
            groups.clone(),
            Arc::new(FakeUsers::new(vec![owner.clone()])),
        );

        let result = use_case.execute(group.id, owner.id, "nobody").await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }
}
