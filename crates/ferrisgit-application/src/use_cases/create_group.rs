use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::{Group, GroupStorePort, NewGroup};
use ferrisgit_domain::group_membership::GroupMembershipPort;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use ferrisgit_domain::user::UserRepositoryPort;
use uuid::Uuid;

use super::name_rules::is_valid_path_name;
use super::require_group_maintainer::require_group_maintainer;

pub struct CreateGroupUseCase {
    groups: Arc<dyn GroupStorePort>,
    group_membership: Arc<dyn GroupMembershipPort>,
    users: Arc<dyn UserRepositoryPort>,
}

impl CreateGroupUseCase {
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
        caller_id: Uuid,
        parent_group_id: Option<Uuid>,
        name: String,
        description: String,
    ) -> Result<Group, DomainError> {
        if !is_valid_path_name(&name) {
            return Err(DomainError::Validation(
                "group name must be non-empty and alphanumeric/-/_ only".to_string(),
            ));
        }

        match parent_group_id {
            None => {
                if self.users.find_by_username(&name).await?.is_some() {
                    return Err(DomainError::Conflict(format!(
                        "'{name}' is already taken by a user account"
                    )));
                }
            }
            Some(parent_id) => {
                require_group_maintainer(
                    self.groups.as_ref(),
                    self.group_membership.as_ref(),
                    parent_id,
                    caller_id,
                )
                .await?;
            }
        }

        if self
            .groups
            .find_child_by_name(parent_group_id, &name)
            .await?
            .is_some()
        {
            return Err(DomainError::Conflict(format!(
                "a group named '{name}' already exists here"
            )));
        }

        let group = self
            .groups
            .create(NewGroup {
                parent_group_id,
                name,
                description,
                created_by: caller_id,
            })
            .await?;
        self.group_membership
            .add_member(group.id, caller_id, CollaboratorRole::Maintainer)
            .await?;
        Ok(group)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeGroups, FakeUsers};
    use crate::use_cases::fixtures::{group, user};
    use ferrisgit_domain::group_membership::GroupMembershipPort;
    use ferrisgit_domain::user::User;

    fn use_case(users: Vec<User>, groups: &Arc<FakeGroups>) -> CreateGroupUseCase {
        CreateGroupUseCase::new(
            groups.clone(),
            groups.clone(),
            Arc::new(FakeUsers::new(users)),
        )
    }

    #[tokio::test]
    async fn any_user_can_create_a_root_group_and_becomes_its_maintainer() {
        let groups = Arc::new(FakeGroups::empty());
        let use_case = use_case(vec![], &groups);
        let caller = Uuid::new_v4();

        let created = use_case
            .execute(caller, None, "acme".to_string(), "desc".to_string())
            .await
            .unwrap();

        assert!(groups.has_member(created.id, caller, CollaboratorRole::Maintainer));
    }

    #[tokio::test]
    async fn a_root_group_name_colliding_with_a_username_is_a_conflict() {
        let groups = Arc::new(FakeGroups::empty());
        let use_case = use_case(vec![user("acme")], &groups);

        let result = use_case
            .execute(Uuid::new_v4(), None, "acme".to_string(), String::new())
            .await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
    }

    #[tokio::test]
    async fn creating_a_subgroup_requires_maintainer_on_the_parent() {
        let owner = Uuid::new_v4();
        let root = group(None, "acme");
        let groups = Arc::new(FakeGroups::new(vec![root.clone()]));
        groups
            .add_member(root.id, owner, CollaboratorRole::Maintainer)
            .await
            .unwrap();
        let use_case = use_case(vec![], &groups);
        let stranger = Uuid::new_v4();

        let result = use_case
            .execute(
                stranger,
                Some(root.id),
                "backend".to_string(),
                String::new(),
            )
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn a_maintainer_of_an_ancestor_can_create_a_subgroup_two_levels_down() {
        let owner = Uuid::new_v4();
        let root = group(None, "acme");
        let mid = group(Some(root.id), "backend");
        let groups = Arc::new(FakeGroups::new(vec![root.clone(), mid.clone()]));
        groups
            .add_member(root.id, owner, CollaboratorRole::Maintainer)
            .await
            .unwrap();
        let use_case = use_case(vec![], &groups);

        let created = use_case
            .execute(owner, Some(mid.id), "infra".to_string(), String::new())
            .await
            .unwrap();

        assert_eq!(created.parent_group_id, Some(mid.id));
    }
}
