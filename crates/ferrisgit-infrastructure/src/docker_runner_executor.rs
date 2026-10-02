use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::{Job, JobStatus, JobStorePort};
use ferrisgit_domain::job_execution::JobExecutionPort;
use std::sync::Arc;

pub struct DockerRunnerExecutor {
    jobs: Arc<dyn JobStorePort>,
}

impl DockerRunnerExecutor {
    pub fn new(jobs: Arc<dyn JobStorePort>) -> Self {
        Self { jobs }
    }
}

#[async_trait]
impl JobExecutionPort for DockerRunnerExecutor {
    async fn submit(&self, _job: &Job) -> Result<(), DomainError> {
        // Nothing to do: the job is already pending and runners pull it with claim_next.
        Ok(())
    }

    async fn cancel(&self, job: &Job) -> Result<(), DomainError> {
        // The store skips already-terminal jobs, so there being nothing to cancel is fine.
        self.jobs
            .update_status(job.id, JobStatus::Canceled)
            .await
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ferrisgit_domain::job::NewJob;
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use uuid::Uuid;

    struct FakeJobs(Mutex<Vec<Job>>);
    #[async_trait]
    impl JobStorePort for FakeJobs {
        async fn create(&self, _new_job: NewJob) -> Result<Job, DomainError> {
            unimplemented!()
        }
        async fn find_by_id(&self, _id: Uuid) -> Result<Option<Job>, DomainError> {
            unimplemented!()
        }
        async fn list_for_pipeline(&self, _pipeline_id: Uuid) -> Result<Vec<Job>, DomainError> {
            unimplemented!()
        }
        async fn claim_next(
            &self,
            _runner_id: Uuid,
            _runner_tags: &[String],
        ) -> Result<Option<Job>, DomainError> {
            unimplemented!()
        }
        async fn append_logs(&self, _id: Uuid, _chunk: &str) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn update_status(&self, id: Uuid, status: JobStatus) -> Result<bool, DomainError> {
            let mut jobs = self.0.lock().unwrap();
            let Some(job) = jobs.iter_mut().find(|j| j.id == id) else {
                return Ok(false);
            };
            if job.status.is_terminal() {
                return Ok(false);
            }
            job.status = status;
            Ok(true)
        }
        async fn release_jobs_claimed_by(&self, _runner_id: Uuid) -> Result<u64, DomainError> {
            unimplemented!()
        }
        async fn count_running(&self) -> Result<i64, DomainError> {
            Ok(0)
        }
        async fn list_runnable(&self, _pipeline_id: Uuid) -> Result<Vec<Job>, DomainError> {
            unimplemented!()
        }
    }

    fn fake_job(id: Uuid) -> Job {
        Job {
            id,
            pipeline_id: Uuid::new_v4(),
            stage: "test".to_string(),
            name: "unit".to_string(),
            image: "alpine".to_string(),
            script: vec![],
            variables: BTreeMap::new(),
            needs: vec![],
            tags: vec![],
            cache: vec![],
            status: JobStatus::Running,
            runner_id: None,
            logs: String::new(),
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
        }
    }

    #[tokio::test]
    async fn submit_does_not_touch_the_job_store() {
        let jobs = Arc::new(FakeJobs(Mutex::new(vec![])));
        let executor = DockerRunnerExecutor::new(jobs);
        executor.submit(&fake_job(Uuid::new_v4())).await.unwrap();
    }

    #[tokio::test]
    async fn cancel_marks_the_job_canceled() {
        let job = fake_job(Uuid::new_v4());
        let jobs = Arc::new(FakeJobs(Mutex::new(vec![job.clone()])));
        let executor = DockerRunnerExecutor::new(jobs.clone());

        executor.cancel(&job).await.unwrap();

        assert_eq!(jobs.0.lock().unwrap()[0].status, JobStatus::Canceled);
    }
}
