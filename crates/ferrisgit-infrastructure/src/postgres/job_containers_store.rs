use crate::error::infra;
use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job_containers::{DockerInJobsPort, JobCloneTokenPort};
use sqlx::PgPool;
use uuid::Uuid;

/// The Docker switch lives on the `system_settings` singleton row with runtime queries of its own, like the registration
/// switch, so `PostgresSystemSettingsStore` and its compile-time checked queries stay untouched.
pub struct PostgresDockerInJobsStore {
    pool: PgPool,
}

impl PostgresDockerInJobsStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DockerInJobsPort for PostgresDockerInJobsStore {
    async fn is_enabled(&self) -> Result<bool, DomainError> {
        let enabled: Option<bool> =
            sqlx::query_scalar("SELECT docker_in_jobs FROM system_settings WHERE id = true")
                .fetch_optional(&self.pool)
                .await
                .map_err(infra)?;
        // No row yet means the default: off.
        Ok(enabled.unwrap_or(false))
    }

    async fn set_enabled(&self, enabled: bool) -> Result<(), DomainError> {
        sqlx::query("INSERT INTO system_settings (id, docker_in_jobs) VALUES (true, $1) ON CONFLICT (id) DO UPDATE SET docker_in_jobs = EXCLUDED.docker_in_jobs")
            .bind(enabled)
            .execute(&self.pool)
            .await
            .map_err(infra)?;
        Ok(())
    }
}

pub struct PostgresJobCloneTokenStore {
    pool: PgPool,
}

impl PostgresJobCloneTokenStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl JobCloneTokenPort for PostgresJobCloneTokenStore {
    async fn issue(&self, job_id: Uuid, token_hash: &str) -> Result<(), DomainError> {
        // A job submitted again (after a failed Pod creation) gets a new token: the old one is replaced.
        sqlx::query("INSERT INTO job_clone_tokens (job_id, token_hash) VALUES ($1, $2) ON CONFLICT (job_id) DO UPDATE SET token_hash = EXCLUDED.token_hash, created_at = now()")
            .bind(job_id)
            .bind(token_hash)
            .execute(&self.pool)
            .await
            .map_err(infra)?;
        Ok(())
    }

    async fn repository_of_unfinished_job(
        &self,
        token_hash: &str,
    ) -> Result<Option<Uuid>, DomainError> {
        sqlx::query_scalar(
            "SELECT p.repository_id FROM job_clone_tokens t \
             JOIN jobs j ON j.id = t.job_id \
             JOIN pipelines p ON p.id = j.pipeline_id \
             WHERE t.token_hash = $1 AND j.status IN ('pending', 'running')",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::system_settings_store::PostgresSystemSettingsStore;
    use crate::postgres::test_support::seed_owned_repository;
    use ferrisgit_domain::settings::{SystemSettingsStorePort, SystemSettingsUpdate};

    #[sqlx::test(migrations = "../../migrations")]
    async fn docker_in_jobs_is_off_until_turned_on_and_leaves_the_other_settings_alone(
        pool: PgPool,
    ) {
        let system = PostgresSystemSettingsStore::new(pool.clone());
        let docker = PostgresDockerInJobsStore::new(pool);
        assert!(!docker.is_enabled().await.unwrap());

        docker.set_enabled(true).await.unwrap();
        let changed = system
            .update(SystemSettingsUpdate {
                jwt_ttl_hours: Some(48),
                ..Default::default()
            })
            .await
            .unwrap();

        assert!(
            docker.is_enabled().await.unwrap(),
            "a settings update keeps it"
        );
        docker.set_enabled(false).await.unwrap();
        assert!(!docker.is_enabled().await.unwrap());
        assert_eq!(
            system.get().await.unwrap().jwt_ttl_hours,
            changed.jwt_ttl_hours
        );
    }

    async fn seed_job(pool: &PgPool, status: &str) -> (Uuid, Uuid) {
        let (owner_id, repository_id) = seed_owned_repository(pool, "alice").await;
        let pipeline_id: Uuid = sqlx::query_scalar(
            "INSERT INTO pipelines (repository_id, commit_sha, triggered_by) VALUES ($1, 'abc', $2) RETURNING id",
        )
        .bind(repository_id)
        .bind(owner_id)
        .fetch_one(pool)
        .await
        .unwrap();
        let job_id = sqlx::query_scalar(
            "INSERT INTO jobs (pipeline_id, stage, name, image, script, status) VALUES ($1, 'test', 'unit', 'alpine', '[]', $2) RETURNING id",
        )
        .bind(pipeline_id)
        .bind(status)
        .fetch_one(pool)
        .await
        .unwrap();
        (job_id, repository_id)
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_token_reads_its_jobs_repository_while_the_job_has_not_finished(pool: PgPool) {
        let store = PostgresJobCloneTokenStore::new(pool.clone());
        let (job_id, repository_id) = seed_job(&pool, "running").await;
        store.issue(job_id, "hash-1").await.unwrap();

        assert_eq!(
            store.repository_of_unfinished_job("hash-1").await.unwrap(),
            Some(repository_id)
        );
        assert_eq!(
            store.repository_of_unfinished_job("other").await.unwrap(),
            None
        );

        sqlx::query("UPDATE jobs SET status = 'success', finished_at = now() WHERE id = $1")
            .bind(job_id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            store.repository_of_unfinished_job("hash-1").await.unwrap(),
            None,
            "a finished job's token reads nothing"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn issuing_again_replaces_the_previous_token(pool: PgPool) {
        let store = PostgresJobCloneTokenStore::new(pool.clone());
        let (job_id, _) = seed_job(&pool, "pending").await;

        store.issue(job_id, "first").await.unwrap();
        store.issue(job_id, "second").await.unwrap();

        assert_eq!(
            store.repository_of_unfinished_job("first").await.unwrap(),
            None
        );
        assert!(
            store
                .repository_of_unfinished_job("second")
                .await
                .unwrap()
                .is_some()
        );
    }
}
