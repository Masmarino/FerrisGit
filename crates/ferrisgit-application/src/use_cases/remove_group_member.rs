use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::group_membership::GroupMembershipPort;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use ferrisgit_domain::user::UserRepositoryPort;
use uuid::Uuid;

use super::group_maintainer_guard::would_leave_chain_without_a_maintainer;
use super::require_group_maintainer::require_group_maintainer;

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
        let chain = require_group_maintainer(
            self.groups.as_ref(),
            self.group_membership.as_ref(),
            group_id,
            caller_id,
        )
        .await?;

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
    use crate::use_cases::fixtures::user;
    use ferrisgit_domain::group::{Group, NewGroup};
    use ferrisgit_domain::group_membership::GroupMembershipPort;

    async fn create_group(
        groups: &FakeGroups,
        parent_group_id: Option<Uuid>,
        name: &str,
        created_by: Uuid,
    ) -> Group {
        groups
            .create(NewGroup {
                parent_group_id,
                name: name.to_string(),
                description: String::new(),
                created_by,
            })
            .await
            .unwrap()
    }

    async fn grant(groups: &FakeGroups, group_id: Uuid, user_id: Uuid, role: CollaboratorRole) {
        groups.add_member(group_id, user_id, role).await.unwrap();
    }

    #[tokio::test]
    async fn a_direct_maintainer_can_remove_a_member() {
        let groups = Arc::new(FakeGroups::empty());
        let owner = user("owner");
        let target = user("alice");
        let group = create_group(&groups, None, "acme", owner.id).await;
        grant(&groups, group.id, owner.id, CollaboratorRole::Maintainer).await;
        grant(&groups, group.id, target.id, CollaboratorRole::Reader).await;
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
        let root = create_group(&groups, None, "acme", owner.id).await;
        grant(&groups, root.id, owner.id, CollaboratorRole::Maintainer).await;
        let mid = create_group(&groups, Some(root.id), "backend", owner.id).await;
        let leaf = create_group(&groups, Some(mid.id), "infra", owner.id).await;
        grant(&groups, leaf.id, target.id, CollaboratorRole::Reader).await;
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
        let group = create_group(&groups, None, "acme", owner.id).await;
        grant(&groups, group.id, owner.id, CollaboratorRole::Maintainer).await;
        grant(
            &groups,
            group.id,
            contributor.id,
            CollaboratorRole::Contributor,
        )
        .await;
        grant(&groups, group.id, target.id, CollaboratorRole::Reader).await;
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
        let group = create_group(&groups, None, "acme", owner.id).await;
        grant(&groups, group.id, owner.id, CollaboratorRole::Maintainer).await;
        grant(&groups, group.id, target.id, CollaboratorRole::Reader).await;
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
        let group = create_group(&groups, None, "acme", owner.id).await;
        grant(&groups, group.id, owner.id, CollaboratorRole::Maintainer).await;
        grant(&groups, group.id, target.id, CollaboratorRole::Maintainer).await;
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
        let group = create_group(&groups, None, "acme", owner.id).await;
        grant(&groups, group.id, owner.id, CollaboratorRole::Maintainer).await;
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
        let root = create_group(&groups, None, "acme", owner.id).await;
        grant(&groups, root.id, owner.id, CollaboratorRole::Maintainer).await;
        let leaf = create_group(&groups, Some(root.id), "infra", owner.id).await;
        grant(&groups, leaf.id, target.id, CollaboratorRole::Maintainer).await;
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
        let root = create_group(&groups, None, "acme", owner.id).await;
        let leaf = create_group(&groups, Some(root.id), "infra", owner.id).await;
        grant(&groups, leaf.id, owner.id, CollaboratorRole::Maintainer).await;
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
        let group = create_group(&groups, None, "acme", owner.id).await;
        grant(&groups, group.id, owner.id, CollaboratorRole::Maintainer).await;
        let use_case = RemoveGroupMemberUseCase::new(
            groups.clone(),
            groups.clone(),
            Arc::new(FakeUsers::new(vec![owner.clone()])),
        );

        let result = use_case.execute(group.id, owner.id, "nobody").await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }
}
