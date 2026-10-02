use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::{Job, JobStatus, JobStorePort};
use ferrisgit_domain::pipeline::PipelineStorePort;
use ferrisgit_domain::pipeline_events::{JobEvent, PipelineEventPublisherPort};
use ferrisgit_domain::settings::SystemSettingsStorePort;
use uuid::Uuid;

use crate::use_cases::report_job_result::mark_pipeline_running;

pub struct ClaimNextJobUseCase {
    jobs: Arc<dyn JobStorePort>,
    pipelines: Arc<dyn PipelineStorePort>,
    events: Arc<dyn PipelineEventPublisherPort>,
    system_settings: Arc<dyn SystemSettingsStorePort>,
}

impl ClaimNextJobUseCase {
    pub fn new(
        jobs: Arc<dyn JobStorePort>,
        pipelines: Arc<dyn PipelineStorePort>,
        events: Arc<dyn PipelineEventPublisherPort>,
        system_settings: Arc<dyn SystemSettingsStorePort>,
    ) -> Self {
        Self {
            jobs,
            pipelines,
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
            mark_pipeline_running(&self.pipelines, &self.events, job.pipeline_id).await?;
        }
        Ok(job)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeEvents, FakeJobs, FakePipelines, FakeSystemSettings};
    use crate::use_cases::fixtures::{job, pipeline, system_settings};
    use ferrisgit_domain::pipeline::{Pipeline, PipelineStatus};
    use ferrisgit_domain::pipeline_events::PipelineEvent;
    use ferrisgit_domain::settings::SystemSettings;

    /// Pending with no runner, which is what the real `claim_next` wants. `FakeJobs` filters on status too.
    fn fake_job() -> Job {
        job(Uuid::new_v4(), JobStatus::Pending)
    }

    fn use_case(
        jobs: Arc<FakeJobs>,
        pipelines: Arc<FakePipelines>,
        events: Arc<FakeEvents>,
        ceiling: Option<i32>,
    ) -> ClaimNextJobUseCase {
        ClaimNextJobUseCase::new(
            jobs,
            pipelines,
            events,
            Arc::new(FakeSystemSettings::new(SystemSettings {
                max_concurrent_jobs: ceiling,
                ..system_settings()
            })),
        )
    }

    fn pending_pipeline(id: Uuid) -> Pipeline {
        pipeline(id, PipelineStatus::Pending)
    }

    #[tokio::test]
    async fn claiming_a_job_publishes_a_running_status_event() {
        let job = fake_job();
        let jobs = Arc::new(FakeJobs::new(vec![job.clone()]));
        let events = Arc::new(FakeEvents::new());
        let use_case = use_case(jobs, Arc::new(FakePipelines::empty()), events.clone(), None);

        let claimed = use_case.execute(Uuid::new_v4(), &[]).await.unwrap();

        assert_eq!(claimed.unwrap().id, job.id);
        let published = events.job_events();
        assert_eq!(published.len(), 1);
        let (published_job_id, JobEvent::StatusChanged { status }) = &published[0];
        assert_eq!(*published_job_id, job.id);
        assert_eq!(*status, JobStatus::Running);
    }

    #[tokio::test]
    async fn the_first_claimed_job_moves_the_pipeline_to_running_and_announces_it_once() {
        let first = fake_job();
        let second = Job {
            id: Uuid::new_v4(),
            name: "lint".to_string(),
            pipeline_id: first.pipeline_id,
            ..fake_job()
        };
        let pipelines = Arc::new(FakePipelines::new(vec![pending_pipeline(
            first.pipeline_id,
        )]));
        let events = Arc::new(FakeEvents::new());
        let use_case = use_case(
            Arc::new(FakeJobs::new(vec![first.clone(), second])),
            pipelines.clone(),
            events.clone(),
            None,
        );

        use_case
            .execute(Uuid::new_v4(), &[])
            .await
            .unwrap()
            .unwrap();
        use_case
            .execute(Uuid::new_v4(), &[])
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            pipelines.get(first.pipeline_id).unwrap().status,
            PipelineStatus::Running
        );
        let running_events: Vec<_> = events
            .pipeline_events()
            .into_iter()
            .filter(|(id, _)| *id == first.pipeline_id)
            .collect();
        assert_eq!(running_events.len(), 1, "only the transition is announced");
        let (_, PipelineEvent::StatusChanged { status }) = &running_events[0];
        assert_eq!(*status, PipelineStatus::Running);
    }

    #[tokio::test]
    async fn a_finished_pipeline_is_not_brought_back_to_running() {
        let job = fake_job();
        let pipelines = Arc::new(FakePipelines::new(vec![pipeline(
            job.pipeline_id,
            PipelineStatus::Canceled,
        )]));
        let events = Arc::new(FakeEvents::new());
        let use_case = use_case(
            Arc::new(FakeJobs::new(vec![job.clone()])),
            pipelines.clone(),
            events.clone(),
            None,
        );

        use_case.execute(Uuid::new_v4(), &[]).await.unwrap();

        assert_eq!(
            pipelines.get(job.pipeline_id).unwrap().status,
            PipelineStatus::Canceled
        );
        assert!(events.pipeline_events().is_empty());
    }

    #[tokio::test]
    async fn a_job_of_a_later_stage_is_not_claimed_before_the_earlier_stage_succeeded() {
        let build = fake_job();
        let test = Job {
            id: Uuid::new_v4(),
            stage: "test".to_string(),
            name: "unit".to_string(),
            pipeline_id: build.pipeline_id,
            ..fake_job()
        };
        let jobs = Arc::new(FakeJobs::new(vec![build.clone(), test.clone()]));
        let use_case = use_case(
            jobs.clone(),
            Arc::new(FakePipelines::empty()),
            Arc::new(FakeEvents::new()),
            None,
        );

        let first = use_case
            .execute(Uuid::new_v4(), &[])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.id, build.id);
        assert!(
            use_case
                .execute(Uuid::new_v4(), &[])
                .await
                .unwrap()
                .is_none(),
            "the build job is running, so the test stage waits"
        );

        jobs.update_status(build.id, JobStatus::Success)
            .await
            .unwrap();
        let second = use_case
            .execute(Uuid::new_v4(), &[])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(second.id, test.id);
    }

    #[tokio::test]
    async fn no_claimable_job_returns_none_without_publishing_anything() {
        let events = Arc::new(FakeEvents::new());
        let use_case = use_case(
            Arc::new(FakeJobs::empty()),
            Arc::new(FakePipelines::empty()),
            events.clone(),
            None,
        );

        let claimed = use_case.execute(Uuid::new_v4(), &[]).await.unwrap();

        assert!(claimed.is_none());
        assert!(events.job_events().is_empty());
        assert!(events.pipeline_events().is_empty());
    }

    #[tokio::test]
    async fn at_the_concurrency_ceiling_claiming_returns_none_without_touching_the_job_store() {
        // The seeded job is claimable, so this fails if the ceiling check doesn't stop before `claim_next`.
        let use_case = use_case(
            Arc::new(FakeJobs::new(vec![fake_job()])),
            Arc::new(FakePipelines::empty()),
            Arc::new(FakeEvents::new()),
            Some(0),
        );

        let claimed = use_case.execute(Uuid::new_v4(), &[]).await.unwrap();

        assert!(claimed.is_none());
    }

    #[tokio::test]
    async fn no_ceiling_configured_claims_normally() {
        let job = fake_job();
        let use_case = use_case(
            Arc::new(FakeJobs::new(vec![job.clone()])),
            Arc::new(FakePipelines::empty()),
            Arc::new(FakeEvents::new()),
            None,
        );

        let claimed = use_case.execute(Uuid::new_v4(), &[]).await.unwrap();

        assert_eq!(claimed.unwrap().id, job.id);
    }
}
