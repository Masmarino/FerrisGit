use async_trait::async_trait;
use ferrisgit_domain::api_token::{ApiToken, ApiTokenRepositoryPort, NewApiToken};
use ferrisgit_domain::error::DomainError;
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresApiTokenRepository {
    pool: PgPool,
}

impl PostgresApiTokenRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ApiTokenRepositoryPort for PostgresApiTokenRepository {
    async fn create(&self, new_token: NewApiToken) -> Result<ApiToken, DomainError> {
        sqlx::query_as!(
            ApiToken,
            "INSERT INTO api_tokens (user_id, name, token_hash) VALUES ($1, $2, $3) RETURNING *",
            new_token.user_id,
            new_token.name,
            new_token.token_hash,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))
    }

    async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<ApiToken>, DomainError> {
        sqlx::query_as!(
            ApiToken,
            "SELECT * FROM api_tokens WHERE user_id = $1 ORDER BY created_at DESC",
            user_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))
    }

    async fn find_by_hash(&self, token_hash: &str) -> Result<Option<ApiToken>, DomainError> {
        sqlx::query_as!(
            ApiToken,
            "SELECT * FROM api_tokens WHERE token_hash = $1",
            token_hash
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))
    }

    async fn revoke(&self, id: Uuid, user_id: Uuid) -> Result<(), DomainError> {
        let result = sqlx::query!(
            "DELETE FROM api_tokens WHERE id = $1 AND user_id = $2",
            id,
            user_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound("api token".to_string()));
        }
        Ok(())
    }

    async fn touch_last_used(&self, id: Uuid) -> Result<(), DomainError> {
        sqlx::query!(
            "UPDATE api_tokens SET last_used_at = now() WHERE id = $1",
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::user_repository::PostgresUserRepository;
    use ferrisgit_domain::user::{NewUser, UserRepositoryPort};

    async fn seed_user(pool: &PgPool) -> Uuid {
        let users = PostgresUserRepository::new(pool.clone());
        users
            .create(NewUser {
                username: "florian".to_string(),
                email: "florian@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
            })
            .await
            .unwrap()
            .id
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn creating_then_finding_a_token_by_hash_returns_it(pool: PgPool) {
        let user_id = seed_user(&pool).await;
        let repo = PostgresApiTokenRepository::new(pool);
        let created = repo
            .create(NewApiToken {
                user_id,
                name: "ci".to_string(),
                token_hash: "abchash".to_string(),
            })
            .await
            .unwrap();

        let found = repo.find_by_hash("abchash").await.unwrap().unwrap();
        assert_eq!(found.id, created.id);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn revoking_a_token_removes_it_from_list_for_user(pool: PgPool) {
        let user_id = seed_user(&pool).await;
        let repo = PostgresApiTokenRepository::new(pool);
        let created = repo
            .create(NewApiToken {
                user_id,
                name: "ci".to_string(),
                token_hash: "hash".to_string(),
            })
            .await
            .unwrap();

        repo.revoke(created.id, user_id).await.unwrap();

        assert!(repo.list_for_user(user_id).await.unwrap().is_empty());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn revoking_another_users_token_fails(pool: PgPool) {
        let user_id = seed_user(&pool).await;
        let repo = PostgresApiTokenRepository::new(pool);
        let created = repo
            .create(NewApiToken {
                user_id,
                name: "ci".to_string(),
                token_hash: "hash".to_string(),
            })
            .await
            .unwrap();

        let result = repo.revoke(created.id, Uuid::new_v4()).await;
        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }
}
