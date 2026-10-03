use crate::error::infra;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::invitation::{PendingAccount, PendingAccountPort};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresPendingAccountStore {
    pool: PgPool,
}

impl PostgresPendingAccountStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct PendingRow {
    user_id: Uuid,
    username: String,
    email: String,
    created_at: DateTime<Utc>,
    link_expires_at: DateTime<Utc>,
}

#[async_trait]
impl PendingAccountPort for PostgresPendingAccountStore {
    async fn list(&self) -> Result<Vec<PendingAccount>, DomainError> {
        // The invitation row is what says an account was never activated: activating consumes it.
        let rows = sqlx::query_as::<_, PendingRow>(
            "SELECT u.id AS user_id, u.username, u.email, u.created_at, i.expires_at AS link_expires_at \
             FROM user_invitations i JOIN users u ON u.id = i.user_id \
             ORDER BY u.created_at, u.id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows
            .into_iter()
            .map(|row| PendingAccount {
                user_id: row.user_id,
                username: row.username,
                email: row.email,
                created_at: row.created_at,
                link_expires_at: row.link_expires_at,
            })
            .collect())
    }

    async fn delete(&self, user_id: Uuid) -> Result<bool, DomainError> {
        // One statement that re-checks the invitation, so an account activated since `list` is never deleted. The
        // invitation and anything else that belongs to the account go with it through their ON DELETE CASCADE.
        let result = sqlx::query(
            "DELETE FROM users WHERE id = $1 AND EXISTS (SELECT 1 FROM user_invitations WHERE user_id = $1)",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::test_support::{in_hours, micros, seed_user};
    use crate::postgres::user_invitation_store::PostgresUserInvitationStore;
    use chrono::Duration;
    use ferrisgit_domain::invitation::UserInvitationPort;

    async fn users_named(pool: &PgPool, username: &str) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM users WHERE username = $1")
            .bind(username)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn created_at(pool: &PgPool, user_id: Uuid, at: DateTime<Utc>) {
        sqlx::query("UPDATE users SET created_at = $2 WHERE id = $1")
            .bind(user_id)
            .bind(at)
            .execute(pool)
            .await
            .unwrap();
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn lists_the_accounts_with_a_pending_invitation_oldest_first(pool: PgPool) {
        let invitations = PostgresUserInvitationStore::new(pool.clone());
        let newer = seed_user(&pool, "newer").await;
        let older = seed_user(&pool, "older").await;
        let active = seed_user(&pool, "active").await;
        created_at(&pool, newer, micros(Utc::now() - Duration::days(1))).await;
        created_at(&pool, older, micros(Utc::now() - Duration::days(3))).await;
        let newer_expiry = in_hours(5);
        let older_expiry = in_hours(-2);
        invitations
            .replace(newer, "h1", newer_expiry)
            .await
            .unwrap();
        invitations
            .replace(older, "h2", older_expiry)
            .await
            .unwrap();
        let store = PostgresPendingAccountStore::new(pool.clone());

        let pending = store.list().await.unwrap();

        assert_eq!(
            pending.iter().map(|a| a.user_id).collect::<Vec<_>>(),
            vec![older, newer],
            "oldest first, and the account without an invitation is not there"
        );
        assert!(!pending.iter().any(|a| a.user_id == active));
        assert_eq!(pending[0].username, "older");
        assert_eq!(pending[0].email, "older@example.com");
        assert_eq!(
            pending[0].link_expires_at, older_expiry,
            "an expired link is listed too"
        );
        assert_eq!(pending[1].link_expires_at, newer_expiry);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn the_listed_expiry_follows_a_renewed_link(pool: PgPool) {
        let invitations = PostgresUserInvitationStore::new(pool.clone());
        let user = seed_user(&pool, "alice").await;
        invitations
            .replace(user, "old", in_hours(-1))
            .await
            .unwrap();
        let renewed = in_hours(24);
        invitations.renew(user, "new", renewed).await.unwrap();
        let store = PostgresPendingAccountStore::new(pool.clone());

        let pending = store.list().await.unwrap();

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].link_expires_at, renewed);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn an_account_whose_invitation_was_consumed_is_no_longer_listed(pool: PgPool) {
        let invitations = PostgresUserInvitationStore::new(pool.clone());
        let user = seed_user(&pool, "alice").await;
        invitations
            .replace(user, "hash", in_hours(24))
            .await
            .unwrap();
        invitations.consume("hash").await.unwrap();
        let store = PostgresPendingAccountStore::new(pool);

        assert!(store.list().await.unwrap().is_empty());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn deleting_removes_a_pending_account_and_its_invitation(pool: PgPool) {
        let invitations = PostgresUserInvitationStore::new(pool.clone());
        let doomed = seed_user(&pool, "doomed").await;
        let other = seed_user(&pool, "other").await;
        invitations
            .replace(doomed, "h1", in_hours(24))
            .await
            .unwrap();
        invitations
            .replace(other, "h2", in_hours(24))
            .await
            .unwrap();
        let store = PostgresPendingAccountStore::new(pool.clone());

        assert!(store.delete(doomed).await.unwrap());

        assert_eq!(users_named(&pool, "doomed").await, 0);
        assert_eq!(users_named(&pool, "other").await, 1, "only that account");
        assert!(
            invitations.consume("h1").await.unwrap().is_none(),
            "its link went with it"
        );
        assert_eq!(store.list().await.unwrap().len(), 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn an_active_account_is_never_deleted(pool: PgPool) {
        let active = seed_user(&pool, "active").await;
        let store = PostgresPendingAccountStore::new(pool.clone());

        assert!(
            !store.delete(active).await.unwrap(),
            "no invitation means it was activated"
        );

        assert_eq!(users_named(&pool, "active").await, 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn an_account_activated_since_the_list_is_not_deleted(pool: PgPool) {
        let invitations = PostgresUserInvitationStore::new(pool.clone());
        let user = seed_user(&pool, "alice").await;
        invitations
            .replace(user, "hash", in_hours(24))
            .await
            .unwrap();
        let store = PostgresPendingAccountStore::new(pool.clone());
        let listed = store.list().await.unwrap();
        assert_eq!(listed.len(), 1);

        // The owner follows the link between the list and the delete.
        invitations.consume("hash").await.unwrap();

        assert!(!store.delete(listed[0].user_id).await.unwrap());
        assert_eq!(users_named(&pool, "alice").await, 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn deleting_an_unknown_account_is_false_not_an_error(pool: PgPool) {
        let store = PostgresPendingAccountStore::new(pool);

        assert!(!store.delete(Uuid::new_v4()).await.unwrap());
    }
}
