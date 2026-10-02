use crate::error::infra;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::JobLogRetentionPort;
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

/// Runtime queries (no `!` macro), like the other stores added after the initial schema: they need no `.sqlx` entry.
pub struct PostgresJobLogRetentionStore {
    pool: PgPool,
}

impl PostgresJobLogRetentionStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl JobLogRetentionPort for PostgresJobLogRetentionStore {
    /// One statement: the CTE picks a bounded batch through `jobs_logs_to_purge_idx`, empties it, and records the purge
    /// in the same transaction. A job whose log is already empty never matches, so it is never marked as purged.
    async fn purge_logs_finished_before(
        &self,
        cutoff: DateTime<Utc>,
        limit: i64,
    ) -> Result<u64, DomainError> {
        let result = sqlx::query(
            r#"
            WITH purged AS (
                UPDATE jobs SET logs = ''
                WHERE id IN (
                    SELECT id FROM jobs
                    WHERE logs <> ''
                      AND status NOT IN ('pending', 'running')
                      AND finished_at < $1
                    ORDER BY finished_at
                    LIMIT $2
                )
                RETURNING id
            )
            INSERT INTO job_log_purges (job_id) SELECT id FROM purged
            "#,
        )
        .bind(cutoff)
        .bind(limit)
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(result.rows_affected())
    }

    async fn logs_purged_at(
        &self,
        pipeline_id: Uuid,
    ) -> Result<HashMap<Uuid, DateTime<Utc>>, DomainError> {
        let rows: Vec<(Uuid, DateTime<Utc>)> = sqlx::query_as(
            "SELECT p.job_id, p.purged_at FROM job_log_purges p JOIN jobs j ON j.id = p.job_id WHERE j.pipeline_id = $1",
        )
        .bind(pipeline_id)
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::test_support::seed_user;
    use chrono::Duration;

    async fn seed_pipeline(pool: &PgPool) -> Uuid {
        let owner_id = seed_user(pool, "florian").await;
        let repository_id: Uuid = sqlx::query_scalar(
            "INSERT INTO repositories (owner_id, name, disk_path) VALUES ($1, 'hello', 'path') RETURNING id",
        )
        .bind(owner_id)
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query_scalar(
            "INSERT INTO pipelines (repository_id, commit_sha, triggered_by) VALUES ($1, 'abc', $2) RETURNING id",
        )
        .bind(repository_id)
        .bind(owner_id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// A job in `status`, with `logs`, that finished `days_ago` days ago.
    async fn seed_job(
        pool: &PgPool,
        pipeline_id: Uuid,
        status: &str,
        days_ago: i32,
        logs: &str,
    ) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO jobs (pipeline_id, stage, name, image, script, status, logs, finished_at) \
             VALUES ($1, 'test', $2, 'alpine', '[]', $3, $4, \
                     CASE WHEN $3 IN ('pending', 'running') THEN NULL ELSE now() - make_interval(days => $5) END) \
             RETURNING id",
        )
        .bind(pipeline_id)
        .bind(Uuid::new_v4().to_string())
        .bind(status)
        .bind(logs)
        .bind(days_ago)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn log_of(pool: &PgPool, id: Uuid) -> String {
        sqlx::query_scalar("SELECT logs FROM jobs WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    fn days_ago(days: i64) -> DateTime<Utc> {
        Utc::now() - Duration::days(days)
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn purging_empties_only_the_logs_of_finished_jobs_older_than_the_cutoff(pool: PgPool) {
        let pipeline_id = seed_pipeline(&pool).await;
        let old_ok = seed_job(&pool, pipeline_id, "success", 40, "ok log").await;
        let old_failed = seed_job(&pool, pipeline_id, "failed", 31, "failed log").await;
        let old_canceled = seed_job(&pool, pipeline_id, "canceled", 90, "canceled log").await;
        let recent = seed_job(&pool, pipeline_id, "success", 5, "recent log").await;
        let running = seed_job(&pool, pipeline_id, "running", 0, "live log").await;
        let pending = seed_job(&pool, pipeline_id, "pending", 0, "queued").await;
        let store = PostgresJobLogRetentionStore::new(pool.clone());

        let purged = store
            .purge_logs_finished_before(days_ago(30), 100)
            .await
            .unwrap();

        assert_eq!(purged, 3);
        for id in [old_ok, old_failed, old_canceled] {
            assert_eq!(log_of(&pool, id).await, "");
        }
        assert_eq!(log_of(&pool, recent).await, "recent log");
        assert_eq!(log_of(&pool, running).await, "live log");
        assert_eq!(log_of(&pool, pending).await, "queued");
        // The job itself, its status and its finish time survive.
        let (status, finished): (String, Option<DateTime<Utc>>) =
            sqlx::query_as("SELECT status, finished_at FROM jobs WHERE id = $1")
                .bind(old_ok)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "success");
        assert!(finished.is_some());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn purging_remembers_which_jobs_were_purged_and_skips_them_afterwards(pool: PgPool) {
        let pipeline_id = seed_pipeline(&pool).await;
        let purged_job = seed_job(&pool, pipeline_id, "success", 40, "some log").await;
        seed_job(&pool, pipeline_id, "success", 40, "").await;
        seed_job(&pool, pipeline_id, "success", 1, "fresh").await;
        let store = PostgresJobLogRetentionStore::new(pool);

        assert_eq!(
            store
                .purge_logs_finished_before(days_ago(30), 100)
                .await
                .unwrap(),
            1,
            "a job that never had a log is not counted"
        );
        assert_eq!(
            store
                .purge_logs_finished_before(days_ago(30), 100)
                .await
                .unwrap(),
            0,
            "a second sweep finds nothing left to do"
        );

        let purged_at = store.logs_purged_at(pipeline_id).await.unwrap();
        assert_eq!(
            purged_at.keys().copied().collect::<Vec<_>>(),
            vec![purged_job]
        );
        assert!(
            store
                .logs_purged_at(Uuid::new_v4())
                .await
                .unwrap()
                .is_empty(),
            "another pipeline has no purged jobs"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn purging_is_bounded_by_the_limit_and_takes_the_oldest_first(pool: PgPool) {
        let pipeline_id = seed_pipeline(&pool).await;
        let newest = seed_job(&pool, pipeline_id, "success", 50, "log").await;
        let middle = seed_job(&pool, pipeline_id, "success", 60, "log").await;
        let oldest = seed_job(&pool, pipeline_id, "success", 70, "log").await;
        let store = PostgresJobLogRetentionStore::new(pool);

        assert_eq!(
            store
                .purge_logs_finished_before(days_ago(30), 2)
                .await
                .unwrap(),
            2
        );

        let purged_at = store.logs_purged_at(pipeline_id).await.unwrap();
        assert!(purged_at.contains_key(&oldest) && purged_at.contains_key(&middle));
        assert!(
            !purged_at.contains_key(&newest),
            "the most recent of the three is left for the next batch"
        );
        assert_eq!(
            store
                .purge_logs_finished_before(days_ago(30), 2)
                .await
                .unwrap(),
            1
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn deleting_a_job_removes_its_purge_marker(pool: PgPool) {
        let pipeline_id = seed_pipeline(&pool).await;
        let job_id = seed_job(&pool, pipeline_id, "success", 40, "log").await;
        let store = PostgresJobLogRetentionStore::new(pool.clone());
        store
            .purge_logs_finished_before(days_ago(30), 10)
            .await
            .unwrap();

        sqlx::query("DELETE FROM jobs WHERE id = $1")
            .bind(job_id)
            .execute(&pool)
            .await
            .unwrap();

        assert!(store.logs_purged_at(pipeline_id).await.unwrap().is_empty());
    }
}
