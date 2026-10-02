use crate::error::infra;
use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::registration::RegistrationSettingsPort;
use sqlx::PgPool;

/// The registration switch lives on the `system_settings` singleton row, but is read and written with its own
/// runtime queries so `PostgresSystemSettingsStore` (and its compile-time checked queries) stay untouched.
pub struct PostgresRegistrationSettingsStore {
    pool: PgPool,
}

impl PostgresRegistrationSettingsStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RegistrationSettingsPort for PostgresRegistrationSettingsStore {
    async fn is_enabled(&self) -> Result<bool, DomainError> {
        let enabled: Option<bool> =
            sqlx::query_scalar("SELECT registration_enabled FROM system_settings WHERE id = true")
                .fetch_optional(&self.pool)
                .await
                .map_err(infra)?;
        // The singleton row is created lazily (by the first settings read or write): no row means the default, off.
        Ok(enabled.unwrap_or(false))
    }

    async fn set_enabled(&self, enabled: bool) -> Result<(), DomainError> {
        // Upsert of the singleton: creates it with the column defaults when missing, and only ever touches this column.
        sqlx::query("INSERT INTO system_settings (id, registration_enabled) VALUES (true, $1) ON CONFLICT (id) DO UPDATE SET registration_enabled = EXCLUDED.registration_enabled")
            .bind(enabled)
            .execute(&self.pool)
            .await
            .map_err(infra)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::system_settings_store::PostgresSystemSettingsStore;
    use ferrisgit_domain::settings::{
        ExecutionEngine, SystemSettingsStorePort, SystemSettingsUpdate,
    };

    #[sqlx::test(migrations = "../../migrations")]
    async fn registration_is_disabled_when_the_settings_row_does_not_exist_yet(pool: PgPool) {
        let store = PostgresRegistrationSettingsStore::new(pool.clone());

        assert!(!store.is_enabled().await.unwrap());
        let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM system_settings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rows, 0, "reading must not create the row");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_settings_row_created_by_the_system_settings_store_has_registration_disabled(
        pool: PgPool,
    ) {
        PostgresSystemSettingsStore::new(pool.clone())
            .get()
            .await
            .unwrap();

        assert!(
            !PostgresRegistrationSettingsStore::new(pool)
                .is_enabled()
                .await
                .unwrap()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn enabling_creates_the_singleton_row_when_it_is_missing(pool: PgPool) {
        let store = PostgresRegistrationSettingsStore::new(pool.clone());

        store.set_enabled(true).await.unwrap();

        assert!(store.is_enabled().await.unwrap());
        let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM system_settings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rows, 1);
        // The row created this way carries the column defaults, so the system settings store reads it normally.
        let settings = PostgresSystemSettingsStore::new(pool).get().await.unwrap();
        assert_eq!(settings.execution_engine, ExecutionEngine::DockerRunners);
        assert_eq!(settings.jwt_ttl_hours, 12);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn toggling_registration_does_not_disturb_the_other_settings(pool: PgPool) {
        let system = PostgresSystemSettingsStore::new(pool.clone());
        let registration = PostgresRegistrationSettingsStore::new(pool);
        system.get().await.unwrap();
        let changed = system
            .update(SystemSettingsUpdate {
                execution_engine: Some(ExecutionEngine::Kubernetes),
                k8s_namespace: Some(Some("ci".to_string())),
                jwt_ttl_hours: Some(48),
                ..Default::default()
            })
            .await
            .unwrap();

        registration.set_enabled(true).await.unwrap();
        assert!(registration.is_enabled().await.unwrap());
        assert_eq!(
            format!("{:?}", system.get().await.unwrap()),
            format!("{changed:?}")
        );

        registration.set_enabled(false).await.unwrap();
        assert!(!registration.is_enabled().await.unwrap());
        assert_eq!(
            format!("{:?}", system.get().await.unwrap()),
            format!("{changed:?}")
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn updating_the_system_settings_does_not_reset_the_registration_switch(pool: PgPool) {
        let system = PostgresSystemSettingsStore::new(pool.clone());
        let registration = PostgresRegistrationSettingsStore::new(pool);
        registration.set_enabled(true).await.unwrap();

        system
            .update(SystemSettingsUpdate {
                jwt_ttl_hours: Some(24),
                ..Default::default()
            })
            .await
            .unwrap();

        assert!(registration.is_enabled().await.unwrap());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn setting_the_same_value_twice_is_idempotent(pool: PgPool) {
        let store = PostgresRegistrationSettingsStore::new(pool.clone());

        store.set_enabled(true).await.unwrap();
        store.set_enabled(true).await.unwrap();

        assert!(store.is_enabled().await.unwrap());
        let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM system_settings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rows, 1);
    }
}
