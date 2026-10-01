use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::secret_encryption::SecretEncryptorPort;
use ferrisgit_domain::settings::{
    CiVariable, NewCiVariable, RepositorySettings, RepositorySettingsStorePort,
    RepositorySettingsUpdate,
};
use sqlx::PgPool;
use std::collections::BTreeMap;
use std::sync::Arc;
use uuid::Uuid;

pub struct PostgresRepositorySettingsStore {
    pool: PgPool,
    encryptor: Arc<dyn SecretEncryptorPort>,
}

impl PostgresRepositorySettingsStore {
    pub fn new(pool: PgPool, encryptor: Arc<dyn SecretEncryptorPort>) -> Self {
        Self { pool, encryptor }
    }
}

#[async_trait]
impl RepositorySettingsStorePort for PostgresRepositorySettingsStore {
    async fn get_or_create_default(
        &self,
        repository_id: Uuid,
    ) -> Result<RepositorySettings, DomainError> {
        sqlx::query_as!(
            RepositorySettings,
            "INSERT INTO repository_settings (repository_id) VALUES ($1) ON CONFLICT (repository_id) DO UPDATE SET repository_id = $1 RETURNING repository_id, pipeline_file_path, ci_enabled, required_approvals",
            repository_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))
    }

    async fn update(
        &self,
        repository_id: Uuid,
        update: RepositorySettingsUpdate,
    ) -> Result<RepositorySettings, DomainError> {
        self.get_or_create_default(repository_id).await?;
        sqlx::query_as!(
            RepositorySettings,
            "UPDATE repository_settings SET pipeline_file_path = COALESCE($1, pipeline_file_path), ci_enabled = COALESCE($2, ci_enabled), required_approvals = COALESCE($3, required_approvals) WHERE repository_id = $4 RETURNING repository_id, pipeline_file_path, ci_enabled, required_approvals",
            update.pipeline_file_path,
            update.ci_enabled,
            update.required_approvals,
            repository_id,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))
    }

    async fn list_ci_variables(&self, repository_id: Uuid) -> Result<Vec<CiVariable>, DomainError> {
        sqlx::query_as!(CiVariable, "SELECT id, repository_id, key, masked FROM repository_ci_variables WHERE repository_id = $1 ORDER BY key", repository_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))
    }

    async fn set_ci_variable(
        &self,
        new_variable: NewCiVariable,
    ) -> Result<CiVariable, DomainError> {
        let encrypted_value = self.encryptor.encrypt(&new_variable.plaintext_value)?;
        sqlx::query_as!(
            CiVariable,
            r#"
            INSERT INTO repository_ci_variables (repository_id, key, encrypted_value, masked)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (repository_id, key) DO UPDATE SET encrypted_value = $3, masked = $4
            RETURNING id, repository_id, key, masked
            "#,
            new_variable.repository_id,
            new_variable.key,
            encrypted_value,
            new_variable.masked,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))
    }

    async fn delete_ci_variable(&self, id: Uuid, repository_id: Uuid) -> Result<(), DomainError> {
        let result = sqlx::query!(
            "DELETE FROM repository_ci_variables WHERE id = $1 AND repository_id = $2",
            id,
            repository_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound("ci variable".to_string()));
        }
        Ok(())
    }

    async fn resolve_ci_variables_plaintext(
        &self,
        repository_id: Uuid,
    ) -> Result<BTreeMap<String, String>, DomainError> {
        let rows = sqlx::query!(
            "SELECT key, encrypted_value FROM repository_ci_variables WHERE repository_id = $1",
            repository_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        let mut result = BTreeMap::new();
        for row in rows {
            let value = self.encryptor.decrypt(&row.encrypted_value)?;
            result.insert(row.key, value);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrisgit_domain::repository::{NewRepository, RepositoryStorePort, RepositoryVisibility};
    use ferrisgit_domain::user::{NewUser, UserRepositoryPort};

    struct FakeEncryptor;
    impl SecretEncryptorPort for FakeEncryptor {
        fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>, DomainError> {
            Ok(plaintext.as_bytes().to_vec())
        }
        fn decrypt(&self, ciphertext: &[u8]) -> Result<String, DomainError> {
            Ok(String::from_utf8(ciphertext.to_vec()).unwrap())
        }
    }

    async fn seed_repository(pool: &PgPool) -> Uuid {
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let owner_id = users
            .create(NewUser {
                username: "florian".to_string(),
                email: "f@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
            })
            .await
            .unwrap()
            .id;
        let repos = crate::postgres::repository_store::PostgresRepositoryStore::new(pool.clone());
        repos
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
            .unwrap()
            .id
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn get_or_create_default_returns_sane_defaults(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let store = PostgresRepositorySettingsStore::new(pool, Arc::new(FakeEncryptor));

        let settings = store.get_or_create_default(repository_id).await.unwrap();
        assert_eq!(settings.pipeline_file_path, ".ferrisgit-ci.yml");
        assert!(settings.ci_enabled);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn required_approvals_defaults_to_zero_and_can_be_updated(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let store = PostgresRepositorySettingsStore::new(pool, Arc::new(FakeEncryptor));

        let defaults = store.get_or_create_default(repository_id).await.unwrap();
        assert_eq!(defaults.required_approvals, 0);

        let updated = store
            .update(
                repository_id,
                RepositorySettingsUpdate {
                    required_approvals: Some(2),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.required_approvals, 2);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn setting_a_ci_variable_then_resolving_it_returns_the_plaintext(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let store = PostgresRepositorySettingsStore::new(pool, Arc::new(FakeEncryptor));

        store
            .set_ci_variable(NewCiVariable {
                repository_id,
                key: "API_KEY".to_string(),
                plaintext_value: "secret-123".to_string(),
                masked: true,
            })
            .await
            .unwrap();

        let resolved = store
            .resolve_ci_variables_plaintext(repository_id)
            .await
            .unwrap();
        assert_eq!(resolved.get("API_KEY"), Some(&"secret-123".to_string()));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn listing_ci_variables_never_exposes_the_value(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let store = PostgresRepositorySettingsStore::new(pool, Arc::new(FakeEncryptor));
        store
            .set_ci_variable(NewCiVariable {
                repository_id,
                key: "API_KEY".to_string(),
                plaintext_value: "secret-123".to_string(),
                masked: true,
            })
            .await
            .unwrap();

        let listed = store.list_ci_variables(repository_id).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].key, "API_KEY");
        // `CiVariable` has no value field at all, so this holds at compile time, not just at runtime.
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn setting_the_same_key_twice_overwrites_the_value(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let store = PostgresRepositorySettingsStore::new(pool, Arc::new(FakeEncryptor));
        store
            .set_ci_variable(NewCiVariable {
                repository_id,
                key: "API_KEY".to_string(),
                plaintext_value: "old".to_string(),
                masked: true,
            })
            .await
            .unwrap();
        store
            .set_ci_variable(NewCiVariable {
                repository_id,
                key: "API_KEY".to_string(),
                plaintext_value: "new".to_string(),
                masked: true,
            })
            .await
            .unwrap();

        let resolved = store
            .resolve_ci_variables_plaintext(repository_id)
            .await
            .unwrap();
        assert_eq!(resolved.get("API_KEY"), Some(&"new".to_string()));
    }
}
