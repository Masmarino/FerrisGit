use crate::error::infra;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::invitation::{Invitation, UserInvitationPort};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresUserInvitationStore {
    pool: PgPool,
}

impl PostgresUserInvitationStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct InvitationRow {
    user_id: Uuid,
    expires_at: DateTime<Utc>,
}

#[async_trait]
impl UserInvitationPort for PostgresUserInvitationStore {
    async fn replace(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        // One statement, so two concurrent replacements for a user cannot leave two rows (`user_id` is UNIQUE).
        sqlx::query(
            "INSERT INTO user_invitations (user_id, token_hash, expires_at) VALUES ($1, $2, $3) \
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

    async fn renew(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<bool, DomainError> {
        // Conditional on the row still existing (one statement): an activation that consumed it first leaves 0 rows.
        let result = sqlx::query("UPDATE user_invitations SET token_hash = $2, expires_at = $3, created_at = now() WHERE user_id = $1")
            .bind(user_id)
            .bind(token_hash)
            .bind(expires_at)
            .execute(&self.pool)
            .await
            .map_err(infra)?;
        Ok(result.rows_affected() > 0)
    }

    async fn consume(&self, token_hash: &str) -> Result<Option<Invitation>, DomainError> {
        // Delete and return in one statement, so only one of several concurrent consumers gets the row. An
        // expired invitation never matches. It stays until replaced, so the admin list keeps showing "invited".
        let row = sqlx::query_as::<_, InvitationRow>("DELETE FROM user_invitations WHERE token_hash = $1 AND expires_at > now() RETURNING user_id, expires_at")
            .bind(token_hash)
            .fetch_optional(&self.pool)
            .await
            .map_err(infra)?;
        Ok(row.map(|row| Invitation {
            user_id: row.user_id,
            expires_at: row.expires_at,
        }))
    }

    async fn expiries(&self, user_ids: &[Uuid]) -> Result<Vec<(Uuid, DateTime<Utc>)>, DomainError> {
        let rows = sqlx::query_as::<_, InvitationRow>(
            "SELECT user_id, expires_at FROM user_invitations WHERE user_id = ANY($1)",
        )
        .bind(user_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows
            .into_iter()
            .map(|row| (row.user_id, row.expires_at))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::test_support::{in_hours, seed_user};
    use std::sync::Arc;

    async fn count_rows(pool: &PgPool) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM user_invitations")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn replace_inserts_an_invitation_that_consume_then_returns(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresUserInvitationStore::new(pool.clone());
        let expires_at = in_hours(24);

        store.replace(user, "hash-a", expires_at).await.unwrap();

        assert_eq!(count_rows(&pool).await, 1);
        assert_eq!(
            store.consume("hash-a").await.unwrap(),
            Some(Invitation {
                user_id: user,
                expires_at
            })
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn renew_swaps_the_token_and_expiry_of_an_existing_invitation(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let other = seed_user(&pool, "bob").await;
        let store = PostgresUserInvitationStore::new(pool.clone());
        store.replace(user, "old-hash", in_hours(1)).await.unwrap();
        store.replace(other, "bob-hash", in_hours(2)).await.unwrap();
        let new_expiry = in_hours(24);

        assert!(store.renew(user, "new-hash", new_expiry).await.unwrap());

        assert_eq!(count_rows(&pool).await, 2);
        assert!(
            store.consume("old-hash").await.unwrap().is_none(),
            "the previous link must stop working"
        );
        assert_eq!(
            store.consume("new-hash").await.unwrap(),
            Some(Invitation {
                user_id: user,
                expires_at: new_expiry
            })
        );
        assert!(
            store.consume("bob-hash").await.unwrap().is_some(),
            "another user's invitation is untouched"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn renew_revives_an_expired_invitation(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresUserInvitationStore::new(pool.clone());
        store.replace(user, "old-hash", in_hours(-1)).await.unwrap();

        assert!(store.renew(user, "new-hash", in_hours(24)).await.unwrap());

        assert!(store.consume("new-hash").await.unwrap().is_some());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn renew_for_a_user_without_an_invitation_is_false_and_creates_nothing(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresUserInvitationStore::new(pool.clone());

        assert!(!store.renew(user, "new-hash", in_hours(24)).await.unwrap());

        assert_eq!(count_rows(&pool).await, 0);
        assert!(store.consume("new-hash").await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn renew_after_the_invitation_was_consumed_is_false(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresUserInvitationStore::new(pool.clone());
        store.replace(user, "hash-a", in_hours(24)).await.unwrap();
        assert!(store.consume("hash-a").await.unwrap().is_some());

        assert!(
            !store.renew(user, "new-hash", in_hours(24)).await.unwrap(),
            "an activated account must not get a new link"
        );

        assert_eq!(count_rows(&pool).await, 0);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn consume_is_single_use_and_deletes_the_row(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresUserInvitationStore::new(pool.clone());
        store.replace(user, "hash-a", in_hours(24)).await.unwrap();

        assert!(store.consume("hash-a").await.unwrap().is_some());

        assert!(store.consume("hash-a").await.unwrap().is_none());
        assert_eq!(count_rows(&pool).await, 0);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn consume_of_an_unknown_token_is_none_and_touches_nothing(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresUserInvitationStore::new(pool.clone());
        store.replace(user, "hash-a", in_hours(24)).await.unwrap();

        assert!(store.consume("other-hash").await.unwrap().is_none());

        assert_eq!(count_rows(&pool).await, 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_second_replace_for_the_same_user_leaves_one_row_and_invalidates_the_old_token(
        pool: PgPool,
    ) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresUserInvitationStore::new(pool.clone());
        store.replace(user, "old-hash", in_hours(1)).await.unwrap();
        let new_expiry = in_hours(24);

        store.replace(user, "new-hash", new_expiry).await.unwrap();

        assert_eq!(count_rows(&pool).await, 1);
        assert!(
            store.consume("old-hash").await.unwrap().is_none(),
            "the previous link must stop working"
        );
        assert_eq!(
            store.consume("new-hash").await.unwrap(),
            Some(Invitation {
                user_id: user,
                expires_at: new_expiry
            })
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn replace_with_the_same_hash_for_the_same_user_is_accepted(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresUserInvitationStore::new(pool.clone());
        store.replace(user, "hash-a", in_hours(1)).await.unwrap();

        store.replace(user, "hash-a", in_hours(24)).await.unwrap();

        assert_eq!(count_rows(&pool).await, 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn replace_does_not_touch_the_invitations_of_other_users(pool: PgPool) {
        let alice = seed_user(&pool, "alice").await;
        let bob = seed_user(&pool, "bob").await;
        let store = PostgresUserInvitationStore::new(pool.clone());
        store.replace(alice, "hash-a", in_hours(24)).await.unwrap();

        store.replace(bob, "hash-b", in_hours(24)).await.unwrap();

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
        let store = PostgresUserInvitationStore::new(pool);
        store
            .replace(alice, "same-hash", in_hours(24))
            .await
            .unwrap();

        assert!(store.replace(bob, "same-hash", in_hours(24)).await.is_err());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn an_expired_invitation_is_not_consumable_and_stays_in_place(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresUserInvitationStore::new(pool.clone());
        store.replace(user, "hash-a", in_hours(-1)).await.unwrap();

        assert!(store.consume("hash-a").await.unwrap().is_none());
        assert!(store.consume("hash-a").await.unwrap().is_none());

        assert_eq!(
            count_rows(&pool).await,
            1,
            "an expired row stays (the admin list still shows the user as invited)"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn replacing_an_expired_invitation_makes_the_new_link_usable(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresUserInvitationStore::new(pool);
        store.replace(user, "old-hash", in_hours(-5)).await.unwrap();

        store.replace(user, "new-hash", in_hours(24)).await.unwrap();

        assert!(store.consume("old-hash").await.unwrap().is_none());
        assert_eq!(
            store.consume("new-hash").await.unwrap().unwrap().user_id,
            user
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn concurrent_consumes_of_the_same_token_have_exactly_one_winner(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = Arc::new(PostgresUserInvitationStore::new(pool.clone()));
        store.replace(user, "hash-a", in_hours(24)).await.unwrap();

        let (a, b, c, d, e, f) = tokio::join!(
            store.consume("hash-a"),
            store.consume("hash-a"),
            store.consume("hash-a"),
            store.consume("hash-a"),
            store.consume("hash-a"),
            store.consume("hash-a")
        );

        let results: Vec<Option<Invitation>> =
            [a, b, c, d, e, f].into_iter().map(|r| r.unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.is_some()).count(), 1);
        assert_eq!(results.into_iter().flatten().next().unwrap().user_id, user);
        assert_eq!(count_rows(&pool).await, 0);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn expiries_returns_only_the_requested_users_including_expired_ones(pool: PgPool) {
        let alice = seed_user(&pool, "alice").await;
        let bob = seed_user(&pool, "bob").await;
        let carol = seed_user(&pool, "carol").await;
        let dave = seed_user(&pool, "dave").await;
        let store = PostgresUserInvitationStore::new(pool);
        let (alice_expiry, bob_expiry, carol_expiry) = (in_hours(24), in_hours(-3), in_hours(12));
        store.replace(alice, "hash-a", alice_expiry).await.unwrap();
        store.replace(bob, "hash-b", bob_expiry).await.unwrap();
        store.replace(carol, "hash-c", carol_expiry).await.unwrap();

        let mut found = store.expiries(&[alice, bob, dave]).await.unwrap();
        found.sort_by_key(|(_, expiry)| *expiry);

        assert_eq!(found, vec![(bob, bob_expiry), (alice, alice_expiry)]);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn expiries_of_no_users_is_empty(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresUserInvitationStore::new(pool);
        store.replace(user, "hash-a", in_hours(24)).await.unwrap();

        assert!(store.expiries(&[]).await.unwrap().is_empty());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn expiries_does_not_consume_anything(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresUserInvitationStore::new(pool.clone());
        store.replace(user, "hash-a", in_hours(24)).await.unwrap();

        store.expiries(&[user]).await.unwrap();

        assert_eq!(count_rows(&pool).await, 1);
        assert!(store.consume("hash-a").await.unwrap().is_some());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn deleting_the_user_cascades_to_the_invitation(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresUserInvitationStore::new(pool.clone());
        store.replace(user, "hash-a", in_hours(24)).await.unwrap();

        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user)
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(count_rows(&pool).await, 0);
        assert!(store.consume("hash-a").await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn replace_for_an_unknown_user_is_an_error(pool: PgPool) {
        let store = PostgresUserInvitationStore::new(pool);

        assert!(matches!(
            store.replace(Uuid::new_v4(), "hash-a", in_hours(24)).await,
            Err(DomainError::Infrastructure(_))
        ));
    }
}
