use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::settings::{
    ExecutionEngine, SystemSettings, SystemSettingsStorePort, SystemSettingsUpdate,
};
use sqlx::PgPool;

pub struct PostgresSystemSettingsStore {
    pool: PgPool,
}

impl PostgresSystemSettingsStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct Row {
    execution_engine: String,
    k8s_namespace: Option<String>,
    k8s_cache_storage_class: Option<String>,
    runner_registration_token: Option<String>,
    log_retention_days: Option<i32>,
    max_concurrent_jobs: Option<i32>,
    jwt_ttl_hours: i32,
    max_push_size_mb: i32,
}

impl Row {
    fn into_domain(self) -> Result<SystemSettings, DomainError> {
        Ok(SystemSettings {
            execution_engine: ExecutionEngine::parse(&self.execution_engine)?,
            k8s_namespace: self.k8s_namespace,
            k8s_cache_storage_class: self.k8s_cache_storage_class,
            runner_registration_token: self.runner_registration_token,
            log_retention_days: self.log_retention_days,
            max_concurrent_jobs: self.max_concurrent_jobs,
            jwt_ttl_hours: self.jwt_ttl_hours,
            max_push_size_mb: self.max_push_size_mb,
        })
    }
}

#[async_trait]
impl SystemSettingsStorePort for PostgresSystemSettingsStore {
    /// The single row is not seeded by the migration, so `get()` upserts on read. `DO UPDATE SET id = true`
    /// is a no-op write that exists only so `RETURNING` yields the existing row.
    async fn get(&self) -> Result<SystemSettings, DomainError> {
        let row = sqlx::query_as!(
            Row,
            "INSERT INTO system_settings (id) VALUES (true) ON CONFLICT (id) DO UPDATE SET id = true RETURNING execution_engine, k8s_namespace, k8s_cache_storage_class, runner_registration_token, log_retention_days, max_concurrent_jobs, jwt_ttl_hours, max_push_size_mb"
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        row.into_domain()
    }

    /// Singly-optional fields use `COALESCE($n, column)`, so `None` leaves the column unchanged.
    /// Doubly-optional fields (`Some(None)` clears to NULL) can't use COALESCE, which can't tell
    /// "unchanged" from "clear". Each one gets an `is_some()` flag plus the flattened value instead:
    /// `CASE WHEN $flag THEN $value ELSE column END`.
    async fn update(&self, update: SystemSettingsUpdate) -> Result<SystemSettings, DomainError> {
        self.get().await?;
        let row = sqlx::query_as!(
            Row,
            r#"
            UPDATE system_settings SET
                execution_engine = COALESCE($1, execution_engine),
                k8s_namespace = CASE WHEN $2 THEN $3 ELSE k8s_namespace END,
                k8s_cache_storage_class = CASE WHEN $4 THEN $5 ELSE k8s_cache_storage_class END,
                runner_registration_token = CASE WHEN $6 THEN $7 ELSE runner_registration_token END,
                log_retention_days = CASE WHEN $8 THEN $9 ELSE log_retention_days END,
                max_concurrent_jobs = CASE WHEN $10 THEN $11 ELSE max_concurrent_jobs END,
                jwt_ttl_hours = COALESCE($12, jwt_ttl_hours),
                max_push_size_mb = COALESCE($13, max_push_size_mb)
            WHERE id = true
            RETURNING execution_engine, k8s_namespace, k8s_cache_storage_class, runner_registration_token, log_retention_days, max_concurrent_jobs, jwt_ttl_hours, max_push_size_mb
            "#,
            update.execution_engine.map(|e| e.as_str().to_string()),
            update.k8s_namespace.is_some(),
            update.k8s_namespace.flatten(),
            update.k8s_cache_storage_class.is_some(),
            update.k8s_cache_storage_class.flatten(),
            update.runner_registration_token.is_some(),
            update.runner_registration_token.flatten(),
            update.log_retention_days.is_some(),
            update.log_retention_days.flatten(),
            update.max_concurrent_jobs.is_some(),
            update.max_concurrent_jobs.flatten(),
            update.jwt_ttl_hours,
            update.max_push_size_mb,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        row.into_domain()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "../../migrations")]
    async fn get_creates_and_returns_default_settings_on_first_call(pool: PgPool) {
        let store = PostgresSystemSettingsStore::new(pool);
        let settings = store.get().await.unwrap();
        assert_eq!(settings.execution_engine, ExecutionEngine::DockerRunners);
        assert_eq!(settings.jwt_ttl_hours, 12);
        assert_eq!(settings.max_push_size_mb, 500);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn get_is_idempotent_and_does_not_reset_prior_updates(pool: PgPool) {
        let store = PostgresSystemSettingsStore::new(pool);
        store.get().await.unwrap();
        store
            .update(SystemSettingsUpdate {
                jwt_ttl_hours: Some(24),
                ..Default::default()
            })
            .await
            .unwrap();

        let settings = store.get().await.unwrap();
        assert_eq!(
            settings.jwt_ttl_hours, 24,
            "a second get() must not reset the row back to defaults"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn update_changes_only_the_fields_that_were_set(pool: PgPool) {
        let store = PostgresSystemSettingsStore::new(pool);
        store
            .update(SystemSettingsUpdate {
                execution_engine: Some(ExecutionEngine::Kubernetes),
                k8s_namespace: Some(Some("ci".to_string())),
                ..Default::default()
            })
            .await
            .unwrap();

        let settings = store
            .update(SystemSettingsUpdate {
                jwt_ttl_hours: Some(48),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(
            settings.execution_engine,
            ExecutionEngine::Kubernetes,
            "an unrelated update must not reset execution_engine"
        );
        assert_eq!(
            settings.k8s_namespace,
            Some("ci".to_string()),
            "an unrelated update must not reset k8s_namespace"
        );
        assert_eq!(settings.jwt_ttl_hours, 48);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn update_can_clear_an_optional_field_back_to_null(pool: PgPool) {
        let store = PostgresSystemSettingsStore::new(pool);
        store
            .update(SystemSettingsUpdate {
                k8s_namespace: Some(Some("ci".to_string())),
                ..Default::default()
            })
            .await
            .unwrap();

        let settings = store
            .update(SystemSettingsUpdate {
                k8s_namespace: Some(None),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(
            settings.k8s_namespace, None,
            "Some(None) must clear the field to NULL, not leave it unchanged"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn update_distinguishes_leave_unchanged_set_and_clear_for_a_doubly_optional_field(
        pool: PgPool,
    ) {
        let store = PostgresSystemSettingsStore::new(pool);

        let settings = store
            .update(SystemSettingsUpdate {
                jwt_ttl_hours: Some(1),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(
            settings.k8s_cache_storage_class, None,
            "an update that never mentions the field must leave it as the default"
        );

        let settings = store
            .update(SystemSettingsUpdate {
                k8s_cache_storage_class: Some(Some("fast-ssd".to_string())),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(
            settings.k8s_cache_storage_class,
            Some("fast-ssd".to_string())
        );

        let settings = store
            .update(SystemSettingsUpdate {
                jwt_ttl_hours: Some(2),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(
            settings.k8s_cache_storage_class,
            Some("fast-ssd".to_string()),
            "an unrelated update must not clear a previously-set value"
        );

        let settings = store
            .update(SystemSettingsUpdate {
                k8s_cache_storage_class: Some(None),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(
            settings.k8s_cache_storage_class, None,
            "Some(None) must clear a previously-set value to NULL"
        );
    }
}
