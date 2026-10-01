use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::repository::{
    NewRepository, Repository, RepositoryStorePort, RepositoryVisibility,
};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresRepositoryStore {
    pool: PgPool,
}

impl PostgresRepositoryStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct Row {
    id: Uuid,
    owner_id: Uuid,
    name: String,
    group_id: Option<Uuid>,
    description: String,
    disk_path: String,
    visibility: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl Row {
    fn into_domain(self) -> Result<Repository, DomainError> {
        Ok(Repository {
            id: self.id,
            owner_id: self.owner_id,
            name: self.name,
            group_id: self.group_id,
            description: self.description,
            disk_path: self.disk_path,
            visibility: RepositoryVisibility::parse(&self.visibility)?,
            created_at: self.created_at,
        })
    }
}

#[async_trait]
impl RepositoryStorePort for PostgresRepositoryStore {
    async fn create(
        &self,
        new_repo: NewRepository,
        disk_path: String,
    ) -> Result<Repository, DomainError> {
        let row = sqlx::query_as!(
            Row,
            "INSERT INTO repositories (owner_id, name, group_id, description, disk_path, visibility) VALUES ($1, $2, $3, $4, $5, $6) \
             RETURNING id, owner_id, name, group_id, description, disk_path, visibility, created_at",
            new_repo.owner_id,
            new_repo.name,
            new_repo.group_id,
            new_repo.description,
            disk_path,
            new_repo.visibility.as_str(),
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db_err) = &e
                && db_err.code().as_deref() == Some("23505")
            {
                return match new_repo.group_id {
                    Some(_) => DomainError::Conflict(format!("a repository named '{}' already exists in this group", new_repo.name)),
                    None => DomainError::Conflict(format!("a repository named '{}' already exists", new_repo.name)),
                };
            }
            DomainError::Infrastructure(e.to_string())
        })?;
        row.into_domain()
    }

    async fn list_for_owner(&self, owner_id: Uuid) -> Result<Vec<Repository>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT id, owner_id, name, group_id, description, disk_path, visibility, created_at FROM repositories WHERE owner_id = $1 AND group_id IS NULL ORDER BY created_at DESC",
            owner_id
        )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        rows.into_iter().map(Row::into_domain).collect()
    }

    async fn find_by_owner_and_name(
        &self,
        owner_id: Uuid,
        name: &str,
    ) -> Result<Option<Repository>, DomainError> {
        let row = sqlx::query_as!(
            Row,
            "SELECT id, owner_id, name, group_id, description, disk_path, visibility, created_at FROM repositories WHERE owner_id = $1 AND name = $2 AND group_id IS NULL",
            owner_id,
            name
        )
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        row.map(Row::into_domain).transpose()
    }

    async fn find_by_group_and_name(
        &self,
        group_id: Uuid,
        name: &str,
    ) -> Result<Option<Repository>, DomainError> {
        let row = sqlx::query_as!(
            Row,
            "SELECT id, owner_id, name, description, group_id, disk_path, visibility, created_at FROM repositories WHERE group_id = $1 AND name = $2",
            group_id,
            name
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(row.map(Row::into_domain).transpose()?)
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Repository>, DomainError> {
        let row = sqlx::query_as!(Row, "SELECT id, owner_id, name, group_id, description, disk_path, visibility, created_at FROM repositories WHERE id = $1", id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        row.map(Row::into_domain).transpose()
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        sqlx::query!("DELETE FROM repositories WHERE id = $1", id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(())
    }

    async fn list_for_group(&self, group_id: Uuid) -> Result<Vec<Repository>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT id, owner_id, name, description, group_id, disk_path, visibility, created_at FROM repositories WHERE group_id = $1 ORDER BY name",
            group_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        rows.into_iter().map(Row::into_domain).collect()
    }

    async fn list_all(&self) -> Result<Vec<Repository>, DomainError> {
        let rows = sqlx::query_as!(Row, "SELECT id, owner_id, name, group_id, description, disk_path, visibility, created_at FROM repositories")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        rows.into_iter().map(Row::into_domain).collect()
    }

    async fn list_public(&self) -> Result<Vec<Uuid>, DomainError> {
        sqlx::query_scalar!("SELECT id FROM repositories WHERE visibility = 'public'")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))
    }

    /// `websearch_to_tsquery` handles quoted phrases, `OR` and `-term` exclusion. A query that is only a
    /// negation matches most rows, capped by `LIMIT`. The `'simple'` config (no stemming or stopwords) is
    /// there so identifiers match exactly. `, id` breaks ties: ranks can tie (names are only unique per
    /// owner or group), and `ORDER BY` alone returns tied rows in no set order, so the results and the
    /// `LIMIT` cut would be unstable.
    async fn search(
        &self,
        ids: &[Uuid],
        query: &str,
        limit: i64,
    ) -> Result<Vec<Repository>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT id, owner_id, name, group_id, description, disk_path, visibility, created_at FROM repositories \
             WHERE id = ANY($1::uuid[]) AND search_vector @@ websearch_to_tsquery('simple', $2) \
             ORDER BY ts_rank_cd(search_vector, websearch_to_tsquery('simple', $2)) DESC, id \
             LIMIT $3",
            ids,
            query,
            limit,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        rows.into_iter().map(Row::into_domain).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::user_repository::PostgresUserRepository;
    use ferrisgit_domain::user::{NewUser, UserRepositoryPort};

    async fn seed_user(pool: &PgPool, username: &str) -> Uuid {
        let users = PostgresUserRepository::new(pool.clone());
        users
            .create(NewUser {
                username: username.to_string(),
                email: format!("{username}@example.com"),
                password_hash: "h".to_string(),
                is_admin: false,
            })
            .await
            .unwrap()
            .id
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn creating_then_finding_a_repository_by_owner_and_name_returns_it(pool: PgPool) {
        let owner_id = seed_user(&pool, "florian").await;
        let store = PostgresRepositoryStore::new(pool);
        store
            .create(
                NewRepository {
                    owner_id,
                    name: "hello".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                format!("{owner_id}/hello.git"),
            )
            .await
            .unwrap();

        let found = store
            .find_by_owner_and_name(owner_id, "hello")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.name, "hello");
        assert_eq!(found.visibility, RepositoryVisibility::Private);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_unique_constraint_violation_maps_to_conflict_not_infrastructure(pool: PgPool) {
        let owner_id = seed_user(&pool, "florian").await;
        let store = PostgresRepositoryStore::new(pool);
        store
            .create(
                NewRepository {
                    owner_id,
                    name: "hello".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                "path-1".to_string(),
            )
            .await
            .unwrap();

        let result = store
            .create(
                NewRepository {
                    owner_id,
                    name: "hello".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                "path-2".to_string(),
            )
            .await;

        assert!(
            matches!(result, Err(DomainError::Conflict(_))),
            "expected Conflict, got {result:?}"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_unique_constraint_violation_in_a_group_maps_to_a_group_scoped_conflict_message(
        pool: PgPool,
    ) {
        use crate::postgres::group_store::PostgresGroupStore;
        use ferrisgit_domain::group::{GroupStorePort, NewGroup};

        let owner_id = seed_user(&pool, "florian").await;
        let groups = PostgresGroupStore::new(pool.clone());
        let group = groups
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: owner_id,
            })
            .await
            .unwrap();

        let store = PostgresRepositoryStore::new(pool);
        store
            .create(
                NewRepository {
                    owner_id,
                    name: "hello".to_string(),
                    group_id: Some(group.id),
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                "path-1".to_string(),
            )
            .await
            .unwrap();

        let result = store
            .create(
                NewRepository {
                    owner_id,
                    name: "hello".to_string(),
                    group_id: Some(group.id),
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                "path-2".to_string(),
            )
            .await;

        match result {
            Err(DomainError::Conflict(message)) => {
                assert_eq!(
                    message,
                    "a repository named 'hello' already exists in this group"
                );
            }
            other => panic!("expected a group-scoped Conflict, got {other:?}"),
        }
    }

    /// Regression: `UNIQUE (owner_id, name)` cross-collided personal and group namespaces, because group
    /// repositories still carry `owner_id = <creator>`.
    #[sqlx::test(migrations = "../../migrations")]
    async fn a_group_repository_may_share_a_name_with_the_creators_personal_repository(
        pool: PgPool,
    ) {
        use crate::postgres::group_store::PostgresGroupStore;
        use ferrisgit_domain::group::{GroupStorePort, NewGroup};

        let owner_id = seed_user(&pool, "florian").await;
        let groups = PostgresGroupStore::new(pool.clone());
        let group = groups
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: owner_id,
            })
            .await
            .unwrap();

        let store = PostgresRepositoryStore::new(pool);
        store
            .create(
                NewRepository {
                    owner_id,
                    name: "api".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                "personal-path".to_string(),
            )
            .await
            .unwrap();

        let group_repo = store
            .create(
                NewRepository {
                    owner_id,
                    name: "api".to_string(),
                    group_id: Some(group.id),
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                "group-path".to_string(),
            )
            .await
            .unwrap();

        assert_eq!(group_repo.name, "api");
        assert_eq!(group_repo.group_id, Some(group.id));

        let other_group = groups
            .create(NewGroup {
                parent_group_id: None,
                name: "widgets".to_string(),
                description: String::new(),
                created_by: owner_id,
            })
            .await
            .unwrap();
        let other_group_repo = store
            .create(
                NewRepository {
                    owner_id,
                    name: "api".to_string(),
                    group_id: Some(other_group.id),
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                "other-group-path".to_string(),
            )
            .await
            .unwrap();
        assert_eq!(other_group_repo.group_id, Some(other_group.id));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_for_owner_returns_only_that_owners_repositories(pool: PgPool) {
        let owner_id = seed_user(&pool, "florian").await;
        let other_owner_id = seed_user(&pool, "other").await;
        let store = PostgresRepositoryStore::new(pool);
        store
            .create(
                NewRepository {
                    owner_id,
                    name: "mine".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                "p1".to_string(),
            )
            .await
            .unwrap();
        store
            .create(
                NewRepository {
                    owner_id: other_owner_id,
                    name: "theirs".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                "p2".to_string(),
            )
            .await
            .unwrap();

        let repos = store.list_for_owner(owner_id).await.unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].name, "mine");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn find_by_id_returns_the_repository(pool: PgPool) {
        let owner_id = seed_user(&pool, "florian").await;
        let store = PostgresRepositoryStore::new(pool);
        let created = store
            .create(
                NewRepository {
                    owner_id,
                    name: "hello".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                "path".to_string(),
            )
            .await
            .unwrap();

        let found = store.find_by_id(created.id).await.unwrap().unwrap();

        assert_eq!(found.name, "hello");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn finding_a_repository_by_group_and_name_returns_it(pool: PgPool) {
        use crate::postgres::group_store::PostgresGroupStore;
        use ferrisgit_domain::group::{GroupStorePort, NewGroup};

        let owner_id = seed_user(&pool, "florian").await;
        let groups = PostgresGroupStore::new(pool.clone());
        let group = groups
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: owner_id,
            })
            .await
            .unwrap();

        let store = PostgresRepositoryStore::new(pool);
        store
            .create(
                NewRepository {
                    owner_id,
                    name: "hello".to_string(),
                    group_id: Some(group.id),
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                format!("{owner_id}/hello.git"),
            )
            .await
            .unwrap();

        let found = store
            .find_by_group_and_name(group.id, "hello")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.name, "hello");
        assert_eq!(found.group_id, Some(group.id));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_for_group_returns_only_that_groups_repositories(pool: PgPool) {
        use crate::postgres::group_store::PostgresGroupStore;
        use ferrisgit_domain::group::{GroupStorePort, NewGroup};

        let owner_id = seed_user(&pool, "florian").await;
        let groups = PostgresGroupStore::new(pool.clone());
        let group = groups
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: owner_id,
            })
            .await
            .unwrap();
        let other_group = groups
            .create(NewGroup {
                parent_group_id: None,
                name: "other".to_string(),
                description: String::new(),
                created_by: owner_id,
            })
            .await
            .unwrap();

        let store = PostgresRepositoryStore::new(pool);
        store
            .create(
                NewRepository {
                    owner_id,
                    name: "in-group".to_string(),
                    group_id: Some(group.id),
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                format!("{owner_id}/in-group.git"),
            )
            .await
            .unwrap();
        store
            .create(
                NewRepository {
                    owner_id,
                    name: "elsewhere".to_string(),
                    group_id: Some(other_group.id),
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                format!("{owner_id}/elsewhere.git"),
            )
            .await
            .unwrap();
        store
            .create(
                NewRepository {
                    owner_id,
                    name: "ungrouped".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                format!("{owner_id}/ungrouped.git"),
            )
            .await
            .unwrap();

        let repos = store.list_for_group(group.id).await.unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].name, "in-group");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn search_ranks_a_name_match_above_a_description_only_match(pool: PgPool) {
        let owner_id = seed_user(&pool, "florian").await;
        let store = PostgresRepositoryStore::new(pool);
        let name_match = store
            .create(
                NewRepository {
                    owner_id,
                    name: "widget parser tool".to_string(),
                    group_id: None,
                    description: "does nothing special".to_string(),
                    visibility: RepositoryVisibility::Private,
                },
                "a.git".to_string(),
            )
            .await
            .unwrap();
        let description_match = store
            .create(
                NewRepository {
                    owner_id,
                    name: "unrelated".to_string(),
                    group_id: None,
                    description: "a widget parser tool for legacy data".to_string(),
                    visibility: RepositoryVisibility::Private,
                },
                "b.git".to_string(),
            )
            .await
            .unwrap();

        let results = store
            .search(
                &[name_match.id, description_match.id],
                "widget parser tool",
                8,
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, name_match.id);
        assert_eq!(results[1].id, description_match.id);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn search_breaks_a_tied_rank_deterministically(pool: PgPool) {
        let owner_a = seed_user(&pool, "florian").await;
        let owner_b = seed_user(&pool, "alice").await;
        let store = PostgresRepositoryStore::new(pool);
        // Two owners so the names can be identical: an exact `ts_rank_cd` tie.
        let a = store
            .create(
                NewRepository {
                    owner_id: owner_a,
                    name: "widget".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                "a.git".to_string(),
            )
            .await
            .unwrap();
        let b = store
            .create(
                NewRepository {
                    owner_id: owner_b,
                    name: "widget".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                "b.git".to_string(),
            )
            .await
            .unwrap();

        let results = store.search(&[a.id, b.id], "widget", 8).await.unwrap();

        let mut expected = vec![a.id, b.id];
        expected.sort();
        assert_eq!(
            results.iter().map(|r| r.id).collect::<Vec<_>>(),
            expected,
            "tied-rank rows must come back in a stable, deterministic order"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn search_excludes_a_negated_term_while_still_matching_the_positive_one(pool: PgPool) {
        let owner_id = seed_user(&pool, "florian").await;
        let store = PostgresRepositoryStore::new(pool);
        let wanted = store
            .create(
                NewRepository {
                    owner_id,
                    name: "widget for external use".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                "a.git".to_string(),
            )
            .await
            .unwrap();
        let excluded = store
            .create(
                NewRepository {
                    owner_id,
                    name: "widget for internal use".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                "b.git".to_string(),
            )
            .await
            .unwrap();

        let results = store
            .search(&[wanted.id, excluded.id], "widget -internal", 8)
            .await
            .unwrap();

        assert_eq!(
            results.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![wanted.id],
            "a leading '-' must exclude repositories matching that term, per websearch_to_tsquery's own syntax"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn search_never_returns_a_repository_outside_the_given_ids(pool: PgPool) {
        let owner_id = seed_user(&pool, "florian").await;
        let store = PostgresRepositoryStore::new(pool);
        let in_scope = store
            .create(
                NewRepository {
                    owner_id,
                    name: "gadget".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                "a.git".to_string(),
            )
            .await
            .unwrap();
        let out_of_scope = store
            .create(
                NewRepository {
                    owner_id,
                    name: "gadget-two".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                "b.git".to_string(),
            )
            .await
            .unwrap();

        let results = store.search(&[in_scope.id], "gadget", 8).await.unwrap();

        assert_eq!(
            results.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![in_scope.id]
        );
        assert!(!results.iter().any(|r| r.id == out_of_scope.id));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_public_returns_every_public_repository_regardless_of_owner(pool: PgPool) {
        let owner_id = seed_user(&pool, "florian").await;
        let other_owner_id = seed_user(&pool, "alice").await;
        let store = PostgresRepositoryStore::new(pool);
        let private_repo = store
            .create(
                NewRepository {
                    owner_id,
                    name: "private-one".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                "a.git".to_string(),
            )
            .await
            .unwrap();
        let public_repo = store
            .create(
                NewRepository {
                    owner_id: other_owner_id,
                    name: "public-one".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Public,
                },
                "b.git".to_string(),
            )
            .await
            .unwrap();

        let results = store.list_public().await.unwrap();

        assert_eq!(results, vec![public_repo.id]);
        assert!(
            !results.contains(&private_repo.id),
            "a private repository must never be returned by list_public"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn deleting_a_repository_removes_it(pool: PgPool) {
        let owner_id = seed_user(&pool, "florian").await;
        let store = PostgresRepositoryStore::new(pool);
        let created = store
            .create(
                NewRepository {
                    owner_id,
                    name: "hello".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                "path".to_string(),
            )
            .await
            .unwrap();

        store.delete(created.id).await.unwrap();

        assert!(store.find_by_id(created.id).await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn deleting_an_unknown_repository_id_is_not_an_error(pool: PgPool) {
        let store = PostgresRepositoryStore::new(pool);

        store.delete(Uuid::new_v4()).await.unwrap();
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_all_returns_every_repository_regardless_of_owner(pool: PgPool) {
        let owner_id = seed_user(&pool, "florian").await;
        let other_owner_id = seed_user(&pool, "alice").await;
        let store = PostgresRepositoryStore::new(pool);
        store
            .create(
                NewRepository {
                    owner_id,
                    name: "mine".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                "p1".to_string(),
            )
            .await
            .unwrap();
        store
            .create(
                NewRepository {
                    owner_id: other_owner_id,
                    name: "theirs".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Public,
                },
                "p2".to_string(),
            )
            .await
            .unwrap();

        let repos = store.list_all().await.unwrap();
        assert_eq!(repos.len(), 2);
    }
}
