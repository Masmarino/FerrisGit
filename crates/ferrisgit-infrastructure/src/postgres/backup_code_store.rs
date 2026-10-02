use crate::error::infra;
use async_trait::async_trait;
use ferrisgit_application::mfa_crypto::verify_backup_code;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::mfa::BackupCodePort;
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresBackupCodeStore {
    pool: PgPool,
}

impl PostgresBackupCodeStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BackupCodePort for PostgresBackupCodeStore {
    async fn replace_all(&self, user_id: Uuid, code_hashes: &[String]) -> Result<(), DomainError> {
        let mut tx = self.pool.begin().await.map_err(infra)?;
        // Serializes concurrent replacements for a user: under READ COMMITTED, two interleaved delete-then-insert
        // transactions would both survive and leave two sets of codes.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(format!("mfa-backup-codes:{user_id}"))
            .execute(&mut *tx)
            .await
            .map_err(infra)?;
        sqlx::query("DELETE FROM mfa_backup_codes WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(infra)?;
        sqlx::query("INSERT INTO mfa_backup_codes (user_id, code_hash) SELECT $1, hash FROM UNNEST($2::text[]) AS t(hash)")
            .bind(user_id)
            .bind(code_hashes)
            .execute(&mut *tx)
            .await
            .map_err(infra)?;
        tx.commit().await.map_err(infra)
    }

    async fn try_consume(&self, user_id: Uuid, plaintext_code: &str) -> Result<bool, DomainError> {
        let candidates: Vec<(Uuid, String)> = sqlx::query_as(
            "SELECT id, code_hash FROM mfa_backup_codes WHERE user_id = $1 AND used_at IS NULL",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        // Check every unused hash with no early exit, so timing doesn't reveal which position matched.
        let mut matched: Option<Uuid> = None;
        for (id, hash) in &candidates {
            if verify_backup_code(plaintext_code, hash) && matched.is_none() {
                matched = Some(*id);
            }
        }
        let Some(id) = matched else { return Ok(false) };
        // This is what makes codes single-use: when two consumers race on a row, only one UPDATE sees `used_at IS NULL`.
        let result = sqlx::query("UPDATE mfa_backup_codes SET used_at = now() WHERE id = $1 AND user_id = $2 AND used_at IS NULL")
            .bind(id)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(infra)?;
        Ok(result.rows_affected() == 1)
    }

    async fn count_unused(&self, user_id: Uuid) -> Result<i64, DomainError> {
        sqlx::query_scalar(
            "SELECT count(*) FROM mfa_backup_codes WHERE user_id = $1 AND used_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(infra)
    }

    async fn delete_all(&self, user_id: Uuid) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM mfa_backup_codes WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(infra)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::test_support::seed_user;
    use ferrisgit_application::mfa_crypto::{generate_backup_codes, hash_backup_code};
    use std::sync::Arc;

    async fn total_rows(pool: &PgPool, user: Uuid) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM mfa_backup_codes WHERE user_id = $1")
            .bind(user)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn replace_all_stores_the_given_hashes_and_count_unused_counts_them(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresBackupCodeStore::new(pool.clone());
        let (_, hashes) = generate_backup_codes();

        store.replace_all(user, &hashes).await.unwrap();

        assert_eq!(store.count_unused(user).await.unwrap(), 10);
        let mut stored: Vec<String> =
            sqlx::query_scalar("SELECT code_hash FROM mfa_backup_codes WHERE user_id = $1")
                .bind(user)
                .fetch_all(&pool)
                .await
                .unwrap();
        let mut expected = hashes.clone();
        stored.sort();
        expected.sort();
        assert_eq!(stored, expected);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn replace_all_invalidates_every_previous_code_used_or_not(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresBackupCodeStore::new(pool.clone());
        let (old_codes, old_hashes) = generate_backup_codes();
        store.replace_all(user, &old_hashes).await.unwrap();
        assert!(store.try_consume(user, &old_codes[0]).await.unwrap());
        let (new_codes, new_hashes) = generate_backup_codes();

        store.replace_all(user, &new_hashes).await.unwrap();

        assert_eq!(
            total_rows(&pool, user).await,
            10,
            "old rows, used ones included, are gone"
        );
        assert!(!store.try_consume(user, &old_codes[1]).await.unwrap());
        assert!(store.try_consume(user, &new_codes[0]).await.unwrap());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn replace_all_only_touches_the_given_user(pool: PgPool) {
        let alice = seed_user(&pool, "alice").await;
        let bob = seed_user(&pool, "bob").await;
        let store = PostgresBackupCodeStore::new(pool);
        let (_, alice_hashes) = generate_backup_codes();
        let (_, bob_hashes) = generate_backup_codes();
        store.replace_all(alice, &alice_hashes).await.unwrap();

        store.replace_all(bob, &bob_hashes).await.unwrap();

        assert_eq!(store.count_unused(alice).await.unwrap(), 10);
        assert_eq!(store.count_unused(bob).await.unwrap(), 10);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn concurrent_replacements_leave_exactly_one_set(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = Arc::new(PostgresBackupCodeStore::new(pool.clone()));
        let (_, a) = generate_backup_codes();
        let (_, b) = generate_backup_codes();
        let (_, c) = generate_backup_codes();

        let (ra, rb, rc) = tokio::join!(
            store.replace_all(user, &a),
            store.replace_all(user, &b),
            store.replace_all(user, &c)
        );
        ra.unwrap();
        rb.unwrap();
        rc.unwrap();

        assert_eq!(total_rows(&pool, user).await, 10);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn try_consume_with_the_right_code_succeeds_once(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresBackupCodeStore::new(pool);
        let (codes, hashes) = generate_backup_codes();
        store.replace_all(user, &hashes).await.unwrap();

        assert!(store.try_consume(user, &codes[3]).await.unwrap());
        assert_eq!(store.count_unused(user).await.unwrap(), 9);
        assert!(!store.try_consume(user, &codes[3]).await.unwrap());
        assert_eq!(store.count_unused(user).await.unwrap(), 9);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn try_consume_with_a_wrong_code_fails_and_consumes_nothing(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresBackupCodeStore::new(pool);
        let (_, hashes) = generate_backup_codes();
        store.replace_all(user, &hashes).await.unwrap();

        assert!(
            !store
                .try_consume(user, "not-one-of-the-codes")
                .await
                .unwrap()
        );
        assert!(!store.try_consume(user, "").await.unwrap());
        assert_eq!(store.count_unused(user).await.unwrap(), 10);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_code_of_another_user_is_refused(pool: PgPool) {
        let alice = seed_user(&pool, "alice").await;
        let bob = seed_user(&pool, "bob").await;
        let store = PostgresBackupCodeStore::new(pool);
        let (alice_codes, alice_hashes) = generate_backup_codes();
        let (_, bob_hashes) = generate_backup_codes();
        store.replace_all(alice, &alice_hashes).await.unwrap();
        store.replace_all(bob, &bob_hashes).await.unwrap();

        assert!(!store.try_consume(bob, &alice_codes[0]).await.unwrap());
        assert_eq!(store.count_unused(alice).await.unwrap(), 10);
        assert_eq!(store.count_unused(bob).await.unwrap(), 10);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn identical_codes_of_two_users_are_consumed_independently(pool: PgPool) {
        let alice = seed_user(&pool, "alice").await;
        let bob = seed_user(&pool, "bob").await;
        let store = PostgresBackupCodeStore::new(pool);
        store
            .replace_all(alice, &[hash_backup_code("shared-code")])
            .await
            .unwrap();
        store
            .replace_all(bob, &[hash_backup_code("shared-code")])
            .await
            .unwrap();

        assert!(store.try_consume(alice, "shared-code").await.unwrap());

        assert_eq!(store.count_unused(bob).await.unwrap(), 1);
        assert!(store.try_consume(bob, "shared-code").await.unwrap());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_legacy_colonless_hash_never_matches(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresBackupCodeStore::new(pool);
        store
            .replace_all(user, &["0123456789abcdef".to_string()])
            .await
            .unwrap();

        assert!(!store.try_consume(user, "0123456789abcdef").await.unwrap());
        assert_eq!(store.count_unused(user).await.unwrap(), 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn concurrent_consumption_of_the_same_code_has_exactly_one_winner(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = Arc::new(PostgresBackupCodeStore::new(pool));
        let (codes, hashes) = generate_backup_codes();
        store.replace_all(user, &hashes).await.unwrap();

        let (a, b, c, d) = tokio::join!(
            store.try_consume(user, &codes[0]),
            store.try_consume(user, &codes[0]),
            store.try_consume(user, &codes[0]),
            store.try_consume(user, &codes[0])
        );

        let wins = [a, b, c, d]
            .into_iter()
            .filter(|r| *r.as_ref().unwrap())
            .count();
        assert_eq!(wins, 1);
        assert_eq!(store.count_unused(user).await.unwrap(), 9);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn delete_all_empties_the_set_of_that_user_only(pool: PgPool) {
        let alice = seed_user(&pool, "alice").await;
        let bob = seed_user(&pool, "bob").await;
        let store = PostgresBackupCodeStore::new(pool.clone());
        let (_, alice_hashes) = generate_backup_codes();
        let (_, bob_hashes) = generate_backup_codes();
        store.replace_all(alice, &alice_hashes).await.unwrap();
        store.replace_all(bob, &bob_hashes).await.unwrap();

        store.delete_all(alice).await.unwrap();

        assert_eq!(total_rows(&pool, alice).await, 0);
        assert_eq!(store.count_unused(bob).await.unwrap(), 10);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn deleting_the_user_cascades_to_the_codes(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = PostgresBackupCodeStore::new(pool.clone());
        let (_, hashes) = generate_backup_codes();
        store.replace_all(user, &hashes).await.unwrap();

        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user)
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(total_rows(&pool, user).await, 0);
    }
}
