use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::Job;
use ferrisgit_domain::job_execution::JobExecutionPort;

pub struct UnavailableKubernetesExecutor;

const UNAVAILABLE: &str = "Kubernetes execution engine is not available on this server (no cluster was reachable at startup)";

#[async_trait]
impl JobExecutionPort for UnavailableKubernetesExecutor {
    async fn submit(&self, _job: &Job) -> Result<(), DomainError> {
        Err(DomainError::Infrastructure(UNAVAILABLE.to_string()))
    }
    async fn cancel(&self, _job: &Job) -> Result<(), DomainError> {
        Err(DomainError::Infrastructure(UNAVAILABLE.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ferrisgit_domain::job::JobStatus;
    use std::collections::BTreeMap;
    use uuid::Uuid;

    fn fake_job() -> Job {
        Job {
            id: Uuid::new_v4(),
            pipeline_id: Uuid::new_v4(),
            stage: "build".to_string(),
            name: "compile".to_string(),
            image: "alpine:3.20".to_string(),
            script: vec!["true".to_string()],
            variables: BTreeMap::new(),
            needs: vec![],
            tags: vec![],
            status: JobStatus::Pending,
            runner_id: None,
            logs: String::new(),
            created_at: Utc::now(),
            cache: vec![],
            started_at: None,
            finished_at: None,
        }
    }

    #[tokio::test]
    async fn submit_and_cancel_both_fail_loudly_with_no_reachable_cluster() {
        let executor = UnavailableKubernetesExecutor;
        let job = fake_job();

        assert!(matches!(
            executor.submit(&job).await,
            Err(DomainError::Infrastructure(_))
        ));
        assert!(matches!(
            executor.cancel(&job).await,
            Err(DomainError::Infrastructure(_))
        ));
    }
}
