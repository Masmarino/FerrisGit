use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::runner::{NewRunner, Runner, RunnerRepositoryPort};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresRunnerRepository {
    pool: PgPool,
}

impl PostgresRunnerRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RunnerRepositoryPort for PostgresRunnerRepository {
    async fn create(&self, new_runner: NewRunner) -> Result<Runner, DomainError> {
        sqlx::query_as!(
            Runner,
            "INSERT INTO runners (name, token_hash, tags) VALUES ($1, $2, $3) RETURNING *",
            new_runner.name,
            new_runner.token_hash,
            &new_runner.tags
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))
    }

    async fn find_by_token_hash(&self, token_hash: &str) -> Result<Option<Runner>, DomainError> {
        sqlx::query_as!(
            Runner,
            "SELECT * FROM runners WHERE token_hash = $1",
            token_hash
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))
    }

    async fn list(&self) -> Result<Vec<Runner>, DomainError> {
        sqlx::query_as!(Runner, "SELECT * FROM runners ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))
    }

    async fn touch_heartbeat(&self, id: Uuid) -> Result<(), DomainError> {
        sqlx::query!(
            "UPDATE runners SET last_heartbeat_at = now() WHERE id = $1",
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        sqlx::query!("DELETE FROM runners WHERE id = $1", id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrisgit_domain::job::JobStorePort;
    use ferrisgit_domain::pipeline::PipelineStorePort;
    use ferrisgit_domain::repository::RepositoryStorePort;
    use ferrisgit_domain::user::UserRepositoryPort;

    #[sqlx::test(migrations = "../../migrations")]
    async fn creating_then_finding_a_runner_by_token_hash_returns_it(pool: PgPool) {
        let repo = PostgresRunnerRepository::new(pool);
        let created = repo
            .create(NewRunner {
                name: "vps-1".to_string(),
                token_hash: "abchash".to_string(),
                tags: vec!["docker".to_string()],
            })
            .await
            .unwrap();

        let found = repo.find_by_token_hash("abchash").await.unwrap().unwrap();
        assert_eq!(found.id, created.id);
        assert_eq!(found.tags, vec!["docker".to_string()]);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn touch_heartbeat_sets_last_heartbeat_at(pool: PgPool) {
        let repo = PostgresRunnerRepository::new(pool);
        let created = repo
            .create(NewRunner {
                name: "vps-1".to_string(),
                token_hash: "h".to_string(),
                tags: vec![],
            })
            .await
            .unwrap();
        assert!(created.last_heartbeat_at.is_none());

        repo.touch_heartbeat(created.id).await.unwrap();

        let found = repo.find_by_token_hash("h").await.unwrap().unwrap();
        assert!(found.last_heartbeat_at.is_some());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn deleting_a_runner_revokes_its_token(pool: PgPool) {
        let repo = PostgresRunnerRepository::new(pool);
        let created = repo
            .create(NewRunner {
                name: "vps-1".to_string(),
                token_hash: "h".to_string(),
                tags: vec![],
            })
            .await
            .unwrap();

        repo.delete(created.id).await.unwrap();

        assert!(
            repo.find_by_token_hash("h").await.unwrap().is_none(),
            "a revoked runner's token must no longer authenticate"
        );
        assert!(repo.list().await.unwrap().is_empty());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn deleting_a_runner_that_does_not_exist_is_not_an_error(pool: PgPool) {
        let repo = PostgresRunnerRepository::new(pool);
        repo.delete(Uuid::new_v4()).await.unwrap();
    }

    /// A job claimed by a deleted runner must survive (`jobs.runner_id ... ON DELETE SET NULL`).
    #[sqlx::test(migrations = "../../migrations")]
    async fn deleting_a_runner_nulls_the_runner_id_of_its_jobs_instead_of_deleting_them(
        pool: PgPool,
    ) {
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let owner_id = users
            .create(ferrisgit_domain::user::NewUser {
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
                ferrisgit_domain::repository::NewRepository {
                    owner_id,
                    name: "hello".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: ferrisgit_domain::repository::RepositoryVisibility::Private,
                },
                "path".to_string(),
            )
            .await
            .unwrap()
            .id;
        let pipelines = crate::postgres::pipeline_store::PostgresPipelineStore::new(pool.clone());
        let pipeline_id = pipelines
            .create(ferrisgit_domain::pipeline::NewPipeline {
                repository_id,
                commit_sha: "abc".to_string(),
                execution_engine: ferrisgit_domain::settings::ExecutionEngine::DockerRunners,
                triggered_by: owner_id,
            })
            .await
            .unwrap()
            .id;

        let jobs = crate::postgres::job_store::PostgresJobStore::new(pool.clone());
        jobs.create(ferrisgit_domain::job::NewJob {
            pipeline_id,
            stage: "build".to_string(),
            name: "compile".to_string(),
            image: "alpine".to_string(),
            script: vec!["echo hi".to_string()],
            variables: Default::default(),
            needs: vec![],
            tags: vec![],
            cache: vec![],
        })
        .await
        .unwrap();

        let repo = PostgresRunnerRepository::new(pool);
        let runner = repo
            .create(NewRunner {
                name: "vps-1".to_string(),
                token_hash: "h".to_string(),
                tags: vec![],
            })
            .await
            .unwrap();
        let claimed = jobs.claim_next(runner.id, &[]).await.unwrap().unwrap();
        assert_eq!(claimed.status, ferrisgit_domain::job::JobStatus::Running);
        assert_eq!(claimed.runner_id, Some(runner.id));

        // What the delete *use case* does first: release the claim, so the job
        // is claimable again rather than stranded `running` with a null runner.
        assert_eq!(jobs.release_jobs_claimed_by(runner.id).await.unwrap(), 1);
        repo.delete(runner.id).await.unwrap();

        let after = jobs
            .find_by_id(claimed.id)
            .await
            .unwrap()
            .expect("ON DELETE SET NULL must keep the job row, not cascade-delete it");
        assert_eq!(after.runner_id, None);
        assert_eq!(after.status, ferrisgit_domain::job::JobStatus::Pending);

        let other_runner = repo
            .create(NewRunner {
                name: "vps-2".to_string(),
                token_hash: "h2".to_string(),
                tags: vec![],
            })
            .await
            .unwrap();
        let reclaimed = jobs
            .claim_next(other_runner.id, &[])
            .await
            .unwrap()
            .expect("the released job must be claimable by another runner");
        assert_eq!(reclaimed.id, claimed.id);
    }

    /// Order matters: releasing after the delete wouldn't fail, but by then the
    /// FK has nulled `runner_id` and `release_jobs_claimed_by` matches nothing.
    /// That's why the use case releases first.
    #[sqlx::test(migrations = "../../migrations")]
    async fn release_jobs_claimed_by_only_touches_running_jobs_of_that_runner(pool: PgPool) {
        let jobs = crate::postgres::job_store::PostgresJobStore::new(pool.clone());
        let repo = PostgresRunnerRepository::new(pool);
        let runner = repo
            .create(NewRunner {
                name: "vps-1".to_string(),
                token_hash: "h".to_string(),
                tags: vec![],
            })
            .await
            .unwrap();

        assert_eq!(
            jobs.release_jobs_claimed_by(runner.id).await.unwrap(),
            0,
            "a runner with no claimed jobs releases nothing"
        );
    }
}
