use std::sync::Arc;

use ferrisgit_domain::job_execution::JobExecutionPort;
use ferrisgit_domain::settings::ExecutionEngine;

pub struct JobExecutionResolver {
    docker: Arc<dyn JobExecutionPort>,
    kubernetes: Arc<dyn JobExecutionPort>,
}

impl JobExecutionResolver {
    pub fn new(docker: Arc<dyn JobExecutionPort>, kubernetes: Arc<dyn JobExecutionPort>) -> Self {
        Self { docker, kubernetes }
    }

    pub fn resolve(&self, engine: ExecutionEngine) -> Arc<dyn JobExecutionPort> {
        match engine {
            ExecutionEngine::DockerRunners => self.docker.clone(),
            ExecutionEngine::Kubernetes => self.kubernetes.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    use ferrisgit_domain::error::DomainError;
    use ferrisgit_domain::job::{Job, JobStatus};
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use uuid::Uuid;

    #[derive(Default)]
    struct TaggedExecutor {
        tag: &'static str,
        submitted: Mutex<Vec<&'static str>>,
    }
    #[async_trait]
    impl JobExecutionPort for TaggedExecutor {
        async fn submit(&self, _job: &Job) -> Result<(), DomainError> {
            self.submitted.lock().unwrap().push(self.tag);
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
            stage: "build".to_string(),
            name: "compile".to_string(),
            image: "rust".to_string(),
            script: vec![],
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
    async fn resolves_docker_runners_to_the_docker_executor() {
        let docker = Arc::new(TaggedExecutor {
            tag: "docker",
            ..Default::default()
        });
        let kubernetes = Arc::new(TaggedExecutor {
            tag: "kubernetes",
            ..Default::default()
        });
        let resolver = JobExecutionResolver::new(docker.clone(), kubernetes.clone());

        resolver
            .resolve(ExecutionEngine::DockerRunners)
            .submit(&fake_job())
            .await
            .unwrap();

        assert_eq!(*docker.submitted.lock().unwrap(), vec!["docker"]);
        assert!(kubernetes.submitted.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn resolves_kubernetes_to_the_kubernetes_executor() {
        let docker = Arc::new(TaggedExecutor {
            tag: "docker",
            ..Default::default()
        });
        let kubernetes = Arc::new(TaggedExecutor {
            tag: "kubernetes",
            ..Default::default()
        });
        let resolver = JobExecutionResolver::new(docker.clone(), kubernetes.clone());

        resolver
            .resolve(ExecutionEngine::Kubernetes)
            .submit(&fake_job())
            .await
            .unwrap();

        assert_eq!(*kubernetes.submitted.lock().unwrap(), vec!["kubernetes"]);
        assert!(docker.submitted.lock().unwrap().is_empty());
    }
}
