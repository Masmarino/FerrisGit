use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::repository_star::RepositoryStarStorePort;
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresRepositoryStarStore {
    pool: PgPool,
}

impl PostgresRepositoryStarStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RepositoryStarStorePort for PostgresRepositoryStarStore {
    async fn add(&self, repository_id: Uuid, user_id: Uuid) -> Result<(), DomainError> {
        sqlx::query!(
            "INSERT INTO repository_stars (repository_id, user_id) VALUES ($1, $2) ON CONFLICT (repository_id, user_id) DO NOTHING",
            repository_id,
            user_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(())
    }

    async fn remove(&self, repository_id: Uuid, user_id: Uuid) -> Result<(), DomainError> {
        sqlx::query!(
            "DELETE FROM repository_stars WHERE repository_id = $1 AND user_id = $2",
            repository_id,
            user_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(())
    }

    async fn count_for_repository(&self, repository_id: Uuid) -> Result<i64, DomainError> {
        let row = sqlx::query!(
            "SELECT COUNT(*) as count FROM repository_stars WHERE repository_id = $1",
            repository_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(row.count.unwrap_or(0))
    }

    async fn is_starred(&self, repository_id: Uuid, user_id: Uuid) -> Result<bool, DomainError> {
        sqlx::query_scalar!("SELECT EXISTS(SELECT 1 FROM repository_stars WHERE repository_id = $1 AND user_id = $2)", repository_id, user_id)
            .fetch_one(&self.pool)
            .await
            .map(|exists| exists.unwrap_or(false))
            .map_err(|e| DomainError::Infrastructure(e.to_string()))
    }

    async fn list_starred_for_user(&self, user_id: Uuid) -> Result<Vec<Uuid>, DomainError> {
        let rows = sqlx::query!(
            "SELECT repository_id FROM repository_stars WHERE user_id = $1",
            user_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(rows.into_iter().map(|r| r.repository_id).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrisgit_domain::repository::{NewRepository, RepositoryStorePort, RepositoryVisibility};
    use ferrisgit_domain::user::{NewUser, UserRepositoryPort};

    async fn seed_user(pool: &PgPool, username: &str) -> Uuid {
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
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

    async fn seed_repository(pool: &PgPool, owner_id: Uuid, name: &str) -> Uuid {
        let repos = crate::postgres::repository_store::PostgresRepositoryStore::new(pool.clone());
        repos
            .create(
                NewRepository {
                    owner_id,
                    name: name.to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                format!("{name}-path"),
            )
            .await
            .unwrap()
            .id
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn starring_then_unstarring_updates_count_and_is_starred(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let repo_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresRepositoryStarStore::new(pool);

        assert_eq!(store.count_for_repository(repo_id).await.unwrap(), 0);
        assert!(!store.is_starred(repo_id, owner_id).await.unwrap());

        store.add(repo_id, owner_id).await.unwrap();
        assert_eq!(store.count_for_repository(repo_id).await.unwrap(), 1);
        assert!(store.is_starred(repo_id, owner_id).await.unwrap());

        store.remove(repo_id, owner_id).await.unwrap();
        assert_eq!(store.count_for_repository(repo_id).await.unwrap(), 0);
        assert!(!store.is_starred(repo_id, owner_id).await.unwrap());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn starring_twice_does_not_error_and_counts_once(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let repo_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresRepositoryStarStore::new(pool);

        store.add(repo_id, owner_id).await.unwrap();
        store.add(repo_id, owner_id).await.unwrap();

        assert_eq!(store.count_for_repository(repo_id).await.unwrap(), 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn count_is_scoped_to_the_repository(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let other_user_id = seed_user(&pool, "other").await;
        let repo_id = seed_repository(&pool, owner_id, "hello").await;
        let other_repo_id = seed_repository(&pool, owner_id, "other-repo").await;
        let store = PostgresRepositoryStarStore::new(pool);

        store.add(repo_id, owner_id).await.unwrap();
        store.add(repo_id, other_user_id).await.unwrap();
        store.add(other_repo_id, owner_id).await.unwrap();

        assert_eq!(store.count_for_repository(repo_id).await.unwrap(), 2);
        assert_eq!(store.count_for_repository(other_repo_id).await.unwrap(), 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_starred_for_user_returns_only_that_users_stars(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let other_user_id = seed_user(&pool, "other").await;
        let repo_a = seed_repository(&pool, owner_id, "a").await;
        let repo_b = seed_repository(&pool, owner_id, "b").await;
        let store = PostgresRepositoryStarStore::new(pool);

        store.add(repo_a, owner_id).await.unwrap();
        store.add(repo_b, other_user_id).await.unwrap();

        let starred = store.list_starred_for_user(owner_id).await.unwrap();
        assert_eq!(starred, vec![repo_a]);
    }
}
