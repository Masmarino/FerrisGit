use async_trait::async_trait;

use crate::error::DomainError;
use crate::job::Job;

#[async_trait]
pub trait JobExecutionPort: Send + Sync {
    /// Called once a job's dependencies are satisfied. What happens depends on the engine: the Docker engine does
    /// nothing (runners poll for ready jobs), and a Kubernetes engine creates a Pod.
    async fn submit(&self, job: &Job) -> Result<(), DomainError>;

    /// Called when a job or its pipeline is canceled.
    async fn cancel(&self, job: &Job) -> Result<(), DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::JobStatus;
    use chrono::Utc;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use uuid::Uuid;

    struct RecordingExecutor {
        submitted: std::sync::Mutex<Vec<uuid::Uuid>>,
    }

    #[async_trait]
    impl JobExecutionPort for RecordingExecutor {
        async fn submit(&self, job: &Job) -> Result<(), DomainError> {
            self.submitted.lock().unwrap().push(job.id);
            Ok(())
        }
        async fn cancel(&self, _job: &Job) -> Result<(), DomainError> {
            Ok(())
        }
    }

    fn fake_job() -> Job {
        Job {
            id: Uuid::new_v4(),
            pipeline_id: Uuid::new_v4(),
            stage: "test".to_string(),
            name: "unit".to_string(),
            image: "alpine".to_string(),
            script: vec!["echo hi".to_string()],
            variables: BTreeMap::new(),
            needs: vec![],
            tags: vec![],
            cache: vec![],
            status: JobStatus::Pending,
            runner_id: None,
            logs: String::new(),
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
        }
    }

    #[tokio::test]
    async fn a_port_implementation_can_be_exercised_through_the_trait_object() {
        let executor: Arc<dyn JobExecutionPort> = Arc::new(RecordingExecutor {
            submitted: std::sync::Mutex::new(vec![]),
        });
        let job = fake_job();
        executor.submit(&job).await.unwrap();
    }
}
