use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::mfa::{TotpCredential, TotpCredentialPort};
use ferrisgit_domain::secret_encryption::SecretEncryptorPort;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

pub struct PostgresTotpCredentialStore {
    pool: PgPool,
    encryptor: Arc<dyn SecretEncryptorPort>,
}

impl PostgresTotpCredentialStore {
    pub fn new(pool: PgPool, encryptor: Arc<dyn SecretEncryptorPort>) -> Self {
        Self { pool, encryptor }
    }
}

#[derive(sqlx::FromRow)]
struct Row {
    user_id: Uuid,
    encrypted_secret: Vec<u8>,
    confirmed: bool,
    last_used_step: Option<i64>,
    created_at: DateTime<Utc>,
}

fn infra(e: sqlx::Error) -> DomainError {
    DomainError::Infrastructure(e.to_string())
}

#[async_trait]
impl TotpCredentialPort for PostgresTotpCredentialStore {
    async fn get(&self, user_id: Uuid) -> Result<Option<TotpCredential>, DomainError> {
        let row = sqlx::query_as::<_, Row>("SELECT user_id, encrypted_secret, confirmed, last_used_step, created_at FROM totp_credentials WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(infra)?;
        // A secret that can't be decrypted (key rotated, dump restored under another key) is an error, not "no
        // credential". Treating it as absent would let the user re-enrol around a factor that still exists.
        // An admin can reset the user's MFA to recover.
        row.map(|row| {
            Ok(TotpCredential {
                user_id: row.user_id,
                secret: self.encryptor.decrypt(&row.encrypted_secret)?,
                confirmed: row.confirmed,
                last_used_step: row.last_used_step,
                created_at: row.created_at,
            })
        })
        .transpose()
    }

    async fn upsert(&self, credential: &TotpCredential) -> Result<bool, DomainError> {
        let encrypted_secret = self.encryptor.encrypt(&credential.secret)?;
        // The WHERE on the conflict branch is what refuses to overwrite a confirmed factor, atomically.
        let result = sqlx::query(
            "INSERT INTO totp_credentials (user_id, encrypted_secret, confirmed, last_used_step, created_at) VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (user_id) DO UPDATE SET encrypted_secret = EXCLUDED.encrypted_secret, confirmed = EXCLUDED.confirmed, \
             last_used_step = EXCLUDED.last_used_step, created_at = EXCLUDED.created_at \
             WHERE totp_credentials.confirmed = false",
        )
        .bind(credential.user_id)
        .bind(encrypted_secret)
        .bind(credential.confirmed)
        .bind(credential.last_used_step)
        .bind(credential.created_at)
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(result.rows_affected() == 1)
    }

    async fn set_last_used_step(&self, user_id: Uuid, step: i64) -> Result<bool, DomainError> {
        let result = sqlx::query("UPDATE totp_credentials SET last_used_step = $2 WHERE user_id = $1 AND (last_used_step IS NULL OR last_used_step < $2)")
            .bind(user_id)
            .bind(step)
            .execute(&self.pool)
            .await
            .map_err(infra)?;
        Ok(result.rows_affected() == 1)
    }

    async fn confirm(&self, user_id: Uuid, expected_step: i64) -> Result<bool, DomainError> {
        let result = sqlx::query("UPDATE totp_credentials SET confirmed = true WHERE user_id = $1 AND confirmed = false AND last_used_step = $2")
            .bind(user_id)
            .bind(expected_step)
            .execute(&self.pool)
            .await
            .map_err(infra)?;
        Ok(result.rows_affected() == 1)
    }

    async fn delete(&self, user_id: Uuid) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM totp_credentials WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(infra)?;
        Ok(())
    }

    async fn confirmed_user_ids(&self, user_ids: &[Uuid]) -> Result<Vec<Uuid>, DomainError> {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT user_id FROM totp_credentials WHERE confirmed AND user_id = ANY($1)",
        )
        .bind(user_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(infra)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aes_gcm_encryptor::AesGcmSecretEncryptor;
    use crate::postgres::test_support::seed_user;

    fn store(pool: PgPool) -> PostgresTotpCredentialStore {
        PostgresTotpCredentialStore::new(pool, Arc::new(AesGcmSecretEncryptor::new(&[7u8; 32])))
    }

    fn credential(
        user_id: Uuid,
        secret: &str,
        confirmed: bool,
        last_used_step: Option<i64>,
    ) -> TotpCredential {
        // Postgres keeps microseconds: truncate so a round-trip compares equal.
        let created_at = DateTime::from_timestamp_micros(Utc::now().timestamp_micros()).unwrap();
        TotpCredential {
            user_id,
            secret: secret.to_string(),
            confirmed,
            last_used_step,
            created_at,
        }
    }

    async fn count_rows(pool: &PgPool) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM totp_credentials")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn get_on_a_user_without_a_credential_is_none(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;

        assert!(store(pool).get(user).await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn upsert_then_get_round_trips_every_field_and_decrypts_the_secret(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = store(pool);
        let stored = credential(user, "JBSWY3DPEHPK3PXP", false, Some(42));

        assert!(store.upsert(&stored).await.unwrap());

        assert_eq!(store.get(user).await.unwrap(), Some(stored));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn the_secret_is_stored_encrypted(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = store(pool.clone());
        store
            .upsert(&credential(user, "JBSWY3DPEHPK3PXP", false, None))
            .await
            .unwrap();

        let stored: Vec<u8> =
            sqlx::query_scalar("SELECT encrypted_secret FROM totp_credentials WHERE user_id = $1")
                .bind(user)
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_ne!(stored, b"JBSWY3DPEHPK3PXP".to_vec());
        assert!(
            stored.len() > 12,
            "expected nonce + ciphertext, got {} bytes",
            stored.len()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_second_upsert_replaces_an_unconfirmed_credential_and_resets_the_step(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = store(pool.clone());
        assert!(
            store
                .upsert(&credential(user, "FIRSTSECRET", false, Some(9)))
                .await
                .unwrap()
        );
        let replacement = credential(user, "SECONDSECRET", false, None);

        assert!(store.upsert(&replacement).await.unwrap());

        assert_eq!(count_rows(&pool).await, 1);
        assert_eq!(store.get(user).await.unwrap(), Some(replacement));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn upsert_refuses_to_overwrite_a_confirmed_credential(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = store(pool.clone());
        let confirmed = credential(user, "CONFIRMEDSECRET", true, Some(100));
        assert!(store.upsert(&confirmed).await.unwrap());

        let written = store
            .upsert(&credential(user, "ATTACKERSECRET", false, None))
            .await
            .unwrap();

        assert!(!written, "a confirmed row must not be overwritten");
        assert_eq!(count_rows(&pool).await, 1);
        assert_eq!(store.get(user).await.unwrap(), Some(confirmed));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn confirm_only_succeeds_for_the_expected_step_and_only_once(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = store(pool);
        store
            .upsert(&credential(user, "SECRET", false, Some(100)))
            .await
            .unwrap();

        assert!(
            !store.confirm(user, 99).await.unwrap(),
            "a different step must not confirm"
        );
        assert!(!store.get(user).await.unwrap().unwrap().confirmed);
        assert!(store.confirm(user, 100).await.unwrap());
        assert!(store.get(user).await.unwrap().unwrap().confirmed);
        assert!(
            !store.confirm(user, 100).await.unwrap(),
            "an already confirmed credential is not confirmed again"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn confirm_without_a_credential_or_without_a_recorded_step_is_false(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = store(pool);

        assert!(!store.confirm(user, 1).await.unwrap());

        store
            .upsert(&credential(user, "SECRET", false, None))
            .await
            .unwrap();
        assert!(!store.confirm(user, 1).await.unwrap());
        assert!(!store.get(user).await.unwrap().unwrap().confirmed);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_credential_replaced_after_the_code_was_verified_cannot_be_confirmed_with_the_old_step(
        pool: PgPool,
    ) {
        let user = seed_user(&pool, "alice").await;
        let store = store(pool);
        store
            .upsert(&credential(user, "OLDSECRET", false, Some(100)))
            .await
            .unwrap();
        // A concurrent re-enrolment swaps the secret and resets the step before the first flow confirms.
        store
            .upsert(&credential(user, "NEWSECRET", false, None))
            .await
            .unwrap();

        assert!(!store.confirm(user, 100).await.unwrap());
        assert!(!store.get(user).await.unwrap().unwrap().confirmed);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn concurrent_confirms_have_exactly_one_winner(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = Arc::new(store(pool));
        store
            .upsert(&credential(user, "SECRET", false, Some(100)))
            .await
            .unwrap();

        let (a, b, c, d) = tokio::join!(
            store.confirm(user, 100),
            store.confirm(user, 100),
            store.confirm(user, 100),
            store.confirm(user, 100)
        );

        let wins = [a, b, c, d]
            .into_iter()
            .filter(|r| *r.as_ref().unwrap())
            .count();
        assert_eq!(wins, 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn set_last_used_step_is_a_compare_and_swap(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = store(pool);
        store
            .upsert(&credential(user, "SECRET", true, None))
            .await
            .unwrap();

        assert!(store.set_last_used_step(user, 100).await.unwrap());
        assert!(!store.set_last_used_step(user, 100).await.unwrap());
        assert!(!store.set_last_used_step(user, 99).await.unwrap());
        assert!(store.set_last_used_step(user, 101).await.unwrap());
        assert_eq!(
            store.get(user).await.unwrap().unwrap().last_used_step,
            Some(101)
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn set_last_used_step_without_a_credential_is_false(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;

        assert!(!store(pool).set_last_used_step(user, 5).await.unwrap());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn concurrent_set_last_used_step_with_the_same_step_has_exactly_one_winner(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = Arc::new(store(pool));
        store
            .upsert(&credential(user, "SECRET", true, None))
            .await
            .unwrap();

        let (a, b, c, d) = tokio::join!(
            store.set_last_used_step(user, 100),
            store.set_last_used_step(user, 100),
            store.set_last_used_step(user, 100),
            store.set_last_used_step(user, 100)
        );

        let wins = [a, b, c, d]
            .into_iter()
            .filter(|r| *r.as_ref().unwrap())
            .count();
        assert_eq!(wins, 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn delete_removes_the_credential_and_is_idempotent(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = store(pool);
        store
            .upsert(&credential(user, "SECRET", true, None))
            .await
            .unwrap();

        store.delete(user).await.unwrap();
        store.delete(user).await.unwrap();

        assert!(store.get(user).await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn deleting_the_user_cascades_to_the_credential(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = store(pool.clone());
        store
            .upsert(&credential(user, "SECRET", true, None))
            .await
            .unwrap();

        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user)
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(count_rows(&pool).await, 0);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_credential_encrypted_under_another_key_is_an_error_not_an_absence(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        store(pool.clone())
            .upsert(&credential(user, "SECRET", true, None))
            .await
            .unwrap();
        let other_key = PostgresTotpCredentialStore::new(
            pool,
            Arc::new(AesGcmSecretEncryptor::new(&[9u8; 32])),
        );

        assert!(other_key.get(user).await.is_err());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn confirmed_user_ids_returns_only_users_with_a_confirmed_credential(pool: PgPool) {
        let confirmed = seed_user(&pool, "alice").await;
        let enrolling = seed_user(&pool, "bob").await;
        let without = seed_user(&pool, "carol").await;
        let confirmed_not_asked = seed_user(&pool, "dave").await;
        let store = store(pool);
        store
            .upsert(&credential(confirmed, "SECRET", true, None))
            .await
            .unwrap();
        store
            .upsert(&credential(enrolling, "SECRET", false, Some(5)))
            .await
            .unwrap();
        store
            .upsert(&credential(confirmed_not_asked, "SECRET", true, None))
            .await
            .unwrap();

        let found = store
            .confirmed_user_ids(&[confirmed, enrolling, without, Uuid::new_v4()])
            .await
            .unwrap();

        assert_eq!(found, vec![confirmed]);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn confirmed_user_ids_of_no_users_is_empty(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = store(pool);
        store
            .upsert(&credential(user, "SECRET", true, None))
            .await
            .unwrap();

        assert!(store.confirmed_user_ids(&[]).await.unwrap().is_empty());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn confirmed_user_ids_reflects_a_confirmation_and_a_deletion(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = store(pool);
        store
            .upsert(&credential(user, "SECRET", false, Some(7)))
            .await
            .unwrap();
        assert!(store.confirmed_user_ids(&[user]).await.unwrap().is_empty());

        store.confirm(user, 7).await.unwrap();
        assert_eq!(store.confirmed_user_ids(&[user]).await.unwrap(), vec![user]);

        store.delete(user).await.unwrap();
        assert!(store.confirmed_user_ids(&[user]).await.unwrap().is_empty());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn confirmed_user_ids_never_decrypts_so_a_garbage_or_foreign_key_row_still_counts(
        pool: PgPool,
    ) {
        let garbage = seed_user(&pool, "alice").await;
        let foreign_key = seed_user(&pool, "bob").await;
        sqlx::query("INSERT INTO totp_credentials (user_id, encrypted_secret, confirmed) VALUES ($1, $2, true)").bind(garbage).bind(vec![0u8, 1, 2]).execute(&pool).await.unwrap();
        let other_key = PostgresTotpCredentialStore::new(
            pool.clone(),
            Arc::new(AesGcmSecretEncryptor::new(&[9u8; 32])),
        );
        other_key
            .upsert(&credential(foreign_key, "SECRET", true, None))
            .await
            .unwrap();
        let store = store(pool);
        assert!(
            store.get(garbage).await.is_err(),
            "precondition: the garbage row cannot be decrypted"
        );
        assert!(
            store.get(foreign_key).await.is_err(),
            "precondition: the foreign-key row cannot be decrypted"
        );

        let mut found = store
            .confirmed_user_ids(&[garbage, foreign_key])
            .await
            .unwrap();
        found.sort();

        let mut expected = vec![garbage, foreign_key];
        expected.sort();
        assert_eq!(found, expected);
    }
}
