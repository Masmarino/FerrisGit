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
        // Terminal statuses are final (like `JobStorePort::update_status`): otherwise canceling a finished pipeline
        // would discard its real outcome.
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
    use chrono::Utc;
    use ferrisgit_domain::job::Job;
    use ferrisgit_domain::pipeline::Pipeline;
    use ferrisgit_domain::settings::ExecutionEngine;
    use std::collections::BTreeMap;

    fn job(pipeline_id: Uuid, status: JobStatus) -> Job {
        Job {
            id: Uuid::new_v4(),
            pipeline_id,
            stage: "build".to_string(),
            name: "compile".to_string(),
            image: "rust".to_string(),
            script: vec![],
            variables: BTreeMap::new(),
            needs: vec![],
            tags: vec![],
            cache: vec![],
            status,
            runner_id: None,
            logs: String::new(),
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
        }
    }

    #[tokio::test]
    async fn canceling_a_pipeline_cancels_its_pending_and_running_jobs_but_leaves_terminal_ones_alone()
     {
        let pipeline_id = Uuid::new_v4();
        let pending = job(pipeline_id, JobStatus::Pending);
        let running = job(pipeline_id, JobStatus::Running);
        let already_succeeded = job(pipeline_id, JobStatus::Success);
        let already_failed = job(pipeline_id, JobStatus::Failed);
        let already_canceled = job(pipeline_id, JobStatus::Canceled);
        let pipelines = Arc::new(FakePipelines::new(vec![Pipeline {
            id: pipeline_id,
            repository_id: Uuid::new_v4(),
            commit_sha: "abc".to_string(),
            execution_engine: ExecutionEngine::DockerRunners,
            status: PipelineStatus::Running,
            triggered_by: Uuid::new_v4(),
            created_at: Utc::now(),
            finished_at: None,
        }]));
        let jobs = Arc::new(FakeJobs::new(vec![
            pending.clone(),
            running.clone(),
            already_succeeded.clone(),
            already_failed.clone(),
            already_canceled.clone(),
        ]));
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

        use_case.execute(pipeline_id).await.unwrap();

        assert_eq!(pipelines.snapshot()[0].status, PipelineStatus::Canceled);
        let stored_jobs = jobs.snapshot();
        assert_eq!(
            stored_jobs
                .iter()
                .find(|j| j.id == pending.id)
                .unwrap()
                .status,
            JobStatus::Canceled
        );
        assert_eq!(
            stored_jobs
                .iter()
                .find(|j| j.id == running.id)
                .unwrap()
                .status,
            JobStatus::Canceled
        );
        assert_eq!(
            stored_jobs
                .iter()
                .find(|j| j.id == already_succeeded.id)
                .unwrap()
                .status,
            JobStatus::Success,
            "an already-succeeded job must not be touched"
        );
        assert_eq!(
            stored_jobs
                .iter()
                .find(|j| j.id == already_failed.id)
                .unwrap()
                .status,
            JobStatus::Failed,
            "an already-failed job must not be touched"
        );
        assert_eq!(
            stored_jobs
                .iter()
                .find(|j| j.id == already_canceled.id)
                .unwrap()
                .status,
            JobStatus::Canceled,
            "an already-canceled job's status must remain Canceled (untouched, not re-set)"
        );

        let canceled_via_engine = execution.canceled();
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

        let published = events.job_events();
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
        let pipeline_id = Uuid::new_v4();
        let running = job(pipeline_id, JobStatus::Running);
        let pipelines = Arc::new(FakePipelines::new(vec![Pipeline {
            id: pipeline_id,
            repository_id: Uuid::new_v4(),
            commit_sha: "abc".to_string(),
            execution_engine: ferrisgit_domain::settings::ExecutionEngine::Kubernetes,
            status: PipelineStatus::Running,
            triggered_by: Uuid::new_v4(),
            created_at: Utc::now(),
            finished_at: None,
        }]));
        let jobs = Arc::new(FakeJobs::new(vec![running.clone()]));
        let docker_executor = Arc::new(FakeExecution::new());
        let kubernetes_executor = Arc::new(FakeExecution::new());
        let use_case = CancelPipelineUseCase::new(
            pipelines,
            jobs,
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
        let execution = Arc::new(FakeExecution::new());
        let use_case = CancelPipelineUseCase::new(
            Arc::new(FakePipelines::empty()),
            Arc::new(FakeJobs::empty()),
            Arc::new(JobExecutionResolver::new(execution.clone(), execution)),
            Arc::new(FakeEvents::new()),
        );

        let result = use_case.execute(Uuid::new_v4()).await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn canceling_an_already_terminal_pipeline_is_a_no_op() {
        for terminal_status in [
            PipelineStatus::Success,
            PipelineStatus::Failed,
            PipelineStatus::Canceled,
        ] {
            let pipeline_id = Uuid::new_v4();
            let running_job = job(pipeline_id, JobStatus::Running);
            let pipelines = Arc::new(FakePipelines::new(vec![Pipeline {
                id: pipeline_id,
                repository_id: Uuid::new_v4(),
                commit_sha: "abc".to_string(),
                execution_engine: ExecutionEngine::DockerRunners,
                status: terminal_status,
                triggered_by: Uuid::new_v4(),
                created_at: Utc::now(),
                finished_at: None,
            }]));
            // A non-terminal job under a finished pipeline is an inconsistency, but it shows that the guard checks the
            // pipeline's status and returns before touching any job.
            let jobs = Arc::new(FakeJobs::new(vec![running_job.clone()]));
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

            use_case.execute(pipeline_id).await.unwrap();

            assert_eq!(
                pipelines.snapshot()[0].status,
                terminal_status,
                "an already-terminal pipeline's status must never be overwritten with Canceled"
            );
            assert_eq!(
                jobs.snapshot()
                    .iter()
                    .find(|j| j.id == running_job.id)
                    .unwrap()
                    .status,
                JobStatus::Running,
                "no job should be touched when the pipeline is already terminal"
            );
            assert!(
                execution.canceled().is_empty(),
                "no job should reach the execution engine's cancel when the pipeline is already terminal"
            );
            assert!(
                events.pipeline_events().is_empty() && events.job_events().is_empty(),
                "no event should be published when canceling an already-terminal pipeline is a no-op"
            );
        }
    }
}
