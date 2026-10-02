use crate::error::{conflict_on_duplicate, infra};
use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::{Group, GroupMember, GroupStorePort, GroupWithPath, NewGroup};
use ferrisgit_domain::group_membership::GroupMembershipPort;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresGroupStore {
    pool: PgPool,
}

impl PostgresGroupStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct Row {
    id: Uuid,
    parent_group_id: Option<Uuid>,
    name: String,
    description: String,
    created_by: Option<Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<Row> for Group {
    fn from(row: Row) -> Self {
        Group {
            id: row.id,
            parent_group_id: row.parent_group_id,
            name: row.name,
            description: row.description,
            created_by: row.created_by,
            created_at: row.created_at,
        }
    }
}

#[async_trait]
impl GroupStorePort for PostgresGroupStore {
    async fn create(&self, new_group: NewGroup) -> Result<Group, DomainError> {
        let row = sqlx::query_as!(
            Row,
            "INSERT INTO groups (parent_group_id, name, description, created_by) VALUES ($1, $2, $3, $4) \
             RETURNING id, parent_group_id, name, description, created_by, created_at",
            new_group.parent_group_id,
            new_group.name,
            new_group.description,
            new_group.created_by,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(conflict_on_duplicate(|| {
            format!("a group named '{}' already exists here", new_group.name)
        }))?;
        Ok(Group::from(row))
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Group>, DomainError> {
        let row = sqlx::query_as!(Row, "SELECT id, parent_group_id, name, description, created_by, created_at FROM groups WHERE id = $1", id)
            .fetch_optional(&self.pool)
            .await
            .map_err(infra)?;
        Ok(row.map(Group::from))
    }

    async fn find_child_by_name(
        &self,
        parent_id: Option<Uuid>,
        name: &str,
    ) -> Result<Option<Group>, DomainError> {
        let row = sqlx::query_as!(
            Row,
            "SELECT id, parent_group_id, name, description, created_by, created_at FROM groups \
             WHERE parent_group_id IS NOT DISTINCT FROM $1 AND name = $2",
            parent_id,
            name
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;
        Ok(row.map(Group::from))
    }

    async fn list_children(&self, parent_id: Option<Uuid>) -> Result<Vec<Group>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT id, parent_group_id, name, description, created_by, created_at FROM groups \
             WHERE parent_group_id IS NOT DISTINCT FROM $1 ORDER BY name",
            parent_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows.into_iter().map(Group::from).collect())
    }

    async fn ancestor_chain(&self, group_id: Uuid) -> Result<Vec<Group>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            r#"
            WITH RECURSIVE chain AS (
                SELECT id, parent_group_id, name, description, created_by, created_at, 0 AS depth
                FROM groups WHERE id = $1
                UNION ALL
                SELECT g.id, g.parent_group_id, g.name, g.description, g.created_by, g.created_at, c.depth + 1
                FROM groups g JOIN chain c ON g.id = c.parent_group_id
            )
            SELECT id AS "id!", parent_group_id, name AS "name!", description AS "description!",
                   created_by AS "created_by?", created_at AS "created_at!" FROM chain ORDER BY depth DESC
            "#,
            group_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows.into_iter().map(Group::from).collect())
    }

    async fn list_writable_groups(&self, user_id: Uuid) -> Result<Vec<GroupWithPath>, DomainError> {
        let maintainer_roots = sqlx::query_as!(
            Row,
            "SELECT g.id, g.parent_group_id, g.name, g.description, g.created_by, g.created_at FROM groups g \
             JOIN group_members gm ON gm.group_id = g.id WHERE gm.user_id = $1 AND gm.role = 'maintainer'",
            user_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;

        struct DescendantRow {
            id: Uuid,
            parent_group_id: Option<Uuid>,
            name: String,
            description: String,
            created_by: Option<Uuid>,
            created_at: chrono::DateTime<chrono::Utc>,
            suffix: Option<String>,
        }

        let mut result = Vec::new();
        for root in maintainer_roots {
            let root = Group::from(root);
            let ancestors = self.ancestor_chain(root.id).await?;
            let base_path = ancestors
                .iter()
                .map(|g| g.name.as_str())
                .collect::<Vec<_>>()
                .join("/");
            result.push(GroupWithPath {
                group: root.clone(),
                path: base_path.clone(),
            });

            let descendants = sqlx::query_as!(
                DescendantRow,
                r#"
                WITH RECURSIVE d AS (
                    SELECT id, parent_group_id, name, description, created_by, created_at, name::text AS suffix
                    FROM groups WHERE parent_group_id = $1
                    UNION ALL
                    SELECT g.id, g.parent_group_id, g.name, g.description, g.created_by, g.created_at, d.suffix || '/' || g.name
                    FROM groups g JOIN d ON g.parent_group_id = d.id
                )
                SELECT id AS "id!", parent_group_id, name AS "name!", description AS "description!",
                       created_by AS "created_by?", created_at AS "created_at!", suffix FROM d
                "#,
                root.id
            )
            .fetch_all(&self.pool)
            .await
            .map_err(infra)?;

            for d in descendants {
                let group = Group {
                    id: d.id,
                    parent_group_id: d.parent_group_id,
                    name: d.name,
                    description: d.description,
                    created_by: d.created_by,
                    created_at: d.created_at,
                };
                let path = format!("{base_path}/{}", d.suffix.unwrap_or_default());
                result.push(GroupWithPath { group, path });
            }
        }

        // A user who is Maintainer at several levels of one chain pushes the same (id, path) pair more
        // than once. Deduplicating by id is enough.
        let mut seen = std::collections::HashSet::new();
        result.retain(|g| seen.insert(g.group.id));
        Ok(result)
    }

    async fn list_member_group_ids(&self, user_id: Uuid) -> Result<Vec<Uuid>, DomainError> {
        let direct_roots = sqlx::query_scalar!(
            "SELECT group_id FROM group_members WHERE user_id = $1",
            user_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;

        let mut result: Vec<Uuid> = Vec::new();
        for root_id in direct_roots {
            result.push(root_id);

            let descendant_ids = sqlx::query_scalar!(
                r#"
                WITH RECURSIVE d AS (
                    SELECT id FROM groups WHERE parent_group_id = $1
                    UNION ALL
                    SELECT g.id FROM groups g JOIN d ON g.parent_group_id = d.id
                )
                SELECT id AS "id!" FROM d
                "#,
                root_id
            )
            .fetch_all(&self.pool)
            .await
            .map_err(infra)?;

            result.extend(descendant_ids);
        }

        let mut seen = std::collections::HashSet::new();
        result.retain(|id| seen.insert(*id));
        Ok(result)
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        sqlx::query!("DELETE FROM groups WHERE id = $1", id)
            .execute(&self.pool)
            .await
            .map_err(infra)?;
        Ok(())
    }

    async fn delete_if_empty(&self, id: Uuid) -> Result<bool, DomainError> {
        let result = sqlx::query!(
            r#"
            DELETE FROM groups g
            WHERE g.id = $1
              AND NOT EXISTS (SELECT 1 FROM groups c WHERE c.parent_group_id = g.id)
              AND NOT EXISTS (SELECT 1 FROM repositories r WHERE r.group_id = g.id)
            "#,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(result.rows_affected() == 1)
    }
}

#[async_trait]
impl GroupMembershipPort for PostgresGroupStore {
    async fn add_member(
        &self,
        group_id: Uuid,
        user_id: Uuid,
        role: CollaboratorRole,
    ) -> Result<(), DomainError> {
        sqlx::query!(
            "INSERT INTO group_members (group_id, user_id, role) VALUES ($1, $2, $3)",
            group_id,
            user_id,
            role.as_str()
        )
        .execute(&self.pool)
        .await
        .map_err(conflict_on_duplicate(|| {
            "already a member of this group".to_string()
        }))?;
        Ok(())
    }

    async fn set_member_role(
        &self,
        group_id: Uuid,
        user_id: Uuid,
        role: CollaboratorRole,
    ) -> Result<(), DomainError> {
        let result = sqlx::query!(
            "UPDATE group_members SET role = $1 WHERE group_id = $2 AND user_id = $3",
            role.as_str(),
            group_id,
            user_id
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound("group member".to_string()));
        }
        Ok(())
    }

    async fn remove_member(&self, group_id: Uuid, user_id: Uuid) -> Result<(), DomainError> {
        sqlx::query!(
            "DELETE FROM group_members WHERE group_id = $1 AND user_id = $2",
            group_id,
            user_id
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }

    async fn list_members(&self, group_id: Uuid) -> Result<Vec<GroupMember>, DomainError> {
        struct MemberRow {
            group_id: Uuid,
            user_id: Uuid,
            username: String,
            role: String,
            created_at: chrono::DateTime<chrono::Utc>,
        }
        let rows = sqlx::query_as!(
            MemberRow,
            "SELECT gm.group_id, gm.user_id, u.username, gm.role, gm.created_at FROM group_members gm \
             JOIN users u ON u.id = gm.user_id WHERE gm.group_id = $1 ORDER BY u.username",
            group_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        rows.into_iter()
            .map(|r| {
                Ok(GroupMember {
                    group_id: r.group_id,
                    user_id: r.user_id,
                    username: r.username,
                    role: CollaboratorRole::parse(&r.role)?,
                    created_at: r.created_at,
                })
            })
            .collect()
    }

    async fn get_member_role(
        &self,
        group_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<CollaboratorRole>, DomainError> {
        let role: Option<String> = sqlx::query_scalar!(
            "SELECT role FROM group_members WHERE group_id = $1 AND user_id = $2",
            group_id,
            user_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;
        role.map(|r| CollaboratorRole::parse(&r)).transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::test_support::seed_user;
    use ferrisgit_domain::group_membership::GroupMembershipPort;

    #[sqlx::test(migrations = "../../migrations")]
    async fn ancestor_chain_is_root_first_at_three_levels(pool: PgPool) {
        let store = PostgresGroupStore::new(pool.clone());
        let creator = seed_user(&pool, "florian").await;
        let root = store
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();
        let mid = store
            .create(NewGroup {
                parent_group_id: Some(root.id),
                name: "backend".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();
        let leaf = store
            .create(NewGroup {
                parent_group_id: Some(mid.id),
                name: "infra".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();

        let chain = store.ancestor_chain(leaf.id).await.unwrap();

        assert_eq!(
            chain.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(),
            vec!["acme", "backend", "infra"]
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn root_group_names_are_unique(pool: PgPool) {
        let store = PostgresGroupStore::new(pool.clone());
        let creator = seed_user(&pool, "florian").await;
        store
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();

        let result = store
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn subgroup_names_are_unique_only_among_siblings(pool: PgPool) {
        let store = PostgresGroupStore::new(pool.clone());
        let creator = seed_user(&pool, "florian").await;
        let root_a = store
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();
        let root_b = store
            .create(NewGroup {
                parent_group_id: None,
                name: "other".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();
        store
            .create(NewGroup {
                parent_group_id: Some(root_a.id),
                name: "backend".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();

        let result = store
            .create(NewGroup {
                parent_group_id: Some(root_b.id),
                name: "backend".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await;

        assert!(result.is_ok());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn member_role_crud(pool: PgPool) {
        let store = PostgresGroupStore::new(pool.clone());
        let creator = seed_user(&pool, "florian").await;
        let member = seed_user(&pool, "alice").await;
        let group = store
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();

        assert_eq!(store.get_member_role(group.id, member).await.unwrap(), None);
        store
            .add_member(group.id, member, CollaboratorRole::Contributor)
            .await
            .unwrap();
        assert_eq!(
            store.get_member_role(group.id, member).await.unwrap(),
            Some(CollaboratorRole::Contributor)
        );

        store
            .set_member_role(group.id, member, CollaboratorRole::Maintainer)
            .await
            .unwrap();
        assert_eq!(
            store.get_member_role(group.id, member).await.unwrap(),
            Some(CollaboratorRole::Maintainer)
        );

        store.remove_member(group.id, member).await.unwrap();
        assert_eq!(store.get_member_role(group.id, member).await.unwrap(), None);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_writable_groups_includes_direct_maintainer_and_all_descendants(pool: PgPool) {
        let store = PostgresGroupStore::new(pool.clone());
        let creator = seed_user(&pool, "florian").await;
        let user = seed_user(&pool, "alice").await;
        let root = store
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();
        let mid = store
            .create(NewGroup {
                parent_group_id: Some(root.id),
                name: "backend".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();
        let leaf = store
            .create(NewGroup {
                parent_group_id: Some(mid.id),
                name: "infra".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();
        let unrelated = store
            .create(NewGroup {
                parent_group_id: None,
                name: "other".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();
        store
            .add_member(mid.id, user, CollaboratorRole::Maintainer)
            .await
            .unwrap();

        let writable = store.list_writable_groups(user).await.unwrap();

        let paths: Vec<String> = writable.iter().map(|w| w.path.clone()).collect();
        assert_eq!(
            paths.len(),
            2,
            "expected mid + its one descendant, got {paths:?}"
        );
        assert!(paths.contains(&"acme/backend".to_string()));
        assert!(paths.contains(&"acme/backend/infra".to_string()));
        assert!(!writable.iter().any(|w| w.group.id == root.id));
        assert!(!writable.iter().any(|w| w.group.id == unrelated.id));
        let _ = leaf;
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_writable_groups_does_not_duplicate_a_group_reachable_through_two_maintainer_grants(
        pool: PgPool,
    ) {
        let store = PostgresGroupStore::new(pool.clone());
        let creator = seed_user(&pool, "florian").await;
        let user = seed_user(&pool, "alice").await;
        let root = store
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();
        let mid = store
            .create(NewGroup {
                parent_group_id: Some(root.id),
                name: "backend".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();
        let leaf = store
            .create(NewGroup {
                parent_group_id: Some(mid.id),
                name: "infra".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();
        store
            .add_member(root.id, user, CollaboratorRole::Maintainer)
            .await
            .unwrap();
        store
            .add_member(mid.id, user, CollaboratorRole::Maintainer)
            .await
            .unwrap();

        let writable = store.list_writable_groups(user).await.unwrap();

        let ids: Vec<Uuid> = writable.iter().map(|w| w.group.id).collect();
        assert_eq!(
            ids.len(),
            3,
            "expected root + mid + leaf exactly once each, got {ids:?}"
        );
        assert!(ids.contains(&root.id));
        assert!(ids.contains(&mid.id));
        assert!(ids.contains(&leaf.id));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_member_group_ids_includes_a_reader_roles_group_and_all_descendants(pool: PgPool) {
        let store = PostgresGroupStore::new(pool.clone());
        let creator = seed_user(&pool, "florian").await;
        let user = seed_user(&pool, "alice").await;
        let root = store
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();
        let mid = store
            .create(NewGroup {
                parent_group_id: Some(root.id),
                name: "backend".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();
        let leaf = store
            .create(NewGroup {
                parent_group_id: Some(mid.id),
                name: "infra".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();
        let unrelated = store
            .create(NewGroup {
                parent_group_id: None,
                name: "other".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();
        // Reader on purpose: any role counts here, unlike in `list_writable_groups`
        store
            .add_member(mid.id, user, CollaboratorRole::Reader)
            .await
            .unwrap();

        let ids = store.list_member_group_ids(user).await.unwrap();

        assert_eq!(
            ids.len(),
            2,
            "expected mid + its one descendant, got {ids:?}"
        );
        assert!(ids.contains(&mid.id));
        assert!(ids.contains(&leaf.id));
        assert!(
            !ids.contains(&root.id),
            "inheritance flows downward only — the ancestor must not be included"
        );
        assert!(!ids.contains(&unrelated.id));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_member_group_ids_does_not_duplicate_a_group_reachable_through_two_direct_memberships(
        pool: PgPool,
    ) {
        let store = PostgresGroupStore::new(pool.clone());
        let creator = seed_user(&pool, "florian").await;
        let user = seed_user(&pool, "alice").await;
        let root = store
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();
        let mid = store
            .create(NewGroup {
                parent_group_id: Some(root.id),
                name: "backend".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();
        let leaf = store
            .create(NewGroup {
                parent_group_id: Some(mid.id),
                name: "infra".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();
        store
            .add_member(root.id, user, CollaboratorRole::Reader)
            .await
            .unwrap();
        store
            .add_member(mid.id, user, CollaboratorRole::Contributor)
            .await
            .unwrap();

        let ids = store.list_member_group_ids(user).await.unwrap();

        assert_eq!(
            ids.len(),
            3,
            "expected root + mid + leaf exactly once each, got {ids:?}"
        );
        assert!(ids.contains(&root.id));
        assert!(ids.contains(&mid.id));
        assert!(ids.contains(&leaf.id));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn set_member_role_on_a_missing_pair_is_not_found(pool: PgPool) {
        let store = PostgresGroupStore::new(pool.clone());
        let creator = seed_user(&pool, "florian").await;
        let non_member = seed_user(&pool, "alice").await;
        let group = store
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();

        let result = store
            .set_member_role(group.id, non_member, CollaboratorRole::Maintainer)
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn deleting_a_group_removes_it(pool: PgPool) {
        let creator = seed_user(&pool, "florian").await;
        let store = PostgresGroupStore::new(pool);
        let group = store
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();

        store.delete(group.id).await.unwrap();

        assert!(store.find_by_id(group.id).await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn delete_if_empty_deletes_a_genuinely_empty_group_and_reports_true(pool: PgPool) {
        let creator = seed_user(&pool, "florian").await;
        let store = PostgresGroupStore::new(pool);
        let group = store
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();

        let deleted = store.delete_if_empty(group.id).await.unwrap();

        assert!(
            deleted,
            "an empty group must be deleted and reported as such"
        );
        assert!(store.find_by_id(group.id).await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn delete_if_empty_refuses_a_group_with_a_child_group(pool: PgPool) {
        let creator = seed_user(&pool, "florian").await;
        let store = PostgresGroupStore::new(pool);
        let root = store
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();
        store
            .create(NewGroup {
                parent_group_id: Some(root.id),
                name: "backend".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();

        let deleted = store.delete_if_empty(root.id).await.unwrap();

        assert!(!deleted, "a group with a child group must not be deleted");
        assert!(
            store.find_by_id(root.id).await.unwrap().is_some(),
            "the group must still exist"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn delete_if_empty_refuses_a_group_with_a_repository(pool: PgPool) {
        let creator = seed_user(&pool, "florian").await;
        let store = PostgresGroupStore::new(pool.clone());
        let group = store
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: creator,
            })
            .await
            .unwrap();

        sqlx::query!(
            "INSERT INTO repositories (owner_id, name, group_id, description, disk_path, visibility) VALUES ($1, $2, $3, '', 'acme/widget.git', 'private')",
            creator,
            "widget",
            group.id
        )
        .execute(&pool)
        .await
        .unwrap();

        let deleted = store.delete_if_empty(group.id).await.unwrap();

        assert!(!deleted, "a group with a repository must not be deleted");
        assert!(
            store.find_by_id(group.id).await.unwrap().is_some(),
            "the group must still exist"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn delete_if_empty_on_a_missing_group_returns_false(pool: PgPool) {
        let store = PostgresGroupStore::new(pool);

        let deleted = store.delete_if_empty(Uuid::new_v4()).await.unwrap();

        assert!(
            !deleted,
            "deleting a group that doesn't exist must report false, not error"
        );
    }
}
