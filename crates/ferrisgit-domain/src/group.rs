use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::DomainError;
use crate::repository_collaborator::CollaboratorRole;

#[derive(Debug, Clone)]
pub struct Group {
    pub id: Uuid,
    pub parent_group_id: Option<Uuid>,
    pub name: String,
    pub description: String,
    /// `None` once the creator's account was deleted. Informational only, never an access grant.
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

pub struct NewGroup {
    pub parent_group_id: Option<Uuid>,
    pub name: String,
    pub description: String,
    pub created_by: Uuid,
}

#[derive(Debug, Clone)]
pub struct GroupMember {
    pub group_id: Uuid,
    pub user_id: Uuid,
    pub username: String,
    pub role: CollaboratorRole,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct GroupWithPath {
    pub group: Group,
    pub path: String,
}

#[async_trait]
pub trait GroupStorePort: Send + Sync {
    async fn create(&self, new_group: NewGroup) -> Result<Group, DomainError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Group>, DomainError>;
    /// `parent_id: None` looks among root groups.
    async fn find_child_by_name(
        &self,
        parent_id: Option<Uuid>,
        name: &str,
    ) -> Result<Option<Group>, DomainError>;
    async fn list_children(&self, parent_id: Option<Uuid>) -> Result<Vec<Group>, DomainError>;
    /// Root-first; the last element is `group_id` itself.
    async fn ancestor_chain(&self, group_id: Uuid) -> Result<Vec<Group>, DomainError>;

    /// Groups where they hold direct Maintainer membership, plus every descendant of such a group.
    async fn list_writable_groups(&self, user_id: Uuid) -> Result<Vec<GroupWithPath>, DomainError>;
    /// Groups where they hold direct membership at any role, plus every descendant (permission inherits downward).
    /// Used to discover group repositories for `GET /repositories`.
    async fn list_member_group_ids(&self, user_id: Uuid) -> Result<Vec<Uuid>, DomainError>;
    /// Cascades to the group's `group_members` rows, the only thing referencing it once the caller has confirmed (see
    /// `DeleteGroupUseCase`) it has no child group or repository. Defaulted to `unimplemented!` like
    /// `RepositoryStorePort::delete`.
    async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
        unimplemented!("delete")
    }
    /// Deletes the group only if it is still empty at the moment of the delete, in one atomic step. Otherwise a
    /// repository or subgroup created after `DeleteGroupUseCase`'s checks would be silently cascade-deleted. `false` if
    /// the group doesn't exist or wasn't empty. Defaulted to `unimplemented!` like `delete`.
    async fn delete_if_empty(&self, _id: Uuid) -> Result<bool, DomainError> {
        unimplemented!("delete_if_empty")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::group_membership::GroupMembershipPort;
    use std::sync::Mutex;

    struct FakeGroups {
        groups: Mutex<Vec<Group>>,
        members: Mutex<Vec<(Uuid, Uuid, CollaboratorRole)>>,
    }

    #[async_trait]
    impl GroupStorePort for FakeGroups {
        async fn create(&self, new_group: NewGroup) -> Result<Group, DomainError> {
            let group = Group {
                id: Uuid::new_v4(),
                parent_group_id: new_group.parent_group_id,
                name: new_group.name,
                description: new_group.description,
                created_by: Some(new_group.created_by),
                created_at: Utc::now(),
            };
            self.groups.lock().unwrap().push(group.clone());
            Ok(group)
        }
        async fn find_by_id(&self, id: Uuid) -> Result<Option<Group>, DomainError> {
            Ok(self
                .groups
                .lock()
                .unwrap()
                .iter()
                .find(|g| g.id == id)
                .cloned())
        }
        async fn find_child_by_name(
            &self,
            parent_id: Option<Uuid>,
            name: &str,
        ) -> Result<Option<Group>, DomainError> {
            Ok(self
                .groups
                .lock()
                .unwrap()
                .iter()
                .find(|g| g.parent_group_id == parent_id && g.name == name)
                .cloned())
        }
        async fn list_children(&self, parent_id: Option<Uuid>) -> Result<Vec<Group>, DomainError> {
            Ok(self
                .groups
                .lock()
                .unwrap()
                .iter()
                .filter(|g| g.parent_group_id == parent_id)
                .cloned()
                .collect())
        }
        async fn ancestor_chain(&self, group_id: Uuid) -> Result<Vec<Group>, DomainError> {
            let groups = self.groups.lock().unwrap();
            let mut chain = vec![];
            let mut current = groups.iter().find(|g| g.id == group_id).cloned();
            while let Some(g) = current {
                let parent = g.parent_group_id;
                chain.push(g);
                current = parent.and_then(|pid| groups.iter().find(|g| g.id == pid).cloned());
            }
            chain.reverse();
            Ok(chain)
        }
        async fn list_writable_groups(
            &self,
            _user_id: Uuid,
        ) -> Result<Vec<GroupWithPath>, DomainError> {
            unimplemented!("not exercised by this domain-level test")
        }
        async fn list_member_group_ids(&self, _user_id: Uuid) -> Result<Vec<Uuid>, DomainError> {
            unimplemented!("not exercised by this domain-level test")
        }
    }

    #[async_trait]
    impl GroupMembershipPort for FakeGroups {
        async fn add_member(
            &self,
            group_id: Uuid,
            user_id: Uuid,
            role: CollaboratorRole,
        ) -> Result<(), DomainError> {
            self.members.lock().unwrap().push((group_id, user_id, role));
            Ok(())
        }
        async fn set_member_role(
            &self,
            group_id: Uuid,
            user_id: Uuid,
            role: CollaboratorRole,
        ) -> Result<(), DomainError> {
            let mut members = self.members.lock().unwrap();
            let Some(m) = members
                .iter_mut()
                .find(|(g, u, _)| *g == group_id && *u == user_id)
            else {
                return Err(DomainError::NotFound("group member".to_string()));
            };
            m.2 = role;
            Ok(())
        }
        async fn remove_member(&self, group_id: Uuid, user_id: Uuid) -> Result<(), DomainError> {
            self.members
                .lock()
                .unwrap()
                .retain(|(g, u, _)| !(*g == group_id && *u == user_id));
            Ok(())
        }
        async fn list_members(&self, group_id: Uuid) -> Result<Vec<GroupMember>, DomainError> {
            Ok(self
                .members
                .lock()
                .unwrap()
                .iter()
                .filter(|(g, _, _)| *g == group_id)
                .map(|(g, u, r)| GroupMember {
                    group_id: *g,
                    user_id: *u,
                    username: "someone".to_string(),
                    role: *r,
                    created_at: Utc::now(),
                })
                .collect())
        }
        async fn get_member_role(
            &self,
            group_id: Uuid,
            user_id: Uuid,
        ) -> Result<Option<CollaboratorRole>, DomainError> {
            Ok(self
                .members
                .lock()
                .unwrap()
                .iter()
                .find(|(g, u, _)| *g == group_id && *u == user_id)
                .map(|(_, _, r)| *r))
        }
    }

    #[tokio::test]
    async fn ancestor_chain_is_root_first_and_includes_the_group_itself() {
        let store = FakeGroups {
            groups: Mutex::new(vec![]),
            members: Mutex::new(vec![]),
        };
        let root = store
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: Uuid::new_v4(),
            })
            .await
            .unwrap();
        let mid = store
            .create(NewGroup {
                parent_group_id: Some(root.id),
                name: "backend".to_string(),
                description: String::new(),
                created_by: Uuid::new_v4(),
            })
            .await
            .unwrap();
        let leaf = store
            .create(NewGroup {
                parent_group_id: Some(mid.id),
                name: "infra".to_string(),
                description: String::new(),
                created_by: Uuid::new_v4(),
            })
            .await
            .unwrap();

        let chain = store.ancestor_chain(leaf.id).await.unwrap();

        assert_eq!(
            chain.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(),
            vec!["acme", "backend", "infra"]
        );
    }

    #[tokio::test]
    async fn find_child_by_name_distinguishes_root_from_nested() {
        let store = FakeGroups {
            groups: Mutex::new(vec![]),
            members: Mutex::new(vec![]),
        };
        let root = store
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: Uuid::new_v4(),
            })
            .await
            .unwrap();
        store
            .create(NewGroup {
                parent_group_id: Some(root.id),
                name: "acme".to_string(),
                description: String::new(),
                created_by: Uuid::new_v4(),
            })
            .await
            .unwrap();

        assert_eq!(
            store
                .find_child_by_name(None, "acme")
                .await
                .unwrap()
                .unwrap()
                .id,
            root.id
        );
        assert_ne!(
            store
                .find_child_by_name(Some(root.id), "acme")
                .await
                .unwrap()
                .unwrap()
                .id,
            root.id
        );
    }
}
