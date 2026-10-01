use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::{Group, GroupStorePort, NewGroup};
use ferrisgit_domain::group_membership::GroupMembershipPort;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use ferrisgit_domain::user::UserRepositoryPort;
use uuid::Uuid;

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
        if name.trim().is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
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
                let chain = self.groups.ancestor_chain(parent_id).await?;
                let mut best: Option<CollaboratorRole> = None;
                for group in &chain {
                    if let Some(role) = self
                        .group_membership
                        .get_member_role(group.id, caller_id)
                        .await?
                    {
                        best = Some(best.map_or(role, |b| b.max(role)));
                    }
                }
                if best.is_none_or(|r| r < CollaboratorRole::Maintainer) {
                    return Err(DomainError::NotFound("group".to_string()));
                }
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
    use chrono::Utc;
    use ferrisgit_domain::group_membership::GroupMembershipPort;
    use ferrisgit_domain::user::User;

    #[tokio::test]
    async fn any_user_can_create_a_root_group_and_becomes_its_maintainer() {
        let groups = Arc::new(FakeGroups::empty());
        let use_case = CreateGroupUseCase::new(
            groups.clone(),
            groups.clone(),
            Arc::new(FakeUsers::new(vec![])),
        );
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
        let users = Arc::new(FakeUsers::new(vec![User {
            id: Uuid::new_v4(),
            username: "acme".to_string(),
            email: "a@a.com".to_string(),
            password_hash: "h".to_string(),
            is_admin: false,
            created_at: Utc::now(),
        }]));
        let use_case = CreateGroupUseCase::new(groups.clone(), groups, users);

        let result = use_case
            .execute(Uuid::new_v4(), None, "acme".to_string(), String::new())
            .await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
    }

    #[tokio::test]
    async fn creating_a_subgroup_requires_maintainer_on_the_parent() {
        let groups = Arc::new(FakeGroups::empty());
        let owner = Uuid::new_v4();
        let root = groups
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: owner,
            })
            .await
            .unwrap();
        groups
            .add_member(root.id, owner, CollaboratorRole::Maintainer)
            .await
            .unwrap();
        let use_case =
            CreateGroupUseCase::new(groups.clone(), groups, Arc::new(FakeUsers::new(vec![])));
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
        let groups = Arc::new(FakeGroups::empty());
        let owner = Uuid::new_v4();
        let root = groups
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: owner,
            })
            .await
            .unwrap();
        groups
            .add_member(root.id, owner, CollaboratorRole::Maintainer)
            .await
            .unwrap();
        let mid = groups
            .create(NewGroup {
                parent_group_id: Some(root.id),
                name: "backend".to_string(),
                description: String::new(),
                created_by: owner,
            })
            .await
            .unwrap();
        let use_case =
            CreateGroupUseCase::new(groups.clone(), groups, Arc::new(FakeUsers::new(vec![])));

        let created = use_case
            .execute(owner, Some(mid.id), "infra".to_string(), String::new())
            .await
            .unwrap();

        assert_eq!(created.parent_group_id, Some(mid.id));
    }
}
