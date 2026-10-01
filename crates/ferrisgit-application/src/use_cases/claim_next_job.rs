use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::{Job, JobStatus, JobStorePort};
use ferrisgit_domain::pipeline_events::{JobEvent, PipelineEventPublisherPort};
use ferrisgit_domain::settings::SystemSettingsStorePort;
use uuid::Uuid;

pub struct ClaimNextJobUseCase {
    jobs: Arc<dyn JobStorePort>,
    events: Arc<dyn PipelineEventPublisherPort>,
    system_settings: Arc<dyn SystemSettingsStorePort>,
}

impl ClaimNextJobUseCase {
    pub fn new(
        jobs: Arc<dyn JobStorePort>,
        events: Arc<dyn PipelineEventPublisherPort>,
        system_settings: Arc<dyn SystemSettingsStorePort>,
    ) -> Self {
        Self {
            jobs,
            events,
            system_settings,
        }
    }

    pub async fn execute(
        &self,
        runner_id: Uuid,
        runner_tags: &[String],
    ) -> Result<Option<Job>, DomainError> {
        if let Some(ceiling) = self.system_settings.get().await?.max_concurrent_jobs
            && self.jobs.count_running().await? >= ceiling as i64
        {
            return Ok(None);
        }

        let job = self.jobs.claim_next(runner_id, runner_tags).await?;
        if let Some(job) = &job {
            self.events
                .publish_job_event(
                    job.id,
                    JobEvent::StatusChanged {
                        status: JobStatus::Running,
                    },
                )
                .await
                .ok();
        }
        Ok(job)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeEvents, FakeJobs};
    use async_trait::async_trait;
    use chrono::Utc;
    use std::collections::BTreeMap;

    struct FakeSettingsWithCeiling(Option<i32>);
    #[async_trait]
    impl ferrisgit_domain::settings::SystemSettingsStorePort for FakeSettingsWithCeiling {
        async fn get(&self) -> Result<ferrisgit_domain::settings::SystemSettings, DomainError> {
            Ok(ferrisgit_domain::settings::SystemSettings {
                execution_engine: ferrisgit_domain::settings::ExecutionEngine::DockerRunners,
                k8s_namespace: None,
                k8s_cache_storage_class: None,
                runner_registration_token: None,
                log_retention_days: None,
                max_concurrent_jobs: self.0,
                jwt_ttl_hours: 12,
                max_push_size_mb: 500,
            })
        }
        async fn update(
            &self,
            _update: ferrisgit_domain::settings::SystemSettingsUpdate,
        ) -> Result<ferrisgit_domain::settings::SystemSettings, DomainError> {
            unimplemented!()
        }
    }

    /// A claimable job: `Pending` with no runner, as `PostgresJobStore::claim_next` expects. The shared `FakeJobs`
    /// really filters on status.
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
    async fn claiming_a_job_publishes_a_running_status_event() {
        let job = fake_job();
        let jobs = Arc::new(FakeJobs::new(vec![job.clone()]));
        let events = Arc::new(FakeEvents::new());
        let use_case = ClaimNextJobUseCase::new(
            jobs,
            events.clone(),
            Arc::new(FakeSettingsWithCeiling(None)),
        );

        let claimed = use_case.execute(Uuid::new_v4(), &[]).await.unwrap();

        assert_eq!(claimed.unwrap().id, job.id);
        let published = events.job_events();
        assert_eq!(published.len(), 1);
        let (published_job_id, JobEvent::StatusChanged { status }) = &published[0];
        assert_eq!(*published_job_id, job.id);
        assert_eq!(*status, JobStatus::Running);
    }

    #[tokio::test]
    async fn no_claimable_job_returns_none_without_publishing_anything() {
        let jobs = Arc::new(FakeJobs::empty());
        let events = Arc::new(FakeEvents::new());
        let use_case = ClaimNextJobUseCase::new(
            jobs,
            events.clone(),
            Arc::new(FakeSettingsWithCeiling(None)),
        );

        let claimed = use_case.execute(Uuid::new_v4(), &[]).await.unwrap();

        assert!(claimed.is_none());
        assert!(events.job_events().is_empty());
    }

    #[tokio::test]
    async fn at_the_concurrency_ceiling_claiming_returns_none_without_touching_the_job_store() {
        let job = fake_job();
        let jobs = Arc::new(FakeJobs::new(vec![job]));
        // The seeded job is claimable, so the test fails if the ceiling check didn't short-circuit before
        // `claim_next`.
        let events = Arc::new(FakeEvents::new());
        let use_case =
            ClaimNextJobUseCase::new(jobs, events, Arc::new(FakeSettingsWithCeiling(Some(0))));

        let claimed = use_case.execute(Uuid::new_v4(), &[]).await.unwrap();

        assert!(claimed.is_none());
    }

    #[tokio::test]
    async fn no_ceiling_configured_claims_normally() {
        let job = fake_job();
        let jobs = Arc::new(FakeJobs::new(vec![job.clone()]));
        let events = Arc::new(FakeEvents::new());
        let use_case =
            ClaimNextJobUseCase::new(jobs, events, Arc::new(FakeSettingsWithCeiling(None)));

        let claimed = use_case.execute(Uuid::new_v4(), &[]).await.unwrap();

        assert_eq!(claimed.unwrap().id, job.id);
    }
}
