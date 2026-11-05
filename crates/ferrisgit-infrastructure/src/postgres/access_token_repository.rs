use crate::error::infra;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ferrisgit_domain::access_token::{
    AccessToken, AccessTokenRepositoryPort, DeviceAuthorization, DeviceAuthorizationPort,
    DeviceAuthorizationStatus, DevicePoll, NewAccessToken, NewDeviceAuthorization, TokenScope,
};
use ferrisgit_domain::error::DomainError;
use sqlx::PgPool;
use uuid::Uuid;

/// A scope this version doesn't know, written by a newer one before a rollback, grants nothing.
fn known_scopes(stored: Vec<String>) -> Vec<TokenScope> {
    stored
        .into_iter()
        .filter_map(|value| value.parse().ok())
        .collect()
}

fn scope_strings(scopes: &[TokenScope]) -> Vec<String> {
    scopes.iter().map(ToString::to_string).collect()
}

struct AccessTokenRow {
    id: Uuid,
    user_id: Uuid,
    name: String,
    token_hash: String,
    scopes: Vec<String>,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
}

impl From<AccessTokenRow> for AccessToken {
    fn from(row: AccessTokenRow) -> Self {
        AccessToken {
            id: row.id,
            user_id: row.user_id,
            name: row.name,
            token_hash: row.token_hash,
            scopes: known_scopes(row.scopes),
            expires_at: row.expires_at,
            created_at: row.created_at,
            last_used_at: row.last_used_at,
        }
    }
}

pub struct PostgresAccessTokenRepository {
    pool: PgPool,
}

impl PostgresAccessTokenRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AccessTokenRepositoryPort for PostgresAccessTokenRepository {
    async fn create(&self, new_token: NewAccessToken) -> Result<AccessToken, DomainError> {
        sqlx::query_as!(
            AccessTokenRow,
            "INSERT INTO access_tokens (user_id, name, token_hash, scopes, expires_at)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id, user_id, name, token_hash, scopes, expires_at, created_at, last_used_at",
            new_token.user_id,
            new_token.name,
            new_token.token_hash,
            &scope_strings(&new_token.scopes),
            new_token.expires_at,
        )
        .fetch_one(&self.pool)
        .await
        .map(Into::into)
        .map_err(infra)
    }

    async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<AccessToken>, DomainError> {
        let rows = sqlx::query_as!(
            AccessTokenRow,
            "SELECT id, user_id, name, token_hash, scopes, expires_at, created_at, last_used_at
             FROM access_tokens WHERE user_id = $1 ORDER BY created_at DESC",
            user_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn find_by_hash(&self, token_hash: &str) -> Result<Option<AccessToken>, DomainError> {
        let row = sqlx::query_as!(
            AccessTokenRow,
            "SELECT id, user_id, name, token_hash, scopes, expires_at, created_at, last_used_at
             FROM access_tokens WHERE token_hash = $1",
            token_hash
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;
        Ok(row.map(Into::into))
    }

    async fn revoke(&self, id: Uuid, user_id: Uuid) -> Result<(), DomainError> {
        let result = sqlx::query!(
            "DELETE FROM access_tokens WHERE id = $1 AND user_id = $2",
            id,
            user_id
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound("access token".to_string()));
        }
        Ok(())
    }

    async fn touch_last_used(&self, id: Uuid) -> Result<(), DomainError> {
        sqlx::query!(
            "UPDATE access_tokens SET last_used_at = now()
             WHERE id = $1 AND (last_used_at IS NULL OR last_used_at < now() - interval '1 minute')",
            id
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }
}

struct DeviceRow {
    id: Uuid,
    user_code: String,
    client_name: String,
    scopes: Vec<String>,
    token_lifetime_days: i32,
    status: String,
    user_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl TryFrom<DeviceRow> for DeviceAuthorization {
    type Error = DomainError;

    fn try_from(row: DeviceRow) -> Result<Self, DomainError> {
        Ok(DeviceAuthorization {
            id: row.id,
            user_code: row.user_code,
            client_name: row.client_name,
            scopes: known_scopes(row.scopes),
            token_lifetime_days: row.token_lifetime_days,
            status: DeviceAuthorizationStatus::parse(&row.status)?,
            user_id: row.user_id,
            created_at: row.created_at,
            expires_at: row.expires_at,
        })
    }
}

pub struct PostgresDeviceAuthorizationStore {
    pool: PgPool,
}

impl PostgresDeviceAuthorizationStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DeviceAuthorizationPort for PostgresDeviceAuthorizationStore {
    async fn create(&self, new_request: NewDeviceAuthorization) -> Result<(), DomainError> {
        sqlx::query!(
            "INSERT INTO device_authorizations
                 (device_code_hash, user_code, client_name, scopes, token_lifetime_days, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6)",
            new_request.device_code_hash,
            new_request.user_code,
            new_request.client_name,
            &scope_strings(&new_request.scopes),
            new_request.token_lifetime_days,
            new_request.expires_at,
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }

    async fn find_pending_by_user_code(
        &self,
        user_code: &str,
    ) -> Result<Option<DeviceAuthorization>, DomainError> {
        let row = sqlx::query_as!(
            DeviceRow,
            "SELECT id, user_code, client_name, scopes, token_lifetime_days, status, user_id,
                    created_at, expires_at
             FROM device_authorizations
             WHERE user_code = $1 AND status = 'pending' AND expires_at > now()",
            user_code
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;
        row.map(TryInto::try_into).transpose()
    }

    async fn approve(&self, user_code: &str, user_id: Uuid) -> Result<bool, DomainError> {
        let result = sqlx::query!(
            "UPDATE device_authorizations SET status = 'approved', user_id = $2
             WHERE user_code = $1 AND status = 'pending' AND expires_at > now()",
            user_code,
            user_id
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(result.rows_affected() == 1)
    }

    async fn deny(&self, user_code: &str) -> Result<bool, DomainError> {
        let result = sqlx::query!(
            "UPDATE device_authorizations SET status = 'denied'
             WHERE user_code = $1 AND status = 'pending' AND expires_at > now()",
            user_code
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(result.rows_affected() == 1)
    }

    /// The approved case deletes the row in the same statement that reads it, so two polls can't both collect it.
    async fn poll(
        &self,
        device_code_hash: &str,
        min_interval_seconds: i32,
    ) -> Result<DevicePoll, DomainError> {
        let collected = sqlx::query_as!(
            DeviceRow,
            "DELETE FROM device_authorizations
             WHERE device_code_hash = $1 AND status = 'approved' AND expires_at > now()
             RETURNING id, user_code, client_name, scopes, token_lifetime_days, status, user_id,
                       created_at, expires_at",
            device_code_hash
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;
        if let Some(row) = collected {
            return Ok(DevicePoll::Approved(row.try_into()?));
        }

        // `too_fast` compares with the previous poll: the RETURNING of an UPDATE can't see the old row, so a CTE reads it.
        let polled = sqlx::query!(
            r#"WITH previous AS (
                   SELECT id, status, last_polled_at FROM device_authorizations
                   WHERE device_code_hash = $1 AND expires_at > now()
                   FOR UPDATE
               ), updated AS (
                   UPDATE device_authorizations AS d SET last_polled_at = now()
                   FROM previous WHERE d.id = previous.id
               )
               SELECT status AS "status!",
                      COALESCE(last_polled_at > now() - make_interval(secs => $2::int), false) AS "too_fast!"
               FROM previous"#,
            device_code_hash,
            min_interval_seconds
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;

        let Some(polled) = polled else {
            return Ok(DevicePoll::Expired);
        };
        Ok(match DeviceAuthorizationStatus::parse(&polled.status)? {
            DeviceAuthorizationStatus::Denied => DevicePoll::Denied,
            // Approved a moment after the DELETE above looked: the next poll collects it.
            DeviceAuthorizationStatus::Approved => DevicePoll::Pending,
            DeviceAuthorizationStatus::Pending if polled.too_fast => DevicePoll::TooFast,
            DeviceAuthorizationStatus::Pending => DevicePoll::Pending,
        })
    }

    async fn delete_expired(&self) -> Result<(), DomainError> {
        sqlx::query!("DELETE FROM device_authorizations WHERE expires_at <= now()")
            .execute(&self.pool)
            .await
            .map_err(infra)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::user_repository::PostgresUserRepository;
    use ferrisgit_domain::user::{NewUser, UserRepositoryPort};

    async fn seed_user(pool: &PgPool) -> Uuid {
        PostgresUserRepository::new(pool.clone())
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

    fn scope(value: &str) -> TokenScope {
        value.parse().unwrap()
    }

    fn new_token(user_id: Uuid, hash: &str) -> NewAccessToken {
        NewAccessToken {
            user_id,
            name: "ci".to_string(),
            token_hash: hash.to_string(),
            scopes: vec![scope("issues:read"), scope("merge-requests:write")],
            expires_at: Utc::now() + chrono::Duration::days(30),
        }
    }

    fn new_request(hash: &str, code: &str, ttl: chrono::Duration) -> NewDeviceAuthorization {
        NewDeviceAuthorization {
            device_code_hash: hash.to_string(),
            user_code: code.to_string(),
            client_name: "fgit".to_string(),
            scopes: vec![scope("issues:read")],
            token_lifetime_days: 90,
            expires_at: Utc::now() + ttl,
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_token_finds_back_with_its_scopes(pool: PgPool) {
        let user_id = seed_user(&pool).await;
        let repo = PostgresAccessTokenRepository::new(pool);
        repo.create(new_token(user_id, "hash")).await.unwrap();

        let found = repo.find_by_hash("hash").await.unwrap().unwrap();

        assert_eq!(found.user_id, user_id);
        assert_eq!(
            found.scopes,
            vec![scope("issues:read"), scope("merge-requests:write")]
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_stored_scope_this_version_does_not_know_grants_nothing(pool: PgPool) {
        let user_id = seed_user(&pool).await;
        let repo = PostgresAccessTokenRepository::new(pool.clone());
        repo.create(new_token(user_id, "hash")).await.unwrap();
        sqlx::query("UPDATE access_tokens SET scopes = ARRAY['future:write']")
            .execute(&pool)
            .await
            .unwrap();

        assert!(
            repo.find_by_hash("hash")
                .await
                .unwrap()
                .unwrap()
                .scopes
                .is_empty()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn revoking_another_users_token_fails_and_leaves_it(pool: PgPool) {
        let user_id = seed_user(&pool).await;
        let repo = PostgresAccessTokenRepository::new(pool);
        let token = repo.create(new_token(user_id, "hash")).await.unwrap();

        let result = repo.revoke(token.id, Uuid::new_v4()).await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
        assert_eq!(repo.list_for_user(user_id).await.unwrap().len(), 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn an_approved_request_is_collected_exactly_once(pool: PgPool) {
        let user_id = seed_user(&pool).await;
        let store = PostgresDeviceAuthorizationStore::new(pool);
        store
            .create(new_request(
                "dc",
                "ABCD-EFGH",
                chrono::Duration::minutes(15),
            ))
            .await
            .unwrap();
        assert!(store.approve("ABCD-EFGH", user_id).await.unwrap());

        let first = store.poll("dc", 0).await.unwrap();
        let second = store.poll("dc", 0).await.unwrap();

        let DevicePoll::Approved(collected) = first else {
            panic!("expected the approved request, got {first:?}");
        };
        assert_eq!(collected.user_id, Some(user_id));
        assert!(matches!(second, DevicePoll::Expired));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn polling_again_too_soon_is_told_to_slow_down(pool: PgPool) {
        let store = PostgresDeviceAuthorizationStore::new(pool);
        store
            .create(new_request(
                "dc",
                "ABCD-EFGH",
                chrono::Duration::minutes(15),
            ))
            .await
            .unwrap();

        assert!(matches!(
            store.poll("dc", 60).await.unwrap(),
            DevicePoll::Pending
        ));
        assert!(matches!(
            store.poll("dc", 60).await.unwrap(),
            DevicePoll::TooFast
        ));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_denied_request_can_not_be_approved_afterwards(pool: PgPool) {
        let user_id = seed_user(&pool).await;
        let store = PostgresDeviceAuthorizationStore::new(pool);
        store
            .create(new_request(
                "dc",
                "ABCD-EFGH",
                chrono::Duration::minutes(15),
            ))
            .await
            .unwrap();

        assert!(store.deny("ABCD-EFGH").await.unwrap());

        assert!(!store.approve("ABCD-EFGH", user_id).await.unwrap());
        assert!(matches!(
            store.poll("dc", 0).await.unwrap(),
            DevicePoll::Denied
        ));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn an_expired_request_can_neither_be_approved_nor_collected(pool: PgPool) {
        let user_id = seed_user(&pool).await;
        let store = PostgresDeviceAuthorizationStore::new(pool);
        store
            .create(new_request(
                "dc",
                "ABCD-EFGH",
                chrono::Duration::seconds(-1),
            ))
            .await
            .unwrap();

        assert!(
            store
                .find_pending_by_user_code("ABCD-EFGH")
                .await
                .unwrap()
                .is_none()
        );
        assert!(!store.approve("ABCD-EFGH", user_id).await.unwrap());
        assert!(matches!(
            store.poll("dc", 0).await.unwrap(),
            DevicePoll::Expired
        ));
    }
}
