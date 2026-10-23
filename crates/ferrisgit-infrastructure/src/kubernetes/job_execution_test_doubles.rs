#![cfg(test)]
use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::{Job, JobStatus, JobStorePort, NewJob};
use ferrisgit_domain::pipeline::{NewPipeline, Pipeline, PipelineStatus, PipelineStorePort};
use ferrisgit_domain::pipeline_events::{JobEvent, PipelineEvent, PipelineEventPublisherPort};
use std::sync::Mutex;
use uuid::Uuid;

pub struct FakePipelines(pub Pipeline);
#[async_trait]
impl PipelineStorePort for FakePipelines {
    async fn create(&self, _new_pipeline: NewPipeline) -> Result<Pipeline, DomainError> {
        unimplemented!()
    }
    async fn create_failed(
        &self,
        _new_pipeline: NewPipeline,
        _error: &str,
    ) -> Result<Pipeline, DomainError> {
        unimplemented!()
    }
    async fn mark_running(&self, _id: Uuid) -> Result<bool, DomainError> {
        Ok(false)
    }
    async fn find_by_id(&self, _id: Uuid) -> Result<Option<Pipeline>, DomainError> {
        Ok(Some(self.0.clone()))
    }
    async fn list_for_repository(
        &self,
        _repository_id: Uuid,
    ) -> Result<Vec<Pipeline>, DomainError> {
        unimplemented!()
    }
    // The fake has no interior mutability and no test checks pipeline status.
    async fn update_status(&self, _id: Uuid, _status: PipelineStatus) -> Result<(), DomainError> {
        Ok(())
    }
    async fn count_created_since(
        &self,
        _since: chrono::DateTime<chrono::Utc>,
    ) -> Result<i64, DomainError> {
        unimplemented!()
    }
    async fn list_for_merge_request(
        &self,
        _merge_request_id: uuid::Uuid,
    ) -> Result<Vec<Pipeline>, DomainError> {
        unimplemented!()
    }
    async fn latest_for_merge_requests(
        &self,
        _merge_request_ids: &[uuid::Uuid],
    ) -> Result<Vec<Pipeline>, DomainError> {
        unimplemented!()
    }
}

#[derive(Default)]
pub struct FakeJobs(pub Mutex<Vec<Job>>);
#[async_trait]
impl JobStorePort for FakeJobs {
    async fn create(&self, _new_job: NewJob) -> Result<Job, DomainError> {
        unimplemented!()
    }
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Job>, DomainError> {
        Ok(self.0.lock().unwrap().iter().find(|j| j.id == id).cloned())
    }
    async fn list_for_pipeline(&self, pipeline_id: Uuid) -> Result<Vec<Job>, DomainError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|j| j.pipeline_id == pipeline_id)
            .cloned()
            .collect())
    }
    async fn claim_next(
        &self,
        _runner_id: Uuid,
        _runner_tags: &[String],
    ) -> Result<Option<Job>, DomainError> {
        unimplemented!()
    }
    async fn append_logs(&self, id: Uuid, chunk: &str) -> Result<(), DomainError> {
        if let Some(job) = self.0.lock().unwrap().iter_mut().find(|j| j.id == id) {
            job.logs.push_str(chunk);
        }
        Ok(())
    }
    async fn list_runnable(&self, _pipeline_id: Uuid) -> Result<Vec<Job>, DomainError> {
        Ok(vec![])
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
}

#[derive(Default)]
pub struct FakeEvents(pub Mutex<Vec<String>>);
#[async_trait]
impl PipelineEventPublisherPort for FakeEvents {
    async fn publish_pipeline_event(
        &self,
        _pipeline_id: Uuid,
        _event: PipelineEvent,
    ) -> Result<(), DomainError> {
        Ok(())
    }
    async fn publish_job_event(&self, job_id: Uuid, event: JobEvent) -> Result<(), DomainError> {
        self.0
            .lock()
            .unwrap()
            .push(format!("{}:{}", job_id, event.event_type()));
        Ok(())
    }
}

/// A job's environment as the server would give it: a repository, a commit, CI variables, Docker on or off. Tokens it
/// issues are kept, to check what the Pod got.
#[derive(Default)]
pub struct FakeEnvironments {
    pub repository_id: Uuid,
    pub ci_variables: std::collections::BTreeMap<String, String>,
    pub masked_values: Vec<String>,
    pub docker: bool,
    pub issued: Mutex<Vec<String>>,
}

impl FakeEnvironments {
    pub fn for_repository(repository_id: Uuid) -> Self {
        Self {
            repository_id,
            ..Self::default()
        }
    }
}

#[async_trait]
impl ferrisgit_application::use_cases::job_environment::JobEnvironmentSource for FakeEnvironments {
    async fn environment(
        &self,
        _job: &Job,
    ) -> Result<ferrisgit_application::use_cases::job_environment::JobEnvironment, DomainError>
    {
        Ok(
            ferrisgit_application::use_cases::job_environment::JobEnvironment {
                repository_id: self.repository_id,
                repository_path: "acme/widget".to_string(),
                commit_sha: "abc123".to_string(),
                ci_variables: self.ci_variables.clone(),
                masked_values: self.masked_values.clone(),
                docker: self.docker,
            },
        )
    }

    async fn issue_clone_token(&self, job_id: Uuid) -> Result<String, DomainError> {
        let token = format!("fgj_test_{job_id}");
        self.issued.lock().unwrap().push(token.clone());
        Ok(token)
    }
}
