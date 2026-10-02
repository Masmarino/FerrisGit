use crate::error::infra;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::webauthn::{StoredPasskey, WebauthnCredentialPort};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresWebauthnCredentialStore {
    pool: PgPool,
}

impl PostgresWebauthnCredentialStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    user_id: Uuid,
    name: String,
    credential_id: Vec<u8>,
    passkey: serde_json::Value,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
}

impl From<Row> for StoredPasskey {
    fn from(row: Row) -> Self {
        // `jsonb` does not keep the original text: the JSON comes back compact, with its keys in Postgres' order.
        Self {
            id: row.id,
            user_id: row.user_id,
            name: row.name,
            credential_id: row.credential_id,
            passkey_json: row.passkey.to_string(),
            created_at: row.created_at,
            last_used_at: row.last_used_at,
        }
    }
}

/// The domain hands the passkey over as text, and it is stored as `jsonb`. The parse error is not
/// forwarded because it could quote a fragment of the key material.
fn parse_passkey(passkey_json: &str) -> Result<serde_json::Value, DomainError> {
    serde_json::from_str(passkey_json)
        .map_err(|_| DomainError::Infrastructure("stored passkey is not valid JSON".to_string()))
}

#[async_trait]
impl WebauthnCredentialPort for PostgresWebauthnCredentialStore {
    async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<StoredPasskey>, DomainError> {
        let rows = sqlx::query_as::<_, Row>(
            "SELECT id, user_id, name, credential_id, passkey, created_at, last_used_at FROM webauthn_credentials WHERE user_id = $1 ORDER BY created_at, id",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows.into_iter().map(StoredPasskey::from).collect())
    }

    async fn count_for_user(&self, user_id: Uuid) -> Result<i64, DomainError> {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM webauthn_credentials WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&self.pool)
            .await
            .map_err(infra)
    }

    async fn insert(&self, passkey: &StoredPasskey) -> Result<bool, DomainError> {
        let value = parse_passkey(&passkey.passkey_json)?;
        // The unique index on credential_id settles concurrent registrations of the same credential. One
        // insert lands, and the others are no-ops that never touch the existing row.
        let result = sqlx::query(
            "INSERT INTO webauthn_credentials (id, user_id, name, credential_id, passkey, created_at, last_used_at) VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (credential_id) DO NOTHING",
        )
        .bind(passkey.id)
        .bind(passkey.user_id)
        .bind(&passkey.name)
        .bind(&passkey.credential_id)
        .bind(value)
        .bind(passkey.created_at)
        .bind(passkey.last_used_at)
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(result.rows_affected() == 1)
    }

    async fn update_after_authentication(
        &self,
        id: Uuid,
        passkey_json: &str,
    ) -> Result<(), DomainError> {
        let value = parse_passkey(passkey_json)?;
        let result = sqlx::query(
            "UPDATE webauthn_credentials SET passkey = $2, last_used_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        // The passkey was deleted between the assertion and this write (the user removed it in another session):
        // fail rather than let a login succeed on a credential that no longer exists.
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound("passkey".to_string()));
        }
        Ok(())
    }

    async fn delete(&self, id: Uuid, user_id: Uuid) -> Result<bool, DomainError> {
        let result = sqlx::query("DELETE FROM webauthn_credentials WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(infra)?;
        Ok(result.rows_affected() == 1)
    }

    async fn delete_all_for_user(&self, user_id: Uuid) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM webauthn_credentials WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(infra)?;
        Ok(())
    }

    async fn user_ids_with_passkeys(&self, user_ids: &[Uuid]) -> Result<Vec<Uuid>, DomainError> {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT DISTINCT user_id FROM webauthn_credentials WHERE user_id = ANY($1)",
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
    use crate::postgres::test_support::micros;
    use crate::postgres::test_support::seed_user;
    use std::sync::Arc;

    fn store(pool: PgPool) -> PostgresWebauthnCredentialStore {
        PostgresWebauthnCredentialStore::new(pool)
    }

    fn passkey_json(counter: i64) -> String {
        serde_json::json!({ "cred": { "counter": counter, "cred_id": "AQID", "backup_eligible": true }, "extensions": {} }).to_string()
    }

    fn passkey(user_id: Uuid, name: &str, credential_id: &[u8]) -> StoredPasskey {
        StoredPasskey {
            id: Uuid::new_v4(),
            user_id,
            name: name.to_string(),
            credential_id: credential_id.to_vec(),
            passkey_json: passkey_json(0),
            created_at: micros(Utc::now()),
            last_used_at: None,
        }
    }

    async fn count_rows(pool: &PgPool) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM webauthn_credentials")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_for_a_user_without_passkeys_is_empty(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;

        assert!(store(pool).list_for_user(user).await.unwrap().is_empty());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn insert_then_list_round_trips_every_field(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = store(pool);
        let mut stored = passkey(user, "MacBook", &[0xde, 0xad, 0xbe, 0xef, 0x00, 0xff]);
        stored.last_used_at = Some(micros(Utc::now()));

        assert!(store.insert(&stored).await.unwrap());

        assert_eq!(store.list_for_user(user).await.unwrap(), vec![stored]);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn the_passkey_is_stored_as_jsonb_and_comes_back_as_equivalent_json(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = store(pool.clone());
        let mut stored = passkey(user, "MacBook", &[1]);
        // jsonb drops whitespace and key order but keeps the content
        stored.passkey_json =
            "{ \"z\": [1, 2, {\"b\": null}],\n \"a\": \"caf\u{e9}\" }".to_string();

        assert!(store.insert(&stored).await.unwrap());

        let kind: String = sqlx::query_scalar(
            "SELECT jsonb_typeof(passkey) FROM webauthn_credentials WHERE id = $1",
        )
        .bind(stored.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(kind, "object");
        let back = store.list_for_user(user).await.unwrap().remove(0);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&back.passkey_json).unwrap(),
            serde_json::from_str::<serde_json::Value>(&stored.passkey_json).unwrap()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_passkey_that_is_not_json_is_refused_without_writing_or_quoting_it(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = store(pool.clone());
        let mut stored = passkey(user, "MacBook", &[1]);
        stored.passkey_json = "SECRET-KEY-MATERIAL not json".to_string();

        let error = store.insert(&stored).await.unwrap_err();

        assert!(matches!(error, DomainError::Infrastructure(_)));
        assert!(
            !error.to_string().contains("SECRET-KEY-MATERIAL"),
            "{error}"
        );
        assert_eq!(count_rows(&pool).await, 0);
        assert!(
            store
                .update_after_authentication(stored.id, "not json")
                .await
                .is_err()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_is_scoped_to_the_user_and_ordered_oldest_first_then_by_id(pool: PgPool) {
        let alice = seed_user(&pool, "alice").await;
        let bob = seed_user(&pool, "bob").await;
        let store = store(pool);
        let base = micros(Utc::now());
        let mut newest = passkey(alice, "newest", &[3]);
        newest.created_at = base + chrono::Duration::seconds(10);
        let mut oldest = passkey(alice, "oldest", &[1]);
        oldest.created_at = base;
        let mut tie_a = passkey(alice, "tie-a", &[2]);
        tie_a.created_at = base + chrono::Duration::seconds(5);
        let mut tie_b = passkey(alice, "tie-b", &[4]);
        tie_b.created_at = base + chrono::Duration::seconds(5);
        let (first_tie, second_tie) = if tie_a.id < tie_b.id {
            (tie_a.clone(), tie_b.clone())
        } else {
            (tie_b.clone(), tie_a.clone())
        };
        for p in [
            &newest,
            &tie_b,
            &oldest,
            &tie_a,
            &passkey(bob, "bobs", &[9]),
        ] {
            assert!(store.insert(p).await.unwrap());
        }

        let listed = store.list_for_user(alice).await.unwrap();

        assert_eq!(listed, vec![oldest, first_tie, second_tie, newest]);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_second_insert_with_the_same_credential_id_is_refused_and_overwrites_nothing(
        pool: PgPool,
    ) {
        let user = seed_user(&pool, "alice").await;
        let store = store(pool.clone());
        let original = passkey(user, "original", &[1, 2, 3]);
        assert!(store.insert(&original).await.unwrap());
        let mut duplicate = passkey(user, "duplicate", &[1, 2, 3]);
        duplicate.passkey_json = passkey_json(99);

        assert!(!store.insert(&duplicate).await.unwrap());

        assert_eq!(count_rows(&pool).await, 1);
        assert_eq!(store.list_for_user(user).await.unwrap(), vec![original]);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn another_users_credential_id_is_refused_too_and_the_owner_keeps_it(pool: PgPool) {
        let alice = seed_user(&pool, "alice").await;
        let mallory = seed_user(&pool, "mallory").await;
        let store = store(pool.clone());
        let original = passkey(alice, "alices", &[7, 7, 7]);
        assert!(store.insert(&original).await.unwrap());

        assert!(
            !store
                .insert(&passkey(mallory, "stolen", &[7, 7, 7]))
                .await
                .unwrap()
        );

        assert_eq!(count_rows(&pool).await, 1);
        assert_eq!(store.list_for_user(alice).await.unwrap(), vec![original]);
        assert!(store.list_for_user(mallory).await.unwrap().is_empty());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn concurrent_inserts_of_the_same_credential_id_have_exactly_one_winner(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let other = seed_user(&pool, "bob").await;
        let store = Arc::new(store(pool.clone()));
        let (a, b, c, d) = (
            passkey(user, "a", &[5, 5]),
            passkey(user, "b", &[5, 5]),
            passkey(other, "c", &[5, 5]),
            passkey(other, "d", &[5, 5]),
        );

        let (ra, rb, rc, rd) = tokio::join!(
            store.insert(&a),
            store.insert(&b),
            store.insert(&c),
            store.insert(&d)
        );

        let wins = [ra, rb, rc, rd]
            .into_iter()
            .filter(|r| *r.as_ref().unwrap())
            .count();
        assert_eq!(wins, 1);
        assert_eq!(count_rows(&pool).await, 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn insert_for_an_unknown_user_is_an_error(pool: PgPool) {
        let store = store(pool.clone());

        assert!(
            store
                .insert(&passkey(Uuid::new_v4(), "ghost", &[1]))
                .await
                .is_err()
        );
        assert_eq!(count_rows(&pool).await, 0);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn count_for_user_counts_only_that_users_passkeys(pool: PgPool) {
        let alice = seed_user(&pool, "alice").await;
        let bob = seed_user(&pool, "bob").await;
        let carol = seed_user(&pool, "carol").await;
        let store = store(pool);
        store.insert(&passkey(alice, "one", &[1])).await.unwrap();
        store.insert(&passkey(alice, "two", &[2])).await.unwrap();
        store.insert(&passkey(bob, "three", &[3])).await.unwrap();

        assert_eq!(store.count_for_user(alice).await.unwrap(), 2);
        assert_eq!(store.count_for_user(bob).await.unwrap(), 1);
        assert_eq!(store.count_for_user(carol).await.unwrap(), 0);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn update_after_authentication_replaces_the_json_and_stamps_last_used_at(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let other = seed_user(&pool, "bob").await;
        let store = store(pool);
        let target = passkey(user, "target", &[1]);
        let untouched = passkey(other, "untouched", &[2]);
        store.insert(&target).await.unwrap();
        store.insert(&untouched).await.unwrap();
        let before = Utc::now() - chrono::Duration::seconds(5);

        store
            .update_after_authentication(target.id, &passkey_json(42))
            .await
            .unwrap();

        let updated = store.list_for_user(user).await.unwrap().remove(0);
        let expected: serde_json::Value = serde_json::from_str(&passkey_json(42)).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&updated.passkey_json).unwrap(),
            expected
        );
        assert!(updated.last_used_at.expect("last_used_at is set") > before);
        assert_eq!(
            (
                updated.id,
                updated.name.as_str(),
                updated.credential_id.clone(),
                updated.created_at
            ),
            (target.id, "target", vec![1], target.created_at)
        );
        assert_eq!(
            store.list_for_user(other).await.unwrap(),
            vec![untouched],
            "another user's row is untouched"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn update_after_authentication_of_a_missing_passkey_is_not_found(pool: PgPool) {
        let store = store(pool);

        let error = store
            .update_after_authentication(Uuid::new_v4(), &passkey_json(1))
            .await
            .unwrap_err();

        assert!(matches!(error, DomainError::NotFound(_)), "{error}");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn delete_is_scoped_to_the_owner(pool: PgPool) {
        let alice = seed_user(&pool, "alice").await;
        let mallory = seed_user(&pool, "mallory").await;
        let store = store(pool);
        let stored = passkey(alice, "alices", &[1]);
        store.insert(&stored).await.unwrap();

        assert!(
            !store.delete(stored.id, mallory).await.unwrap(),
            "another user's id must not delete the row"
        );
        assert_eq!(
            store.list_for_user(alice).await.unwrap(),
            vec![stored.clone()]
        );
        assert!(store.delete(stored.id, alice).await.unwrap());
        assert!(store.list_for_user(alice).await.unwrap().is_empty());
        assert!(
            !store.delete(stored.id, alice).await.unwrap(),
            "already deleted"
        );
        assert!(
            !store.delete(Uuid::new_v4(), alice).await.unwrap(),
            "unknown id"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_deleted_credential_id_can_be_registered_again(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = store(pool);
        let first = passkey(user, "first", &[8]);
        store.insert(&first).await.unwrap();
        store.delete(first.id, user).await.unwrap();

        assert!(store.insert(&passkey(user, "again", &[8])).await.unwrap());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn delete_all_for_user_only_removes_that_users_passkeys_and_is_idempotent(pool: PgPool) {
        let alice = seed_user(&pool, "alice").await;
        let bob = seed_user(&pool, "bob").await;
        let store = store(pool);
        store.insert(&passkey(alice, "a1", &[1])).await.unwrap();
        store.insert(&passkey(alice, "a2", &[2])).await.unwrap();
        let bobs = passkey(bob, "b1", &[3]);
        store.insert(&bobs).await.unwrap();

        store.delete_all_for_user(alice).await.unwrap();
        store.delete_all_for_user(alice).await.unwrap();

        assert!(store.list_for_user(alice).await.unwrap().is_empty());
        assert_eq!(store.list_for_user(bob).await.unwrap(), vec![bobs]);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn user_ids_with_passkeys_returns_exactly_the_asked_users_that_have_one(pool: PgPool) {
        let with_two = seed_user(&pool, "alice").await;
        let with_one = seed_user(&pool, "bob").await;
        let without = seed_user(&pool, "carol").await;
        let not_asked = seed_user(&pool, "dave").await;
        let store = store(pool);
        store.insert(&passkey(with_two, "a1", &[1])).await.unwrap();
        store.insert(&passkey(with_two, "a2", &[2])).await.unwrap();
        store.insert(&passkey(with_one, "b1", &[3])).await.unwrap();
        store.insert(&passkey(not_asked, "d1", &[4])).await.unwrap();

        let mut found = store
            .user_ids_with_passkeys(&[with_two, with_one, without, Uuid::new_v4()])
            .await
            .unwrap();
        found.sort();

        let mut expected = vec![with_two, with_one];
        expected.sort();
        assert_eq!(
            found, expected,
            "one entry per user even with several passkeys"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn user_ids_with_passkeys_of_no_users_is_empty(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = store(pool);
        store.insert(&passkey(user, "a1", &[1])).await.unwrap();

        assert!(store.user_ids_with_passkeys(&[]).await.unwrap().is_empty());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn user_ids_with_passkeys_follows_inserts_and_deletes(pool: PgPool) {
        let user = seed_user(&pool, "alice").await;
        let store = store(pool);
        assert!(
            store
                .user_ids_with_passkeys(&[user])
                .await
                .unwrap()
                .is_empty()
        );
        let stored = passkey(user, "a1", &[1]);

        store.insert(&stored).await.unwrap();
        assert_eq!(
            store.user_ids_with_passkeys(&[user]).await.unwrap(),
            vec![user]
        );

        store.delete(stored.id, user).await.unwrap();
        assert!(
            store
                .user_ids_with_passkeys(&[user])
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn deleting_the_user_cascades_to_the_passkeys(pool: PgPool) {
        let alice = seed_user(&pool, "alice").await;
        let bob = seed_user(&pool, "bob").await;
        let store = store(pool.clone());
        store.insert(&passkey(alice, "a1", &[1])).await.unwrap();
        store.insert(&passkey(alice, "a2", &[2])).await.unwrap();
        store.insert(&passkey(bob, "b1", &[3])).await.unwrap();

        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(alice)
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(count_rows(&pool).await, 1);
        assert_eq!(store.count_for_user(bob).await.unwrap(), 1);
    }
}
