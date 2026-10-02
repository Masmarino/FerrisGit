use crate::error::infra;
use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::public_pages::{
    PublicPagesSettings, PublicPagesSettingsPort, PublicPagesSettingsUpdate,
};
use sqlx::PgPool;

/// Like the registration switch, these columns of the `system_settings` singleton use their own runtime queries so
/// `PostgresSystemSettingsStore` and its compile-time checked queries stay untouched.
pub struct PostgresPublicPagesSettingsStore {
    pool: PgPool,
}

impl PostgresPublicPagesSettingsStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PublicPagesSettingsPort for PostgresPublicPagesSettingsStore {
    async fn get(&self) -> Result<PublicPagesSettings, DomainError> {
        let row: Option<(bool, bool)> = sqlx::query_as(
            "SELECT public_pages_enabled, seo_indexing_enabled FROM system_settings WHERE id = true",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;
        // The singleton row is created lazily: no row means the column defaults.
        Ok(row.map_or_else(
            PublicPagesSettings::default,
            |(public_pages_enabled, seo_indexing_enabled)| PublicPagesSettings {
                public_pages_enabled,
                seo_indexing_enabled,
            },
        ))
    }

    async fn update(
        &self,
        update: PublicPagesSettingsUpdate,
    ) -> Result<PublicPagesSettings, DomainError> {
        let mut tx = self.pool.begin().await.map_err(infra)?;
        sqlx::query("INSERT INTO system_settings (id) VALUES (true) ON CONFLICT (id) DO NOTHING")
            .execute(&mut *tx)
            .await
            .map_err(infra)?;
        let (public_pages_enabled, seo_indexing_enabled): (bool, bool) = sqlx::query_as(
            "UPDATE system_settings \
             SET public_pages_enabled = COALESCE($1, public_pages_enabled), \
                 seo_indexing_enabled = COALESCE($2, seo_indexing_enabled) \
             WHERE id = true \
             RETURNING public_pages_enabled, seo_indexing_enabled",
        )
        .bind(update.public_pages_enabled)
        .bind(update.seo_indexing_enabled)
        .fetch_one(&mut *tx)
        .await
        .map_err(infra)?;
        tx.commit().await.map_err(infra)?;
        Ok(PublicPagesSettings {
            public_pages_enabled,
            seo_indexing_enabled,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::registration_settings_store::PostgresRegistrationSettingsStore;
    use crate::postgres::system_settings_store::PostgresSystemSettingsStore;
    use ferrisgit_domain::registration::RegistrationSettingsPort;
    use ferrisgit_domain::settings::{
        ExecutionEngine, SystemSettingsStorePort, SystemSettingsUpdate,
    };

    async fn row_count(pool: &PgPool) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM system_settings")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn defaults_apply_before_the_settings_row_exists_and_reading_creates_nothing(
        pool: PgPool,
    ) {
        let store = PostgresPublicPagesSettingsStore::new(pool.clone());

        assert_eq!(store.get().await.unwrap(), PublicPagesSettings::default());
        assert_eq!(row_count(&pool).await, 0);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_row_created_elsewhere_carries_the_same_defaults(pool: PgPool) {
        PostgresSystemSettingsStore::new(pool.clone())
            .get()
            .await
            .unwrap();

        let settings = PostgresPublicPagesSettingsStore::new(pool)
            .get()
            .await
            .unwrap();

        assert_eq!(settings, PublicPagesSettings::default());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn an_update_creates_the_singleton_row_and_only_changes_what_it_names(pool: PgPool) {
        let store = PostgresPublicPagesSettingsStore::new(pool.clone());

        let updated = store
            .update(PublicPagesSettingsUpdate {
                seo_indexing_enabled: Some(true),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(
            updated,
            PublicPagesSettings {
                public_pages_enabled: true,
                seo_indexing_enabled: true,
            }
        );
        assert_eq!(store.get().await.unwrap(), updated);
        assert_eq!(row_count(&pool).await, 1);

        let updated = store
            .update(PublicPagesSettingsUpdate {
                public_pages_enabled: Some(false),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(
            updated,
            PublicPagesSettings {
                public_pages_enabled: false,
                seo_indexing_enabled: true,
            }
        );
        let settings = PostgresSystemSettingsStore::new(pool).get().await.unwrap();
        assert_eq!(settings.execution_engine, ExecutionEngine::DockerRunners);
        assert_eq!(settings.jwt_ttl_hours, 12);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn the_switches_and_the_other_settings_do_not_disturb_each_other(pool: PgPool) {
        let system = PostgresSystemSettingsStore::new(pool.clone());
        let registration = PostgresRegistrationSettingsStore::new(pool.clone());
        let public_pages = PostgresPublicPagesSettingsStore::new(pool);
        system.get().await.unwrap();
        let changed = system
            .update(SystemSettingsUpdate {
                jwt_ttl_hours: Some(48),
                ..Default::default()
            })
            .await
            .unwrap();
        registration.set_enabled(true).await.unwrap();

        public_pages
            .update(PublicPagesSettingsUpdate {
                public_pages_enabled: Some(false),
                seo_indexing_enabled: Some(true),
            })
            .await
            .unwrap();

        assert_eq!(
            format!("{:?}", system.get().await.unwrap()),
            format!("{changed:?}")
        );
        assert!(registration.is_enabled().await.unwrap());

        system
            .update(SystemSettingsUpdate {
                jwt_ttl_hours: Some(24),
                ..Default::default()
            })
            .await
            .unwrap();
        registration.set_enabled(false).await.unwrap();
        assert_eq!(
            public_pages.get().await.unwrap(),
            PublicPagesSettings {
                public_pages_enabled: false,
                seo_indexing_enabled: true,
            }
        );
    }
}
