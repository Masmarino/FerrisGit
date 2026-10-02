use std::sync::Arc;

use crate::job_execution_resolver::JobExecutionResolver;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::{JobStatus, JobStorePort};
use ferrisgit_domain::pipeline::{PipelineStatus, PipelineStorePort};
use ferrisgit_domain::pipeline_events::{JobEvent, PipelineEvent, PipelineEventPublisherPort};
use uuid::Uuid;

pub struct CancelPipelineUseCase {
    pipelines: Arc<dyn PipelineStorePort>,
    jobs: Arc<dyn JobStorePort>,
    job_execution: Arc<JobExecutionResolver>,
    events: Arc<dyn PipelineEventPublisherPort>,
}

impl CancelPipelineUseCase {
    pub fn new(
        pipelines: Arc<dyn PipelineStorePort>,
        jobs: Arc<dyn JobStorePort>,
        job_execution: Arc<JobExecutionResolver>,
        events: Arc<dyn PipelineEventPublisherPort>,
    ) -> Self {
        Self {
            pipelines,
            jobs,
            job_execution,
            events,
        }
    }

    pub async fn execute(&self, pipeline_id: Uuid) -> Result<(), DomainError> {
        let pipeline = self
            .pipelines
            .find_by_id(pipeline_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("pipeline".to_string()))?;
        // A finished status is final, or canceling would overwrite the real outcome.
        if matches!(
            pipeline.status,
            PipelineStatus::Success | PipelineStatus::Failed | PipelineStatus::Canceled
        ) {
            return Ok(());
        }
        let executor = self.job_execution.resolve(pipeline.execution_engine);

        self.pipelines
            .update_status(pipeline_id, PipelineStatus::Canceled)
            .await?;
        self.events
            .publish_pipeline_event(
                pipeline_id,
                PipelineEvent::StatusChanged {
                    status: PipelineStatus::Canceled,
                },
            )
            .await
            .ok();

        let jobs = self.jobs.list_for_pipeline(pipeline_id).await?;
        for job in jobs {
            if !matches!(job.status, JobStatus::Pending | JobStatus::Running) {
                continue;
            }
            self.jobs.update_status(job.id, JobStatus::Canceled).await?;
            self.events
                .publish_job_event(
                    job.id,
                    JobEvent::StatusChanged {
                        status: JobStatus::Canceled,
                    },
                )
                .await
                .ok();
            executor.cancel(&job).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeEvents, FakeExecution, FakeJobs, FakePipelines};
    use crate::use_cases::fixtures::{job, pipeline};
    use ferrisgit_domain::job::Job;
    use ferrisgit_domain::pipeline::Pipeline;
    use ferrisgit_domain::settings::ExecutionEngine;

    struct Fixture {
        use_case: CancelPipelineUseCase,
        pipelines: Arc<FakePipelines>,
        jobs: Arc<FakeJobs>,
        execution: Arc<FakeExecution>,
        events: Arc<FakeEvents>,
    }

    /// Both engines share one `FakeExecution`, so `canceled()` lists every canceled job.
    fn fixture(pipeline: Pipeline, jobs: Vec<Job>) -> Fixture {
        let pipelines = Arc::new(FakePipelines::new(vec![pipeline]));
        let jobs = Arc::new(FakeJobs::new(jobs));
        let execution = Arc::new(FakeExecution::new());
        let events = Arc::new(FakeEvents::new());
        let use_case = CancelPipelineUseCase::new(
            pipelines.clone(),
            jobs.clone(),
            Arc::new(JobExecutionResolver::new(
                execution.clone(),
                execution.clone(),
            )),
            events.clone(),
        );
        Fixture {
            use_case,
            pipelines,
            jobs,
            execution,
            events,
        }
    }

    impl Fixture {
        fn job_status(&self, job: &Job) -> JobStatus {
            self.jobs
                .snapshot()
                .iter()
                .find(|j| j.id == job.id)
                .unwrap()
                .status
        }
    }

    #[tokio::test]
    async fn canceling_a_pipeline_cancels_its_pending_and_running_jobs_but_leaves_terminal_ones_alone()
     {
        let pipeline = pipeline(Uuid::new_v4(), PipelineStatus::Running);
        let pipeline_id = pipeline.id;
        let pending = job(pipeline_id, JobStatus::Pending);
        let running = job(pipeline_id, JobStatus::Running);
        let already_succeeded = job(pipeline_id, JobStatus::Success);
        let already_failed = job(pipeline_id, JobStatus::Failed);
        let already_canceled = job(pipeline_id, JobStatus::Canceled);
        let f = fixture(
            pipeline,
            vec![
                pending.clone(),
                running.clone(),
                already_succeeded.clone(),
                already_failed.clone(),
                already_canceled.clone(),
            ],
        );

        f.use_case.execute(pipeline_id).await.unwrap();

        assert_eq!(f.pipelines.snapshot()[0].status, PipelineStatus::Canceled);
        assert_eq!(f.job_status(&pending), JobStatus::Canceled);
        assert_eq!(f.job_status(&running), JobStatus::Canceled);
        assert_eq!(
            f.job_status(&already_succeeded),
            JobStatus::Success,
            "an already-succeeded job must not be touched"
        );
        assert_eq!(
            f.job_status(&already_failed),
            JobStatus::Failed,
            "an already-failed job must not be touched"
        );
        assert_eq!(
            f.job_status(&already_canceled),
            JobStatus::Canceled,
            "an already-canceled job's status must remain Canceled (untouched, not re-set)"
        );

        let canceled_via_engine = f.execution.canceled();
        assert_eq!(
            canceled_via_engine.len(),
            2,
            "only the 2 non-terminal jobs should reach the execution engine's cancel"
        );
        assert!(canceled_via_engine.contains(&pending.id));
        assert!(canceled_via_engine.contains(&running.id));
        assert!(!canceled_via_engine.contains(&already_succeeded.id));
        assert!(!canceled_via_engine.contains(&already_failed.id));
        assert!(!canceled_via_engine.contains(&already_canceled.id));

        let published = f.events.job_events();
        assert_eq!(
            published.len(),
            2,
            "no job event should be published for jobs that were already terminal"
        );
        assert!(
            published
                .iter()
                .any(|(id, e)| *id == pending.id && e.event_type() == "JobStatusChanged")
        );
        assert!(
            published
                .iter()
                .any(|(id, e)| *id == running.id && e.event_type() == "JobStatusChanged")
        );
        assert!(!published.iter().any(|(id, _)| *id == already_succeeded.id));
        assert!(!published.iter().any(|(id, _)| *id == already_failed.id));
        assert!(!published.iter().any(|(id, _)| *id == already_canceled.id));
    }

    #[tokio::test]
    async fn cancellation_routes_to_the_pipelines_own_engine_not_a_second_executor() {
        let pipeline = Pipeline {
            execution_engine: ExecutionEngine::Kubernetes,
            ..pipeline(Uuid::new_v4(), PipelineStatus::Running)
        };
        let pipeline_id = pipeline.id;
        let running = job(pipeline_id, JobStatus::Running);
        let docker_executor = Arc::new(FakeExecution::new());
        let kubernetes_executor = Arc::new(FakeExecution::new());
        let use_case = CancelPipelineUseCase::new(
            Arc::new(FakePipelines::new(vec![pipeline])),
            Arc::new(FakeJobs::new(vec![running.clone()])),
            Arc::new(JobExecutionResolver::new(
                docker_executor.clone(),
                kubernetes_executor.clone(),
            )),
            Arc::new(FakeEvents::new()),
        );

        use_case.execute(pipeline_id).await.unwrap();

        assert_eq!(
            kubernetes_executor.canceled(),
            vec![running.id],
            "a pipeline created under kubernetes must have its jobs canceled via the kubernetes executor"
        );
        assert!(
            docker_executor.canceled().is_empty(),
            "never the docker executor, regardless of what execution_engine is currently configured"
        );
    }

    #[tokio::test]
    async fn canceling_an_unknown_pipeline_is_not_found() {
        let f = fixture(pipeline(Uuid::new_v4(), PipelineStatus::Running), vec![]);

        let result = f.use_case.execute(Uuid::new_v4()).await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn canceling_an_already_terminal_pipeline_is_a_no_op() {
        for terminal_status in [
            PipelineStatus::Success,
            PipelineStatus::Failed,
            PipelineStatus::Canceled,
        ] {
            let pipeline = pipeline(Uuid::new_v4(), terminal_status);
            let pipeline_id = pipeline.id;
            // A running job under a finished pipeline is inconsistent, but proves the guard returns before touching jobs.
            let running_job = job(pipeline_id, JobStatus::Running);
            let f = fixture(pipeline, vec![running_job.clone()]);

            f.use_case.execute(pipeline_id).await.unwrap();

            assert_eq!(
                f.pipelines.snapshot()[0].status,
                terminal_status,
                "an already-terminal pipeline's status must never be overwritten with Canceled"
            );
            assert_eq!(
                f.job_status(&running_job),
                JobStatus::Running,
                "no job should be touched when the pipeline is already terminal"
            );
            assert!(
                f.execution.canceled().is_empty(),
                "no job should reach the execution engine's cancel when the pipeline is already terminal"
            );
            assert!(
                f.events.pipeline_events().is_empty() && f.events.job_events().is_empty(),
                "no event should be published when canceling an already-terminal pipeline is a no-op"
            );
        }
    }
}
