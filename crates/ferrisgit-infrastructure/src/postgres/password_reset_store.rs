use crate::error::infra;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::password_reset::{PasswordReset, PasswordResetPort};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresPasswordResetStore {
    pool: PgPool,
}

impl PostgresPasswordResetStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct PasswordResetRow {
    user_id: Uuid,
    expires_at: DateTime<Utc>,
}

#[async_trait]
impl PasswordResetPort for PostgresPasswordResetStore {
    async fn replace(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        // One statement, so two concurrent admin resets for a user cannot leave two live links (`user_id` is UNIQUE).
        sqlx::query(
            "INSERT INTO password_reset_tokens (user_id, token_hash, expires_at) VALUES ($1, $2, $3) \
             ON CONFLICT (user_id) DO UPDATE SET token_hash = EXCLUDED.token_hash, expires_at = EXCLUDED.expires_at, created_at = now()",
        )
        .bind(user_id)
        .bind(token_hash)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }

    async fn consume(&self, token_hash: &str) -> Result<Option<PasswordReset>, DomainError> {
        // Delete and return in one statement, so only one of several concurrent consumers gets the row. An
        // expired link never matches. It stays until the next admin reset replaces it, or the user is deleted.
        let row = sqlx::query_as::<_, PasswordResetRow>("DELETE FROM password_reset_tokens WHERE token_hash = $1 AND expires_at > now() RETURNING user_id, expires_at")
            .bind(token_hash)
            .fetch_optional(&self.pool)
            .await
            .map_err(infra)?;
        Ok(row.map(|row| PasswordReset {
            user_id: row.user_id,
            expires_at: row.expires_at,
        }))
    }

    async fn restore(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<bool, DomainError> {
        // One statement: a row issued meanwhile for this user (a newer admin reset) wins, and this insert is a no-op.
        let result = sqlx::query("INSERT INTO password_reset_tokens (user_id, token_hash, expires_at) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
            .bind(user_id)
            .bind(token_hash)
            .bind(expires_at)
            .execute(&self.pool)
            .await
            .map_err(infra)?;
        Ok(result.rows_affected() > 0)
    }

    async fn is_pending(&self, user_id: Uuid) -> Result<bool, DomainError> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM password_reset_tokens WHERE user_id = $1)",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(infra)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::test_support::{in_hours, seed_user};
    use std::sync::Arc;

    async fn count_rows(pool: &PgPool) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM password_reset_tokens")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn replace_inserts_a_reset_that_consume_then_returns(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresPasswordResetStore::new(pool.clone());
        let expires_at = in_hours(1);

        store.replace(user, "hash-a", expires_at).await.unwrap();

        assert_eq!(count_rows(&pool).await, 1);
        assert_eq!(
            store.consume("hash-a").await.unwrap(),
            Some(PasswordReset {
                user_id: user,
                expires_at
            })
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn consume_is_single_use_and_deletes_the_row(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresPasswordResetStore::new(pool.clone());
        store.replace(user, "hash-a", in_hours(1)).await.unwrap();

        assert!(store.consume("hash-a").await.unwrap().is_some());

        assert!(store.consume("hash-a").await.unwrap().is_none());
        assert_eq!(count_rows(&pool).await, 0);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn consume_of_an_unknown_token_is_none_and_touches_nothing(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresPasswordResetStore::new(pool.clone());
        store.replace(user, "hash-a", in_hours(1)).await.unwrap();

        assert!(store.consume("other-hash").await.unwrap().is_none());

        assert_eq!(count_rows(&pool).await, 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_second_replace_for_the_same_user_leaves_one_row_and_invalidates_the_old_token(
        pool: PgPool,
    ) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresPasswordResetStore::new(pool.clone());
        store.replace(user, "old-hash", in_hours(1)).await.unwrap();
        let new_expiry = in_hours(1);

        store.replace(user, "new-hash", new_expiry).await.unwrap();

        assert_eq!(count_rows(&pool).await, 1);
        assert!(
            store.consume("old-hash").await.unwrap().is_none(),
            "the previous link must stop working"
        );
        assert_eq!(
            store.consume("new-hash").await.unwrap(),
            Some(PasswordReset {
                user_id: user,
                expires_at: new_expiry
            })
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn replace_does_not_touch_the_resets_of_other_users(pool: PgPool) {
        let alice = seed_user(&pool, "alice").await;
        let bob = seed_user(&pool, "bob").await;
        let store = PostgresPasswordResetStore::new(pool.clone());
        store.replace(alice, "hash-a", in_hours(1)).await.unwrap();

        store.replace(bob, "hash-b", in_hours(1)).await.unwrap();

        assert_eq!(count_rows(&pool).await, 2);
        assert_eq!(
            store.consume("hash-a").await.unwrap().unwrap().user_id,
            alice
        );
        assert_eq!(store.consume("hash-b").await.unwrap().unwrap().user_id, bob);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_token_hash_cannot_be_shared_by_two_users(pool: PgPool) {
        let alice = seed_user(&pool, "alice").await;
        let bob = seed_user(&pool, "bob").await;
        let store = PostgresPasswordResetStore::new(pool);
        store
            .replace(alice, "same-hash", in_hours(1))
            .await
            .unwrap();

        assert!(store.replace(bob, "same-hash", in_hours(1)).await.is_err());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn an_expired_reset_is_not_consumable(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresPasswordResetStore::new(pool.clone());
        store.replace(user, "hash-a", in_hours(-1)).await.unwrap();

        assert!(store.consume("hash-a").await.unwrap().is_none());
        assert!(store.consume("hash-a").await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn replacing_an_expired_reset_makes_the_new_link_usable(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresPasswordResetStore::new(pool);
        store.replace(user, "old-hash", in_hours(-5)).await.unwrap();

        store.replace(user, "new-hash", in_hours(1)).await.unwrap();

        assert!(store.consume("old-hash").await.unwrap().is_none());
        assert_eq!(
            store.consume("new-hash").await.unwrap().unwrap().user_id,
            user
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn concurrent_consumes_of_the_same_token_have_exactly_one_winner(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = Arc::new(PostgresPasswordResetStore::new(pool.clone()));
        store.replace(user, "hash-a", in_hours(1)).await.unwrap();

        let (a, b, c, d, e, f) = tokio::join!(
            store.consume("hash-a"),
            store.consume("hash-a"),
            store.consume("hash-a"),
            store.consume("hash-a"),
            store.consume("hash-a"),
            store.consume("hash-a")
        );

        let results: Vec<Option<PasswordReset>> =
            [a, b, c, d, e, f].into_iter().map(|r| r.unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.is_some()).count(), 1);
        assert_eq!(results.into_iter().flatten().next().unwrap().user_id, user);
        assert_eq!(count_rows(&pool).await, 0);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn deleting_the_user_cascades_to_the_reset(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresPasswordResetStore::new(pool.clone());
        store.replace(user, "hash-a", in_hours(1)).await.unwrap();

        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user)
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(count_rows(&pool).await, 0);
        assert!(store.consume("hash-a").await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn restore_puts_a_consumed_link_back_with_its_expiry(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresPasswordResetStore::new(pool.clone());
        let expires_at = in_hours(1);
        store.replace(user, "hash-a", expires_at).await.unwrap();
        store.consume("hash-a").await.unwrap().unwrap();

        assert!(store.restore(user, "hash-a", expires_at).await.unwrap());

        assert_eq!(
            store.consume("hash-a").await.unwrap(),
            Some(PasswordReset {
                user_id: user,
                expires_at
            })
        );
    }

    /// The race `restore` exists for: a newer admin reset landed between the consume and the failed password write.
    #[sqlx::test(migrations = "../../migrations")]
    async fn restore_never_overwrites_a_link_issued_meanwhile(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresPasswordResetStore::new(pool.clone());
        store.replace(user, "old-hash", in_hours(1)).await.unwrap();
        let consumed = store.consume("old-hash").await.unwrap().unwrap();
        store.replace(user, "new-hash", in_hours(1)).await.unwrap();

        assert!(
            !store
                .restore(user, "old-hash", consumed.expires_at)
                .await
                .unwrap()
        );

        assert_eq!(count_rows(&pool).await, 1);
        assert!(
            store.consume("old-hash").await.unwrap().is_none(),
            "the superseded link must stay dead"
        );
        assert!(store.consume("new-hash").await.unwrap().is_some());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn is_pending_reports_any_row_expired_or_not_until_it_is_consumed(pool: PgPool) {
        let alice = seed_user(&pool, "alice").await;
        let bob = seed_user(&pool, "bob").await;
        let store = PostgresPasswordResetStore::new(pool);
        assert!(!store.is_pending(alice).await.unwrap());

        store.replace(alice, "hash-a", in_hours(1)).await.unwrap();
        store.replace(bob, "hash-b", in_hours(-1)).await.unwrap();

        assert!(store.is_pending(alice).await.unwrap());
        assert!(
            store.is_pending(bob).await.unwrap(),
            "an expired link still means a scrambled password"
        );
        store.consume("hash-a").await.unwrap().unwrap();
        assert!(!store.is_pending(alice).await.unwrap());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn replace_for_an_unknown_user_is_an_error(pool: PgPool) {
        let store = PostgresPasswordResetStore::new(pool);

        assert!(matches!(
            store.replace(Uuid::new_v4(), "hash-a", in_hours(1)).await,
            Err(DomainError::Infrastructure(_))
        ));
    }

    /// Resets and invitations are separate tables: a hash living in one is never consumable from the other.
    #[sqlx::test(migrations = "../../migrations")]
    async fn an_invitation_token_is_not_a_reset_token(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        sqlx::query("INSERT INTO user_invitations (user_id, token_hash, expires_at) VALUES ($1, 'hash-a', $2)").bind(user).bind(in_hours(24)).execute(&pool).await.unwrap();
        let store = PostgresPasswordResetStore::new(pool);

        assert!(store.consume("hash-a").await.unwrap().is_none());
    }
}
