use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::JobStorePort;
use ferrisgit_domain::runner::RunnerRepositoryPort;
use uuid::Uuid;

pub struct DeleteRunnerUseCase {
    runners: Arc<dyn RunnerRepositoryPort>,
    jobs: Arc<dyn JobStorePort>,
}

impl DeleteRunnerUseCase {
    pub fn new(runners: Arc<dyn RunnerRepositoryPort>, jobs: Arc<dyn JobStorePort>) -> Self {
        Self { runners, jobs }
    }

    /// Revokes a runner: its token stops authenticating immediately.
    ///
    /// Claimed jobs go back to `pending` first. `jobs.runner_id` is `ON DELETE SET NULL` but `claim_next` only looks at
    /// `pending` rows, so a `running` job with a null runner would be stranded and its pipeline would never finish. The
    /// two steps are not in one transaction, so the release comes first: a failed delete just leaves the jobs
    /// claimable, while after the delete the release would match nothing.
    pub async fn execute(&self, runner_id: Uuid) -> Result<(), DomainError> {
        self.jobs.release_jobs_claimed_by(runner_id).await?;
        self.runners.delete(runner_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeJobs, FakeRunners};
    use chrono::Utc;
    use ferrisgit_domain::job::{Job, JobStatus};

    fn job(runner_id: Option<Uuid>, status: JobStatus) -> Job {
        Job {
            id: Uuid::new_v4(),
            pipeline_id: Uuid::new_v4(),
            stage: "build".to_string(),
            name: "compile".to_string(),
            image: "alpine".to_string(),
            script: vec![],
            variables: Default::default(),
            needs: vec![],
            tags: vec![],
            cache: vec![],
            status,
            runner_id,
            logs: String::new(),
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
        }
    }

    #[tokio::test]
    async fn deleting_a_runner_releases_its_running_jobs_before_deleting_it() {
        let runner_id = Uuid::new_v4();
        let other_runner_id = Uuid::new_v4();
        let mine = job(Some(runner_id), JobStatus::Running);
        let already_done = job(Some(runner_id), JobStatus::Success);
        let someone_elses = job(Some(other_runner_id), JobStatus::Running);
        let jobs = Arc::new(FakeJobs::new(vec![
            mine.clone(),
            already_done.clone(),
            someone_elses.clone(),
        ]));
        let runners = Arc::new(FakeRunners::default());
        let use_case = DeleteRunnerUseCase::new(runners.clone(), jobs.clone());

        use_case.execute(runner_id).await.unwrap();

        assert_eq!(runners.deleted_ids(), vec![runner_id]);
        let stored = jobs.snapshot();
        let released = stored.iter().find(|j| j.id == mine.id).unwrap();
        assert_eq!(
            released.status,
            JobStatus::Pending,
            "the revoked runner's in-flight job must become claimable again"
        );
        assert_eq!(released.runner_id, None);
        assert_eq!(
            stored
                .iter()
                .find(|j| j.id == already_done.id)
                .unwrap()
                .status,
            JobStatus::Success,
            "a finished job must not be re-queued"
        );
        assert_eq!(
            stored
                .iter()
                .find(|j| j.id == someone_elses.id)
                .unwrap()
                .status,
            JobStatus::Running,
            "another runner's job must not be touched"
        );
        assert_eq!(
            stored
                .iter()
                .find(|j| j.id == someone_elses.id)
                .unwrap()
                .runner_id,
            Some(other_runner_id)
        );
    }
}
