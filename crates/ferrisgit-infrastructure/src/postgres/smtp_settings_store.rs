use crate::error::infra;
use async_trait::async_trait;
use ferrisgit_domain::email::{SmtpSecurity, SmtpSettings, SmtpSettingsPort};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::secret_encryption::SecretEncryptorPort;
use sqlx::PgPool;
use std::sync::Arc;

pub struct PostgresSmtpSettingsStore {
    pool: PgPool,
    encryptor: Arc<dyn SecretEncryptorPort>,
}

impl PostgresSmtpSettingsStore {
    pub fn new(pool: PgPool, encryptor: Arc<dyn SecretEncryptorPort>) -> Self {
        Self { pool, encryptor }
    }
}

#[derive(sqlx::FromRow)]
struct Row {
    host: String,
    port: i32,
    security: String,
    username: String,
    encrypted_password: Option<Vec<u8>>,
    from_address: String,
    from_name: String,
}

#[async_trait]
impl SmtpSettingsPort for PostgresSmtpSettingsStore {
    async fn get(&self) -> Result<Option<SmtpSettings>, DomainError> {
        let row = sqlx::query_as::<_, Row>("SELECT host, port, security, username, encrypted_password, from_address, from_name FROM smtp_settings WHERE id = true")
            .fetch_optional(&self.pool)
            .await
            .map_err(infra)?;
        let Some(row) = row else { return Ok(None) };
        // A row that can't be decrypted (dump restored under another key, or a rotated key) shouldn't lock the admin
        // out of the settings page: report "no password" so they're asked for it again.
        let password = match &row.encrypted_password {
            Some(ciphertext) => match self.encryptor.decrypt(ciphertext) {
                Ok(password) => Some(password),
                Err(error) => {
                    tracing::warn!(%error, "the stored SMTP password cannot be decrypted (was SETTINGS_ENCRYPTION_KEY changed?); it must be entered again");
                    None
                }
            },
            None => None,
        };
        let port = u16::try_from(row.port).map_err(|e| {
            DomainError::Infrastructure(format!("stored SMTP port is out of range: {e}"))
        })?;
        Ok(Some(SmtpSettings {
            host: row.host,
            port,
            security: SmtpSecurity::parse(&row.security)?,
            username: row.username,
            password,
            from_address: row.from_address,
            from_name: row.from_name,
        }))
    }

    async fn save(&self, settings: &SmtpSettings) -> Result<(), DomainError> {
        let encrypted_password = match &settings.password {
            Some(password) => Some(self.encryptor.encrypt(password)?),
            None => None,
        };
        sqlx::query(
            "INSERT INTO smtp_settings (id, host, port, security, username, encrypted_password, from_address, from_name) \
             VALUES (true, $1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (id) DO UPDATE SET host = EXCLUDED.host, port = EXCLUDED.port, security = EXCLUDED.security, \
             username = EXCLUDED.username, encrypted_password = EXCLUDED.encrypted_password, \
             from_address = EXCLUDED.from_address, from_name = EXCLUDED.from_name, updated_at = now()",
        )
        .bind(&settings.host)
        .bind(i32::from(settings.port))
        .bind(settings.security.as_str())
        .bind(&settings.username)
        .bind(encrypted_password)
        .bind(&settings.from_address)
        .bind(&settings.from_name)
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aes_gcm_encryptor::AesGcmSecretEncryptor;

    fn store(pool: PgPool) -> PostgresSmtpSettingsStore {
        PostgresSmtpSettingsStore::new(pool, Arc::new(AesGcmSecretEncryptor::new(&[7u8; 32])))
    }

    fn sample() -> SmtpSettings {
        SmtpSettings {
            host: "smtp.example.com".into(),
            port: 587,
            security: SmtpSecurity::StartTls,
            username: "mailer".into(),
            password: Some("smtp-password-value".into()),
            from_address: "noreply@example.com".into(),
            from_name: "FerrisGit".into(),
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn get_on_an_empty_table_is_none(pool: PgPool) {
        assert!(store(pool).get().await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn saving_then_getting_round_trips_every_field(pool: PgPool) {
        let store = store(pool);
        store.save(&sample()).await.unwrap();

        assert_eq!(store.get().await.unwrap(), Some(sample()));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn the_password_is_stored_encrypted(pool: PgPool) {
        let store = store(pool.clone());
        store.save(&sample()).await.unwrap();

        let stored: Vec<u8> = sqlx::query_scalar("SELECT encrypted_password FROM smtp_settings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_ne!(stored, b"smtp-password-value".to_vec());
        assert!(
            stored.len() > 12,
            "expected nonce + ciphertext, got {} bytes",
            stored.len()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_second_save_overwrites_the_single_row(pool: PgPool) {
        let store = store(pool.clone());
        store.save(&sample()).await.unwrap();
        let changed = SmtpSettings {
            host: "mail.other.test".into(),
            port: 465,
            security: SmtpSecurity::Tls,
            ..sample()
        };
        store.save(&changed).await.unwrap();

        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM smtp_settings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(store.get().await.unwrap(), Some(changed));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn saving_without_a_password_stores_null(pool: PgPool) {
        let store = store(pool.clone());
        let settings = SmtpSettings {
            username: String::new(),
            password: None,
            security: SmtpSecurity::None,
            port: 25,
            ..sample()
        };
        store.save(&settings).await.unwrap();

        let stored: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT encrypted_password FROM smtp_settings")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(stored.is_none());
        assert_eq!(store.get().await.unwrap(), Some(settings));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn clearing_the_password_on_a_later_save_removes_it(pool: PgPool) {
        let store = store(pool);
        store.save(&sample()).await.unwrap();
        store
            .save(&SmtpSettings {
                password: None,
                ..sample()
            })
            .await
            .unwrap();

        assert_eq!(store.get().await.unwrap().unwrap().password, None);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_password_encrypted_under_another_key_reads_back_as_unset(pool: PgPool) {
        store(pool.clone()).save(&sample()).await.unwrap();
        let other_key =
            PostgresSmtpSettingsStore::new(pool, Arc::new(AesGcmSecretEncryptor::new(&[9u8; 32])));

        let read = other_key
            .get()
            .await
            .unwrap()
            .expect("the row itself is still readable");

        assert_eq!(read.password, None);
        assert_eq!(
            read,
            SmtpSettings {
                password: None,
                ..sample()
            }
        );
    }
}
