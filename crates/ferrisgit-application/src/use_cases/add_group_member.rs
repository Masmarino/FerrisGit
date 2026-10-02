use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::group_membership::GroupMembershipPort;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use ferrisgit_domain::user::UserRepositoryPort;
use uuid::Uuid;

use super::require_group_maintainer::require_group_maintainer;

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
        require_group_maintainer(
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
        self.group_membership
            .add_member(group_id, target.id, role)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeGroups, FakeUsers};
    use crate::use_cases::fixtures::{group, user};
    use ferrisgit_domain::group::Group;
    use ferrisgit_domain::user::User;

    /// The use case over `groups`, with each `(group, user, role)` of `members` already granted.
    async fn fixture(
        users: Vec<User>,
        groups: Vec<Group>,
        members: &[(Uuid, Uuid, CollaboratorRole)],
    ) -> (AddGroupMemberUseCase, Arc<FakeGroups>) {
        let groups = Arc::new(FakeGroups::new(groups));
        for &(group_id, user_id, role) in members {
            groups.add_member(group_id, user_id, role).await.unwrap();
        }
        let use_case = AddGroupMemberUseCase::new(
            groups.clone(),
            groups.clone(),
            Arc::new(FakeUsers::new(users)),
        );
        (use_case, groups)
    }

    #[tokio::test]
    async fn a_direct_maintainer_can_add_a_member() {
        let owner = user("owner");
        let target = user("alice");
        let acme = group(None, "acme");
        let (use_case, groups) = fixture(
            vec![owner.clone(), target.clone()],
            vec![acme.clone()],
            &[(acme.id, owner.id, CollaboratorRole::Maintainer)],
        )
        .await;

        use_case
            .execute(acme.id, owner.id, "alice", CollaboratorRole::Contributor)
            .await
            .unwrap();

        assert!(groups.has_member(acme.id, target.id, CollaboratorRole::Contributor));
    }

    #[tokio::test]
    async fn a_maintainer_inherited_from_an_ancestor_two_levels_up_can_add_a_member() {
        let owner = user("owner");
        let target = user("alice");
        let root = group(None, "acme");
        let mid = group(Some(root.id), "backend");
        let leaf = group(Some(mid.id), "infra");
        let (use_case, groups) = fixture(
            vec![owner.clone(), target.clone()],
            vec![root.clone(), mid, leaf.clone()],
            &[(root.id, owner.id, CollaboratorRole::Maintainer)],
        )
        .await;

        use_case
            .execute(leaf.id, owner.id, "alice", CollaboratorRole::Reader)
            .await
            .unwrap();

        assert!(groups.has_member(leaf.id, target.id, CollaboratorRole::Reader));
    }

    #[tokio::test]
    async fn a_caller_below_maintainer_anywhere_in_the_chain_is_rejected() {
        let owner = user("owner");
        let contributor = user("contributor");
        let acme = group(None, "acme");
        let (use_case, _) = fixture(
            vec![owner.clone(), contributor.clone(), user("alice")],
            vec![acme.clone()],
            &[
                (acme.id, owner.id, CollaboratorRole::Maintainer),
                (acme.id, contributor.id, CollaboratorRole::Contributor),
            ],
        )
        .await;

        let result = use_case
            .execute(acme.id, contributor.id, "alice", CollaboratorRole::Reader)
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn a_caller_with_no_role_anywhere_in_the_chain_is_rejected() {
        let owner = user("owner");
        let stranger = user("stranger");
        let acme = group(None, "acme");
        let (use_case, _) = fixture(
            vec![owner.clone(), stranger.clone(), user("alice")],
            vec![acme.clone()],
            &[(acme.id, owner.id, CollaboratorRole::Maintainer)],
        )
        .await;

        let result = use_case
            .execute(acme.id, stranger.id, "alice", CollaboratorRole::Reader)
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn adding_an_unknown_username_is_a_validation_error() {
        let owner = user("owner");
        let acme = group(None, "acme");
        let (use_case, _) = fixture(
            vec![owner.clone()],
            vec![acme.clone()],
            &[(acme.id, owner.id, CollaboratorRole::Maintainer)],
        )
        .await;

        let result = use_case
            .execute(acme.id, owner.id, "nobody", CollaboratorRole::Reader)
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }
}
