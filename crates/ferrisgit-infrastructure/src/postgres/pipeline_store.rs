use crate::error::infra;
use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::pipeline::{NewPipeline, Pipeline, PipelineStatus, PipelineStorePort};
use ferrisgit_domain::settings::ExecutionEngine;
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresPipelineStore {
    pool: PgPool,
}

impl PostgresPipelineStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

// Runtime queries: SELECT * would otherwise need every new column in the offline cache.
#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    repository_id: Uuid,
    commit_sha: String,
    execution_engine: String,
    status: String,
    triggered_by: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    finished_at: Option<chrono::DateTime<chrono::Utc>>,
    error: Option<String>,
}

impl TryFrom<Row> for Pipeline {
    type Error = DomainError;

    fn try_from(row: Row) -> Result<Self, DomainError> {
        Ok(Pipeline {
            id: row.id,
            repository_id: row.repository_id,
            commit_sha: row.commit_sha,
            execution_engine: ExecutionEngine::parse(&row.execution_engine)?,
            status: PipelineStatus::parse(&row.status)?,
            triggered_by: row.triggered_by,
            created_at: row.created_at,
            finished_at: row.finished_at,
            error: row.error,
        })
    }
}

#[async_trait]
impl PipelineStorePort for PostgresPipelineStore {
    async fn create(&self, new_pipeline: NewPipeline) -> Result<Pipeline, DomainError> {
        let row: Row = sqlx::query_as(
            "INSERT INTO pipelines (repository_id, commit_sha, execution_engine, triggered_by) VALUES ($1, $2, $3, $4) RETURNING *",
        )
        .bind(new_pipeline.repository_id)
        .bind(new_pipeline.commit_sha)
        .bind(new_pipeline.execution_engine.as_str())
        .bind(new_pipeline.triggered_by)
        .fetch_one(&self.pool)
        .await
        .map_err(infra)?;
        Pipeline::try_from(row)
    }

    async fn create_failed(
        &self,
        new_pipeline: NewPipeline,
        error: &str,
    ) -> Result<Pipeline, DomainError> {
        let row: Row = sqlx::query_as(
            "INSERT INTO pipelines (repository_id, commit_sha, execution_engine, triggered_by, status, finished_at, error) VALUES ($1, $2, $3, $4, 'failed', now(), $5) RETURNING *",
        )
        .bind(new_pipeline.repository_id)
        .bind(new_pipeline.commit_sha)
        .bind(new_pipeline.execution_engine.as_str())
        .bind(new_pipeline.triggered_by)
        .bind(error)
        .fetch_one(&self.pool)
        .await
        .map_err(infra)?;
        Pipeline::try_from(row)
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Pipeline>, DomainError> {
        let row: Option<Row> = sqlx::query_as("SELECT * FROM pipelines WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(infra)?;
        row.map(Pipeline::try_from).transpose()
    }

    async fn list_for_repository(&self, repository_id: Uuid) -> Result<Vec<Pipeline>, DomainError> {
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT * FROM pipelines WHERE repository_id = $1 ORDER BY created_at DESC",
        )
        .bind(repository_id)
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        rows.into_iter().map(Pipeline::try_from).collect()
    }

    async fn update_status(&self, id: Uuid, status: PipelineStatus) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE pipelines SET status = $1::text, \
                 finished_at = CASE WHEN $1::text IN ('success', 'failed', 'canceled') THEN COALESCE(finished_at, now()) ELSE finished_at END \
             WHERE id = $2",
        )
        .bind(status.as_str())
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }

    async fn mark_running(&self, id: Uuid) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "UPDATE pipelines SET status = 'running' WHERE id = $1 AND status = 'pending'",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(result.rows_affected() > 0)
    }

    async fn count_created_since(
        &self,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<i64, DomainError> {
        sqlx::query_scalar!(
            "SELECT COUNT(*) FROM pipelines WHERE created_at >= $1",
            since
        )
        .fetch_one(&self.pool)
        .await
        .map(|count| count.unwrap_or(0))
        .map_err(infra)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::test_support::seed_owned_repository;
    use ferrisgit_domain::user::{NewUser, UserRepositoryPort};

    async fn seed_repository(pool: &PgPool) -> (Uuid, Uuid) {
        seed_owned_repository(pool, "florian").await
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn creating_then_finding_a_pipeline_returns_it(pool: PgPool) {
        let (owner_id, repository_id) = seed_repository(&pool).await;
        let store = PostgresPipelineStore::new(pool);
        let created = store
            .create(NewPipeline {
                repository_id,
                commit_sha: "abc123".to_string(),
                execution_engine: ExecutionEngine::DockerRunners,
                triggered_by: owner_id,
            })
            .await
            .unwrap();

        let found = store.find_by_id(created.id).await.unwrap().unwrap();
        assert_eq!(found.commit_sha, "abc123");
        assert_eq!(found.status, PipelineStatus::Pending);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn update_status_persists(pool: PgPool) {
        let (owner_id, repository_id) = seed_repository(&pool).await;
        let store = PostgresPipelineStore::new(pool);
        let created = store
            .create(NewPipeline {
                repository_id,
                commit_sha: "abc123".to_string(),
                execution_engine: ExecutionEngine::DockerRunners,
                triggered_by: owner_id,
            })
            .await
            .unwrap();

        store
            .update_status(created.id, PipelineStatus::Running)
            .await
            .unwrap();

        let found = store.find_by_id(created.id).await.unwrap().unwrap();
        assert_eq!(found.status, PipelineStatus::Running);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_terminal_status_stamps_finished_at_and_a_non_terminal_one_does_not(pool: PgPool) {
        let (owner_id, repository_id) = seed_repository(&pool).await;
        let store = PostgresPipelineStore::new(pool);
        let created = store
            .create(NewPipeline {
                repository_id,
                commit_sha: "abc123".to_string(),
                execution_engine: ExecutionEngine::DockerRunners,
                triggered_by: owner_id,
            })
            .await
            .unwrap();

        store
            .update_status(created.id, PipelineStatus::Running)
            .await
            .unwrap();
        assert!(
            store
                .find_by_id(created.id)
                .await
                .unwrap()
                .unwrap()
                .finished_at
                .is_none()
        );

        store
            .update_status(created.id, PipelineStatus::Success)
            .await
            .unwrap();
        assert!(
            store
                .find_by_id(created.id)
                .await
                .unwrap()
                .unwrap()
                .finished_at
                .is_some()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_for_repository_returns_only_that_repositorys_pipelines(pool: PgPool) {
        let (owner_id, repository_id) = seed_repository(&pool).await;
        let store = PostgresPipelineStore::new(pool);
        store
            .create(NewPipeline {
                repository_id,
                commit_sha: "a".to_string(),
                execution_engine: ExecutionEngine::DockerRunners,
                triggered_by: owner_id,
            })
            .await
            .unwrap();
        store
            .create(NewPipeline {
                repository_id,
                commit_sha: "b".to_string(),
                execution_engine: ExecutionEngine::DockerRunners,
                triggered_by: owner_id,
            })
            .await
            .unwrap();

        let pipelines = store.list_for_repository(repository_id).await.unwrap();
        assert_eq!(pipelines.len(), 2);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn creating_a_pipeline_persists_and_returns_its_execution_engine(pool: PgPool) {
        let (owner_id, repository_id) = seed_repository(&pool).await;
        let store = PostgresPipelineStore::new(pool);

        let created = store
            .create(NewPipeline {
                repository_id,
                commit_sha: "abc".to_string(),
                execution_engine: ExecutionEngine::Kubernetes,
                triggered_by: owner_id,
            })
            .await
            .unwrap();
        assert_eq!(created.execution_engine, ExecutionEngine::Kubernetes);

        let found = store.find_by_id(created.id).await.unwrap().unwrap();
        assert_eq!(
            found.execution_engine,
            ExecutionEngine::Kubernetes,
            "execution_engine must round-trip through the database, not just the in-memory return value"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn triggered_by_round_trips_through_the_database(pool: PgPool) {
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let pusher_id = users
            .create(NewUser {
                username: "pusher".to_string(),
                email: "p@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
            })
            .await
            .unwrap()
            .id;
        let (_, repository_id) = seed_repository(&pool).await;
        let store = PostgresPipelineStore::new(pool);

        let created = store
            .create(NewPipeline {
                repository_id,
                commit_sha: "abc".to_string(),
                execution_engine: ExecutionEngine::DockerRunners,
                triggered_by: pusher_id,
            })
            .await
            .unwrap();

        assert_eq!(created.triggered_by, pusher_id);
        let found = store.find_by_id(created.id).await.unwrap().unwrap();
        assert_eq!(found.triggered_by, pusher_id);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn count_created_since_excludes_pipelines_created_before_the_cutoff(pool: PgPool) {
        let (owner_id, repository_id) = seed_repository(&pool).await;
        let store = PostgresPipelineStore::new(pool);
        let old = store
            .create(NewPipeline {
                repository_id,
                commit_sha: "old".to_string(),
                execution_engine: ExecutionEngine::DockerRunners,
                triggered_by: owner_id,
            })
            .await
            .unwrap();

        // Real gap between the two created_at values: sub-millisecond inserts would make the cutoff ambiguous.
        std::thread::sleep(std::time::Duration::from_millis(5));

        store
            .create(NewPipeline {
                repository_id,
                commit_sha: "new".to_string(),
                execution_engine: ExecutionEngine::DockerRunners,
                triggered_by: owner_id,
            })
            .await
            .unwrap();

        let cutoff = old.created_at + chrono::Duration::milliseconds(1);

        let count = store.count_created_since(cutoff).await.unwrap();
        assert_eq!(count, 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_failed_pipeline_is_created_finished_with_its_error(pool: PgPool) {
        let (owner_id, repository_id) = seed_repository(&pool).await;
        let store = PostgresPipelineStore::new(pool);

        let created = store
            .create_failed(
                NewPipeline {
                    repository_id,
                    commit_sha: "abc".to_string(),
                    execution_engine: ExecutionEngine::DockerRunners,
                    triggered_by: owner_id,
                },
                "invalid YAML: boom",
            )
            .await
            .unwrap();
        assert_eq!(created.status, PipelineStatus::Failed);

        let found = store.find_by_id(created.id).await.unwrap().unwrap();
        assert_eq!(found.status, PipelineStatus::Failed);
        assert_eq!(found.error.as_deref(), Some("invalid YAML: boom"));
        assert!(found.finished_at.is_some());
        let listed = store.list_for_repository(repository_id).await.unwrap();
        assert_eq!(listed[0].error.as_deref(), Some("invalid YAML: boom"));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_regular_pipeline_has_no_error(pool: PgPool) {
        let (owner_id, repository_id) = seed_repository(&pool).await;
        let store = PostgresPipelineStore::new(pool);
        let created = store
            .create(NewPipeline {
                repository_id,
                commit_sha: "abc".to_string(),
                execution_engine: ExecutionEngine::DockerRunners,
                triggered_by: owner_id,
            })
            .await
            .unwrap();
        assert!(created.error.is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn mark_running_only_moves_a_pending_pipeline(pool: PgPool) {
        let (owner_id, repository_id) = seed_repository(&pool).await;
        let store = PostgresPipelineStore::new(pool);
        let created = store
            .create(NewPipeline {
                repository_id,
                commit_sha: "abc".to_string(),
                execution_engine: ExecutionEngine::DockerRunners,
                triggered_by: owner_id,
            })
            .await
            .unwrap();

        assert!(store.mark_running(created.id).await.unwrap());
        assert_eq!(
            store.find_by_id(created.id).await.unwrap().unwrap().status,
            PipelineStatus::Running
        );
        assert!(
            !store.mark_running(created.id).await.unwrap(),
            "already running: nothing to announce"
        );

        store
            .update_status(created.id, PipelineStatus::Canceled)
            .await
            .unwrap();
        assert!(!store.mark_running(created.id).await.unwrap());
        assert_eq!(
            store.find_by_id(created.id).await.unwrap().unwrap().status,
            PipelineStatus::Canceled,
            "a finished pipeline must not be brought back to running"
        );
    }
}
