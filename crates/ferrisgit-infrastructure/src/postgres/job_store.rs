use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::{Job, JobStatus, JobStorePort, NewJob};
use sqlx::PgPool;
use std::collections::BTreeMap;
use uuid::Uuid;

pub struct PostgresJobStore {
    pool: PgPool,
}

impl PostgresJobStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct Row {
    id: Uuid,
    pipeline_id: Uuid,
    stage: String,
    name: String,
    image: String,
    script: serde_json::Value,
    variables: serde_json::Value,
    needs: Vec<String>,
    tags: Vec<String>,
    cache: Vec<String>,
    status: String,
    runner_id: Option<Uuid>,
    logs: String,
    created_at: chrono::DateTime<chrono::Utc>,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    finished_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Row {
    fn into_domain(self) -> Result<Job, DomainError> {
        let script: Vec<String> = serde_json::from_value(self.script)
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        let variables: BTreeMap<String, String> = serde_json::from_value(self.variables)
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(Job {
            id: self.id,
            pipeline_id: self.pipeline_id,
            stage: self.stage,
            name: self.name,
            image: self.image,
            script,
            variables,
            needs: self.needs,
            tags: self.tags,
            cache: self.cache,
            status: JobStatus::parse(&self.status)?,
            runner_id: self.runner_id,
            logs: self.logs,
            created_at: self.created_at,
            started_at: self.started_at,
            finished_at: self.finished_at,
        })
    }
}

#[async_trait]
impl JobStorePort for PostgresJobStore {
    async fn create(&self, new_job: NewJob) -> Result<Job, DomainError> {
        let script = serde_json::to_value(&new_job.script)
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        let variables = serde_json::to_value(&new_job.variables)
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        let row = sqlx::query_as!(
            Row,
            "INSERT INTO jobs (pipeline_id, stage, name, image, script, variables, needs, tags, cache) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING *",
            new_job.pipeline_id,
            new_job.stage,
            new_job.name,
            new_job.image,
            script,
            variables,
            &new_job.needs,
            &new_job.tags,
            &new_job.cache,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        row.into_domain()
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Job>, DomainError> {
        let row = sqlx::query_as!(Row, "SELECT * FROM jobs WHERE id = $1", id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        row.map(Row::into_domain).transpose()
    }

    async fn list_for_pipeline(&self, pipeline_id: Uuid) -> Result<Vec<Job>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT * FROM jobs WHERE pipeline_id = $1 ORDER BY created_at",
            pipeline_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        rows.into_iter().map(Row::into_domain).collect()
    }

    async fn claim_next(
        &self,
        runner_id: Uuid,
        runner_tags: &[String],
    ) -> Result<Option<Job>, DomainError> {
        let row = sqlx::query_as!(
            Row,
            r#"
            UPDATE jobs
            SET status = 'running', runner_id = $1, started_at = now()
            WHERE id = (
                SELECT id FROM jobs
                WHERE status = 'pending'
                  AND tags <@ $2::text[]
                  AND NOT EXISTS (
                      SELECT 1 FROM unnest(needs) AS need_name
                      WHERE NOT EXISTS (
                          SELECT 1 FROM jobs j2 WHERE j2.pipeline_id = jobs.pipeline_id AND j2.name = need_name AND j2.status = 'success'
                      )
                  )
                ORDER BY created_at
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            )
            RETURNING *
            "#,
            runner_id,
            runner_tags,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        row.map(Row::into_domain).transpose()
    }

    async fn append_logs(&self, id: Uuid, chunk: &str) -> Result<(), DomainError> {
        sqlx::query!("UPDATE jobs SET logs = logs || $1 WHERE id = $2", chunk, id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(())
    }

    /// The `status NOT IN (...)` predicate enforces "terminal is terminal" in the database instead of a
    /// racy read-then-write. `rows_affected() == 0` means the job was already terminal.
    async fn update_status(&self, id: Uuid, status: JobStatus) -> Result<bool, DomainError> {
        let result = sqlx::query!(
            "UPDATE jobs SET status = $1::text, \
                 started_at = CASE WHEN $1::text = 'running' THEN COALESCE(started_at, now()) ELSE started_at END, \
                 finished_at = CASE WHEN $1::text IN ('success', 'failed', 'canceled') THEN now() ELSE finished_at END \
             WHERE id = $2 AND status NOT IN ('success', 'failed', 'canceled')",
            status.as_str(),
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn release_jobs_claimed_by(&self, runner_id: Uuid) -> Result<u64, DomainError> {
        let result = sqlx::query!("UPDATE jobs SET status = 'pending', runner_id = NULL, started_at = NULL WHERE runner_id = $1 AND status = 'running'", runner_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(result.rows_affected())
    }

    async fn count_running(&self) -> Result<i64, DomainError> {
        let row = sqlx::query!("SELECT COUNT(*) as count FROM jobs WHERE status = 'running'")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(row.count.unwrap_or(0))
    }

    async fn list_runnable(&self, pipeline_id: Uuid) -> Result<Vec<Job>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            r#"
            SELECT * FROM jobs
            WHERE pipeline_id = $1
              AND status = 'pending'
              AND NOT EXISTS (
                  SELECT 1 FROM unnest(needs) AS need_name
                  WHERE NOT EXISTS (
                      SELECT 1 FROM jobs j2 WHERE j2.pipeline_id = jobs.pipeline_id AND j2.name = need_name AND j2.status = 'success'
                  )
              )
            ORDER BY created_at
            "#,
            pipeline_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        rows.into_iter().map(Row::into_domain).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::pipeline_store::PostgresPipelineStore;
    use ferrisgit_domain::pipeline::{NewPipeline, PipelineStorePort};
    use ferrisgit_domain::repository::{NewRepository, RepositoryStorePort, RepositoryVisibility};
    use ferrisgit_domain::settings::ExecutionEngine;
    use ferrisgit_domain::user::{NewUser, UserRepositoryPort};

    async fn seed_pipeline(pool: &PgPool) -> Uuid {
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
        let repository_id = repos
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
            .id;
        let pipelines = PostgresPipelineStore::new(pool.clone());
        pipelines
            .create(NewPipeline {
                repository_id,
                commit_sha: "abc".to_string(),
                execution_engine: ExecutionEngine::DockerRunners,
                triggered_by: owner_id,
            })
            .await
            .unwrap()
            .id
    }

    fn job(pipeline_id: Uuid, name: &str, needs: Vec<String>, tags: Vec<String>) -> NewJob {
        NewJob {
            pipeline_id,
            stage: "test".to_string(),
            name: name.to_string(),
            image: "alpine".to_string(),
            script: vec!["echo hi".to_string()],
            variables: BTreeMap::new(),
            needs,
            tags,
            cache: vec![],
        }
    }

    /// `jobs.runner_id` has a foreign key to `runners.id`, so claim_next tests that
    /// actually expect a row to be claimed must pass a runner id that really exists.
    async fn seed_runner(pool: &PgPool) -> Uuid {
        sqlx::query_scalar!(
            "INSERT INTO runners (name, token_hash) VALUES ($1, $2) RETURNING id",
            "test-runner",
            Uuid::new_v4().to_string()
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn claim_next_returns_none_when_nothing_is_pending(pool: PgPool) {
        let store = PostgresJobStore::new(pool);
        assert!(
            store
                .claim_next(Uuid::new_v4(), &[])
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn claim_next_ignores_a_job_whose_tags_are_not_a_subset_of_the_runners(pool: PgPool) {
        let pipeline_id = seed_pipeline(&pool).await;
        let store = PostgresJobStore::new(pool);
        store
            .create(job(pipeline_id, "build", vec![], vec!["gpu".to_string()]))
            .await
            .unwrap();

        let claimed = store
            .claim_next(Uuid::new_v4(), &["docker".to_string()])
            .await
            .unwrap();
        assert!(
            claimed.is_none(),
            "a job requiring 'gpu' must not be claimable by a runner tagged only 'docker'"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn claim_next_ignores_a_job_whose_needs_are_not_yet_satisfied(pool: PgPool) {
        let pipeline_id = seed_pipeline(&pool).await;
        let runner_id = seed_runner(&pool).await;
        let store = PostgresJobStore::new(pool);
        store
            .create(job(pipeline_id, "build", vec![], vec![]))
            .await
            .unwrap();
        store
            .create(job(pipeline_id, "test", vec!["build".to_string()], vec![]))
            .await
            .unwrap();

        let claimed = store.claim_next(runner_id, &[]).await.unwrap().unwrap();
        assert_eq!(
            claimed.name, "build",
            "only the job with no unmet needs should be claimable"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn claim_next_claims_a_job_once_its_needs_succeed(pool: PgPool) {
        let pipeline_id = seed_pipeline(&pool).await;
        let runner_id = seed_runner(&pool).await;
        let store = PostgresJobStore::new(pool);
        let build = store
            .create(job(pipeline_id, "build", vec![], vec![]))
            .await
            .unwrap();
        store
            .create(job(pipeline_id, "test", vec!["build".to_string()], vec![]))
            .await
            .unwrap();

        store
            .update_status(build.id, JobStatus::Success)
            .await
            .unwrap();

        let claimed = store.claim_next(runner_id, &[]).await.unwrap().unwrap();
        assert_eq!(claimed.name, "test");
        assert_eq!(claimed.status, JobStatus::Running);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn two_concurrent_claims_never_pick_the_same_job(pool: PgPool) {
        let pipeline_id = seed_pipeline(&pool).await;
        let runner_a = seed_runner(&pool).await;
        let runner_b = seed_runner(&pool).await;
        let store = std::sync::Arc::new(PostgresJobStore::new(pool));
        store
            .create(job(pipeline_id, "only-job", vec![], vec![]))
            .await
            .unwrap();

        let store_a = store.clone();
        let store_b = store.clone();
        let (a, b) = tokio::join!(
            store_a.claim_next(runner_a, &[]),
            store_b.claim_next(runner_b, &[])
        );
        let claims: Vec<_> = [a.unwrap(), b.unwrap()].into_iter().flatten().collect();
        assert_eq!(
            claims.len(),
            1,
            "exactly one of the two concurrent claims should succeed for a single pending job"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn append_logs_concatenates_chunks(pool: PgPool) {
        let pipeline_id = seed_pipeline(&pool).await;
        let store = PostgresJobStore::new(pool);
        let created = store
            .create(job(pipeline_id, "build", vec![], vec![]))
            .await
            .unwrap();

        store.append_logs(created.id, "line one\n").await.unwrap();
        store.append_logs(created.id, "line two\n").await.unwrap();

        let found = store.find_by_id(created.id).await.unwrap().unwrap();
        assert_eq!(found.logs, "line one\nline two\n");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn count_running_counts_only_running_jobs(pool: PgPool) {
        let pipeline_id = seed_pipeline(&pool).await;
        let store = PostgresJobStore::new(pool);

        let running = store
            .create(job(pipeline_id, "running", vec![], vec![]))
            .await
            .unwrap();
        store
            .update_status(running.id, JobStatus::Running)
            .await
            .unwrap();
        store
            .create(job(pipeline_id, "pending", vec![], vec![]))
            .await
            .unwrap();

        assert_eq!(store.count_running().await.unwrap(), 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn update_status_refuses_to_move_a_job_out_of_a_terminal_status(pool: PgPool) {
        let pipeline_id = seed_pipeline(&pool).await;
        let store = PostgresJobStore::new(pool);

        for terminal in [JobStatus::Success, JobStatus::Failed, JobStatus::Canceled] {
            let created = store
                .create(job(pipeline_id, terminal.as_str(), vec![], vec![]))
                .await
                .unwrap();
            assert!(
                store.update_status(created.id, terminal).await.unwrap(),
                "pending -> terminal must be allowed and report that it changed a row"
            );

            for attempted in [
                JobStatus::Success,
                JobStatus::Failed,
                JobStatus::Canceled,
                JobStatus::Running,
                JobStatus::Pending,
            ] {
                assert!(
                    !store.update_status(created.id, attempted).await.unwrap(),
                    "a {} job must not be movable to {}",
                    terminal.as_str(),
                    attempted.as_str()
                );
                assert_eq!(
                    store.find_by_id(created.id).await.unwrap().unwrap().status,
                    terminal,
                    "the stored status must be unchanged after a refused update"
                );
            }
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn update_status_reports_false_for_a_job_that_does_not_exist(pool: PgPool) {
        let store = PostgresJobStore::new(pool);
        assert!(
            !store
                .update_status(Uuid::new_v4(), JobStatus::Success)
                .await
                .unwrap()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_runnable_returns_a_pending_job_whose_needs_are_all_satisfied(pool: PgPool) {
        let pipeline_id = seed_pipeline(&pool).await;
        let store = PostgresJobStore::new(pool);
        let build = store
            .create(job(pipeline_id, "build", vec![], vec![]))
            .await
            .unwrap();
        let test_job = store
            .create(job(pipeline_id, "test", vec!["build".to_string()], vec![]))
            .await
            .unwrap();
        store
            .update_status(build.id, JobStatus::Success)
            .await
            .unwrap();

        let runnable = store.list_runnable(pipeline_id).await.unwrap();

        assert_eq!(runnable.len(), 1);
        assert_eq!(runnable[0].id, test_job.id);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_runnable_excludes_a_pending_job_whose_needs_are_not_yet_satisfied(pool: PgPool) {
        let pipeline_id = seed_pipeline(&pool).await;
        let store = PostgresJobStore::new(pool);
        let build = store
            .create(job(pipeline_id, "build", vec![], vec![]))
            .await
            .unwrap();
        let test_job = store
            .create(job(pipeline_id, "test", vec!["build".to_string()], vec![]))
            .await
            .unwrap();

        let runnable = store.list_runnable(pipeline_id).await.unwrap();

        assert!(
            runnable.iter().any(|j| j.id == build.id),
            "build has no needs and should be runnable"
        );
        assert!(
            !runnable.iter().any(|j| j.id == test_job.id),
            "build hasn't succeeded yet, so test must not be considered runnable"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_runnable_excludes_a_job_that_is_already_running_or_terminal(pool: PgPool) {
        let pipeline_id = seed_pipeline(&pool).await;
        let store = PostgresJobStore::new(pool);
        let already_running = store
            .create(job(pipeline_id, "build", vec![], vec![]))
            .await
            .unwrap();
        store
            .update_status(already_running.id, JobStatus::Running)
            .await
            .unwrap();

        let runnable = store.list_runnable(pipeline_id).await.unwrap();

        assert!(
            runnable.is_empty(),
            "a job already Running (or terminal) is not Pending and must not be re-submitted"
        );
    }

    async fn find(store: &PostgresJobStore, pipeline_id: Uuid, id: Uuid) -> Job {
        store
            .list_for_pipeline(pipeline_id)
            .await
            .unwrap()
            .into_iter()
            .find(|j| j.id == id)
            .unwrap()
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn claim_next_stamps_started_at_and_leaves_finished_at_empty(pool: PgPool) {
        let pipeline_id = seed_pipeline(&pool).await;
        let runner_id = seed_runner(&pool).await;
        let store = PostgresJobStore::new(pool);
        store
            .create(job(pipeline_id, "build", vec![], vec![]))
            .await
            .unwrap();

        let claimed = store.claim_next(runner_id, &[]).await.unwrap().unwrap();

        assert!(
            claimed.started_at.is_some(),
            "claiming moves the job to running, which starts its clock"
        );
        assert!(claimed.finished_at.is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_job_created_pending_has_no_timestamps(pool: PgPool) {
        let pipeline_id = seed_pipeline(&pool).await;
        let store = PostgresJobStore::new(pool);
        let created = store
            .create(job(pipeline_id, "build", vec![], vec![]))
            .await
            .unwrap();

        let stored = find(&store, pipeline_id, created.id).await;
        assert!(stored.started_at.is_none() && stored.finished_at.is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn update_status_to_running_sets_started_at_once_and_never_moves_it(pool: PgPool) {
        let pipeline_id = seed_pipeline(&pool).await;
        let store = PostgresJobStore::new(pool);
        let created = store
            .create(job(pipeline_id, "build", vec![], vec![]))
            .await
            .unwrap();

        store
            .update_status(created.id, JobStatus::Running)
            .await
            .unwrap();
        let first = find(&store, pipeline_id, created.id)
            .await
            .started_at
            .expect("started_at set on running");
        store
            .update_status(created.id, JobStatus::Running)
            .await
            .unwrap();
        let second = find(&store, pipeline_id, created.id)
            .await
            .started_at
            .unwrap();

        assert_eq!(
            first, second,
            "a repeated running update must not restart the clock"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn update_status_to_a_terminal_status_sets_finished_at_and_it_is_never_overwritten(
        pool: PgPool,
    ) {
        let pipeline_id = seed_pipeline(&pool).await;
        let store = PostgresJobStore::new(pool);
        let created = store
            .create(job(pipeline_id, "build", vec![], vec![]))
            .await
            .unwrap();

        assert!(
            store
                .update_status(created.id, JobStatus::Success)
                .await
                .unwrap()
        );
        let finished = find(&store, pipeline_id, created.id)
            .await
            .finished_at
            .expect("finished_at set on terminal");

        assert!(
            !store
                .update_status(created.id, JobStatus::Failed)
                .await
                .unwrap(),
            "terminal is terminal"
        );
        assert_eq!(
            find(&store, pipeline_id, created.id).await.finished_at,
            Some(finished)
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn releasing_a_dead_runners_jobs_clears_started_at(pool: PgPool) {
        let pipeline_id = seed_pipeline(&pool).await;
        let runner_id = seed_runner(&pool).await;
        let store = PostgresJobStore::new(pool);
        let created = store
            .create(job(pipeline_id, "build", vec![], vec![]))
            .await
            .unwrap();
        store.claim_next(runner_id, &[]).await.unwrap().unwrap();

        store.release_jobs_claimed_by(runner_id).await.unwrap();

        let stored = find(&store, pipeline_id, created.id).await;
        assert_eq!(stored.status, JobStatus::Pending);
        assert!(
            stored.started_at.is_none(),
            "a job back in the queue has not started"
        );
    }
}
