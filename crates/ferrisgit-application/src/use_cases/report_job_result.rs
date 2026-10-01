use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::{JobStatus, JobStorePort};
use ferrisgit_domain::notification::{NewNotification, NotificationKind, NotificationStorePort};
use ferrisgit_domain::pipeline::{PipelineStatus, PipelineStorePort};
use ferrisgit_domain::pipeline_events::{JobEvent, PipelineEvent, PipelineEventPublisherPort};
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::user::UserRepositoryPort;
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use ferrisgit_domain::webhook_event::WebhookEvent;
use uuid::Uuid;

use crate::job_execution_resolver::JobExecutionResolver;

pub struct AppendJobLogsUseCase {
    jobs: Arc<dyn JobStorePort>,
}

impl AppendJobLogsUseCase {
    pub fn new(jobs: Arc<dyn JobStorePort>) -> Self {
        Self { jobs }
    }

    pub async fn execute(&self, job_id: Uuid, chunk: &str) -> Result<(), DomainError> {
        self.jobs.append_logs(job_id, chunk).await
    }
}

pub struct ReportJobResultUseCase {
    jobs: Arc<dyn JobStorePort>,
    pipelines: Arc<dyn PipelineStorePort>,
    events: Arc<dyn PipelineEventPublisherPort>,
    job_execution: Arc<JobExecutionResolver>,
}

impl ReportJobResultUseCase {
    pub fn new(
        jobs: Arc<dyn JobStorePort>,
        pipelines: Arc<dyn PipelineStorePort>,
        events: Arc<dyn PipelineEventPublisherPort>,
        job_execution: Arc<JobExecutionResolver>,
    ) -> Self {
        Self {
            jobs,
            pipelines,
            events,
            job_execution,
        }
    }

    /// `status` must be terminal (`Success`, `Failed`, `Canceled`). When every job in the pipeline is terminal the
    /// pipeline follows: `Failed` if any failed, else `Canceled` if any canceled, else `Success`.
    ///
    /// A report for a job that is already terminal is dropped entirely: a runner finishing after its pipeline was
    /// canceled must not un-cancel anything. The store enforces this atomically (`JobStorePort::update_status`).
    ///
    /// A `Failed`/`Canceled` status also cancels every job that transitively `needs` it: `claim_next` never claims
    /// those, so the pipeline would otherwise stay non-terminal forever.
    pub async fn execute(
        &self,
        job_id: Uuid,
        status: JobStatus,
    ) -> Result<Option<Uuid>, DomainError> {
        let changed = self.jobs.update_status(job_id, status).await?;
        let job = self
            .jobs
            .find_by_id(job_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("job".to_string()))?;
        if !changed {
            return Ok(None);
        }
        self.events
            .publish_job_event(job_id, JobEvent::StatusChanged { status })
            .await
            .ok();

        if matches!(status, JobStatus::Failed | JobStatus::Canceled) {
            self.cancel_unreachable_dependents(job.pipeline_id).await?;
        }

        let mut cascade_triggered_by = None;
        if status == JobStatus::Success {
            cascade_triggered_by = self.submit_newly_runnable_jobs(job.pipeline_id).await?;
        }

        let siblings = self.jobs.list_for_pipeline(job.pipeline_id).await?;
        let all_terminal = siblings.iter().all(|j| {
            matches!(
                j.status,
                JobStatus::Success | JobStatus::Failed | JobStatus::Canceled
            )
        });
        if !all_terminal {
            return Ok(None);
        }

        let pipeline_status = if siblings.iter().any(|j| j.status == JobStatus::Failed) {
            PipelineStatus::Failed
        } else if siblings.iter().any(|j| j.status == JobStatus::Canceled) {
            PipelineStatus::Canceled
        } else {
            PipelineStatus::Success
        };

        let previously_failed = self
            .pipelines
            .find_by_id(job.pipeline_id)
            .await
            .ok()
            .flatten()
            .is_some_and(|p| p.status == PipelineStatus::Failed);
        let triggered_by = if pipeline_status == PipelineStatus::Failed && !previously_failed {
            self.pipelines
                .find_by_id(job.pipeline_id)
                .await
                .ok()
                .flatten()
                .map(|p| p.triggered_by)
        } else {
            None
        };

        self.pipelines
            .update_status(job.pipeline_id, pipeline_status)
            .await?;
        self.events
            .publish_pipeline_event(
                job.pipeline_id,
                PipelineEvent::StatusChanged {
                    status: pipeline_status,
                },
            )
            .await
            .ok();

        Ok(cascade_triggered_by.or(triggered_by))
    }

    /// Cancels every `Pending` job that can never be claimed because a (transitive) `need` ended non-`success`. Starts
    /// from every Failed or Canceled job, not just the reported one, so it is idempotent and also sweeps dependents
    /// stranded earlier. It repeats until nothing changes.
    async fn cancel_unreachable_dependents(&self, pipeline_id: Uuid) -> Result<(), DomainError> {
        let siblings = self.jobs.list_for_pipeline(pipeline_id).await?;
        let mut blocked: std::collections::BTreeSet<String> = siblings
            .iter()
            .filter(|j| matches!(j.status, JobStatus::Failed | JobStatus::Canceled))
            .map(|j| j.name.clone())
            .collect();

        let mut to_cancel: Vec<Uuid> = Vec::new();
        loop {
            let mut progressed = false;
            for sibling in siblings.iter().filter(|j| j.status == JobStatus::Pending) {
                if blocked.contains(&sibling.name) {
                    continue;
                }
                if sibling.needs.iter().any(|need| blocked.contains(need)) {
                    blocked.insert(sibling.name.clone());
                    to_cancel.push(sibling.id);
                    progressed = true;
                }
            }
            if !progressed {
                break;
            }
        }

        for id in to_cancel {
            if self.jobs.update_status(id, JobStatus::Canceled).await? {
                self.events
                    .publish_job_event(
                        id,
                        JobEvent::StatusChanged {
                            status: JobStatus::Canceled,
                        },
                    )
                    .await
                    .ok();
            }
        }
        Ok(())
    }

    /// Submits each newly runnable job to the engine the pipeline was created with, never the live `execution_engine`
    /// setting (hot-swap guarantee, as in `CancelPipelineUseCase`). A submission failure is reported back through
    /// `execute` itself (boxed async recursion, bounded by the job count).
    ///
    /// The recursion's `Option<Uuid>` (who to notify) is propagated up: the caller's own re-aggregation would not
    /// rediscover it, since the pipeline is already `Failed`.
    async fn submit_newly_runnable_jobs(
        &self,
        pipeline_id: Uuid,
    ) -> Result<Option<Uuid>, DomainError> {
        let pipeline = self
            .pipelines
            .find_by_id(pipeline_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("pipeline".to_string()))?;
        let executor = self.job_execution.resolve(pipeline.execution_engine);
        let mut triggered_by = None;
        for runnable in self.jobs.list_runnable(pipeline_id).await? {
            if let Err(err) = executor.submit(&runnable).await {
                tracing::error!(error = %err, job_id = %runnable.id, "failed to submit newly-runnable job to execution engine; marking it failed");
                if let Some(t) = Box::pin(self.execute(runnable.id, JobStatus::Failed)).await? {
                    triggered_by = Some(t);
                }
            }
        }
        Ok(triggered_by)
    }
}

/// Given the `Option<Uuid>` returned by `execute`, creates a `PipelineFailed` notification. Does nothing if
/// `triggered_by` is `None` or a lookup comes back empty, and never blocks the caller.
pub async fn notify_pipeline_failure(
    triggered_by: Option<Uuid>,
    pipeline_id: Uuid,
    pipelines: &Arc<dyn PipelineStorePort>,
    repositories: &Arc<dyn RepositoryStorePort>,
    users: &Arc<dyn UserRepositoryPort>,
    notifications: &Arc<dyn NotificationStorePort>,
    webhooks: &Arc<dyn WebhookDispatcherPort>,
) {
    let Some(pusher_id) = triggered_by else {
        return;
    };
    let Ok(Some(pipeline)) = pipelines.find_by_id(pipeline_id).await else {
        return;
    };
    let Ok(Some(repo)) = repositories.find_by_id(pipeline.repository_id).await else {
        return;
    };
    let Ok(Some(owner)) = users.find_by_id(repo.owner_id).await else {
        return;
    };

    webhooks
        .dispatch(
            repo.id,
            WebhookEvent::PipelineFailed {
                repository_owner: owner.username.clone(),
                repository_name: repo.name.clone(),
                pipeline_id: pipeline.id,
                commit_sha: pipeline.commit_sha.clone(),
            },
        )
        .await
        .ok();

    notifications
        .create(NewNotification {
            recipient_id: pusher_id,
            kind: NotificationKind::PipelineFailed,
            repository_owner: owner.username,
            repository_name: repo.name,
            actor_username: None,
            merge_request_id: None,
            merge_request_title: None,
            pipeline_id: Some(pipeline.id),
            commit_sha: Some(pipeline.commit_sha.clone()),
            role: None,
            issue_id: None,
            issue_number: None,
            issue_title: None,
        })
        .await
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeEvents, FakeExecution, FakeJobs, FakeNotifications, FakePipelines, FakeRepositories,
        FakeUsers, FakeWebhooks,
    };
    use async_trait::async_trait;
    use chrono::Utc;
    use ferrisgit_domain::job::Job;
    use ferrisgit_domain::pipeline::Pipeline;
    use ferrisgit_domain::repository::{Repository, RepositoryVisibility};
    use ferrisgit_domain::settings::ExecutionEngine;
    use ferrisgit_domain::user::User;
    use std::collections::BTreeMap;

    struct FailingExecution;
    #[async_trait]
    impl ferrisgit_domain::job_execution::JobExecutionPort for FailingExecution {
        async fn submit(&self, _job: &Job) -> Result<(), DomainError> {
            Err(DomainError::Infrastructure("simulated failure".to_string()))
        }
        async fn cancel(&self, _job: &Job) -> Result<(), DomainError> {
            unimplemented!()
        }
    }

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

    fn named_job(pipeline_id: Uuid, name: &str, needs: &[&str], status: JobStatus) -> Job {
        Job {
            name: name.to_string(),
            needs: needs.iter().map(|n| n.to_string()).collect(),
            ..job(pipeline_id, status)
        }
    }

    fn pipeline(pipeline_id: Uuid) -> Pipeline {
        Pipeline {
            id: pipeline_id,
            repository_id: Uuid::new_v4(),
            commit_sha: "abc".to_string(),
            execution_engine: ExecutionEngine::DockerRunners,
            status: PipelineStatus::Running,
            triggered_by: Uuid::new_v4(),
            created_at: Utc::now(),
            finished_at: None,
        }
    }

    #[tokio::test]
    async fn append_job_logs_appends_to_the_existing_log_text() {
        let existing = job(Uuid::new_v4(), JobStatus::Running);
        let jobs = Arc::new(FakeJobs::new(vec![existing.clone()]));
        let use_case = AppendJobLogsUseCase::new(jobs.clone());

        use_case.execute(existing.id, "line one\n").await.unwrap();
        use_case.execute(existing.id, "line two\n").await.unwrap();

        assert_eq!(jobs.snapshot()[0].logs, "line one\nline two\n");
    }

    #[tokio::test]
    async fn reporting_the_only_jobs_success_marks_the_pipeline_success() {
        let pipeline_id = Uuid::new_v4();
        let the_job = job(pipeline_id, JobStatus::Running);
        let jobs = Arc::new(FakeJobs::new(vec![the_job.clone()]));
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
        let events = Arc::new(FakeEvents::new());
        let execution = Arc::new(FakeExecution::new());
        let use_case = ReportJobResultUseCase::new(
            jobs,
            pipelines.clone(),
            events.clone(),
            Arc::new(JobExecutionResolver::new(
                execution.clone(),
                execution.clone(),
            )),
        );

        use_case
            .execute(the_job.id, JobStatus::Success)
            .await
            .unwrap();

        assert_eq!(pipelines.snapshot()[0].status, PipelineStatus::Success);
        assert!(
            events
                .pipeline_events()
                .iter()
                .any(|(_, e)| e.event_type() == "PipelineStatusChanged")
        );
    }

    #[tokio::test]
    async fn one_failed_job_marks_the_whole_pipeline_failed_even_if_others_succeeded() {
        let pipeline_id = Uuid::new_v4();
        let succeeded = job(pipeline_id, JobStatus::Success);
        let still_running = job(pipeline_id, JobStatus::Running);
        let jobs = Arc::new(FakeJobs::new(vec![succeeded, still_running.clone()]));
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
        let events = Arc::new(FakeEvents::new());
        let execution = Arc::new(FakeExecution::new());
        let use_case = ReportJobResultUseCase::new(
            jobs,
            pipelines.clone(),
            events.clone(),
            Arc::new(JobExecutionResolver::new(
                execution.clone(),
                execution.clone(),
            )),
        );

        let notify = use_case
            .execute(still_running.id, JobStatus::Failed)
            .await
            .unwrap();

        assert_eq!(pipelines.snapshot()[0].status, PipelineStatus::Failed);
        assert_eq!(
            notify,
            Some(pipelines.snapshot()[0].triggered_by),
            "resolving the pipeline to Failed must report its pusher for notification"
        );
    }

    #[tokio::test]
    async fn a_job_finishing_while_a_sibling_is_still_running_does_not_touch_the_pipeline() {
        let pipeline_id = Uuid::new_v4();
        let finished = job(pipeline_id, JobStatus::Running);
        let still_running = job(pipeline_id, JobStatus::Running);
        let jobs = Arc::new(FakeJobs::new(vec![finished.clone(), still_running]));
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
        let events = Arc::new(FakeEvents::new());
        let execution = Arc::new(FakeExecution::new());
        let use_case = ReportJobResultUseCase::new(
            jobs,
            pipelines.clone(),
            events.clone(),
            Arc::new(JobExecutionResolver::new(
                execution.clone(),
                execution.clone(),
            )),
        );

        use_case
            .execute(finished.id, JobStatus::Success)
            .await
            .unwrap();

        assert_eq!(
            pipelines.snapshot()[0].status,
            PipelineStatus::Running,
            "pipeline must stay Running while a sibling job is unfinished"
        );
        assert!(events.pipeline_events().is_empty());
    }

    #[tokio::test]
    async fn a_failed_job_beats_a_canceled_sibling_for_pipeline_status() {
        let pipeline_id = Uuid::new_v4();
        let canceled = job(pipeline_id, JobStatus::Canceled);
        let succeeded = job(pipeline_id, JobStatus::Success);
        let still_running = job(pipeline_id, JobStatus::Running);
        let jobs = Arc::new(FakeJobs::new(vec![
            canceled,
            succeeded,
            still_running.clone(),
        ]));
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
        let events = Arc::new(FakeEvents::new());
        let execution = Arc::new(FakeExecution::new());
        let use_case = ReportJobResultUseCase::new(
            jobs,
            pipelines.clone(),
            events.clone(),
            Arc::new(JobExecutionResolver::new(
                execution.clone(),
                execution.clone(),
            )),
        );

        use_case
            .execute(still_running.id, JobStatus::Failed)
            .await
            .unwrap();

        assert_eq!(
            pipelines.snapshot()[0].status,
            PipelineStatus::Failed,
            "Failed must win over Canceled even when both are present"
        );
    }

    #[tokio::test]
    async fn a_pipeline_with_every_job_canceled_and_no_failures_is_marked_canceled() {
        let pipeline_id = Uuid::new_v4();
        let already_canceled = job(pipeline_id, JobStatus::Canceled);
        let still_running = job(pipeline_id, JobStatus::Running);
        let jobs = Arc::new(FakeJobs::new(vec![already_canceled, still_running.clone()]));
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
        let events = Arc::new(FakeEvents::new());
        let execution = Arc::new(FakeExecution::new());
        let use_case = ReportJobResultUseCase::new(
            jobs,
            pipelines.clone(),
            events.clone(),
            Arc::new(JobExecutionResolver::new(
                execution.clone(),
                execution.clone(),
            )),
        );

        use_case
            .execute(still_running.id, JobStatus::Canceled)
            .await
            .unwrap();

        assert_eq!(pipelines.snapshot()[0].status, PipelineStatus::Canceled);
    }

    #[tokio::test]
    async fn a_failed_job_transitively_cancels_everything_downstream_and_terminates_the_pipeline() {
        let pipeline_id = Uuid::new_v4();
        let build = named_job(pipeline_id, "build", &[], JobStatus::Running);
        let test = named_job(pipeline_id, "test", &["build"], JobStatus::Pending);
        let deploy = named_job(pipeline_id, "deploy", &["test"], JobStatus::Pending);
        let jobs = Arc::new(FakeJobs::new(vec![
            build.clone(),
            test.clone(),
            deploy.clone(),
        ]));
        let pipelines = Arc::new(FakePipelines::new(vec![pipeline(pipeline_id)]));
        let events = Arc::new(FakeEvents::new());
        let execution = Arc::new(FakeExecution::new());
        let use_case = ReportJobResultUseCase::new(
            jobs.clone(),
            pipelines.clone(),
            events.clone(),
            Arc::new(JobExecutionResolver::new(
                execution.clone(),
                execution.clone(),
            )),
        );

        let notify = use_case.execute(build.id, JobStatus::Failed).await.unwrap();
        assert_eq!(notify, Some(pipelines.snapshot()[0].triggered_by));

        let stored = jobs.snapshot();
        assert_eq!(
            stored.iter().find(|j| j.id == build.id).unwrap().status,
            JobStatus::Failed
        );
        assert_eq!(
            stored.iter().find(|j| j.id == test.id).unwrap().status,
            JobStatus::Canceled,
            "a job needing the failed job can never be claimed and must be canceled"
        );
        assert_eq!(
            stored.iter().find(|j| j.id == deploy.id).unwrap().status,
            JobStatus::Canceled,
            "cancellation must propagate transitively through the needs chain, not just one hop"
        );
        assert_eq!(
            pipelines.snapshot()[0].status,
            PipelineStatus::Failed,
            "the pipeline must now reach a terminal status instead of hanging forever"
        );
        assert_eq!(
            events.job_events().len(),
            3,
            "one job event per status change, including the transitively canceled ones"
        );
    }

    #[tokio::test]
    async fn a_failed_job_does_not_cancel_an_independent_parallel_job() {
        let pipeline_id = Uuid::new_v4();
        let build = named_job(pipeline_id, "build", &[], JobStatus::Running);
        let lint = named_job(pipeline_id, "lint", &[], JobStatus::Pending);
        let jobs = Arc::new(FakeJobs::new(vec![build.clone(), lint.clone()]));
        let pipelines = Arc::new(FakePipelines::new(vec![pipeline(pipeline_id)]));
        let execution = Arc::new(FakeExecution::new());
        let use_case = ReportJobResultUseCase::new(
            jobs.clone(),
            pipelines.clone(),
            Arc::new(FakeEvents::new()),
            Arc::new(JobExecutionResolver::new(
                execution.clone(),
                execution.clone(),
            )),
        );

        use_case.execute(build.id, JobStatus::Failed).await.unwrap();

        assert_eq!(
            jobs.snapshot()
                .iter()
                .find(|j| j.id == lint.id)
                .unwrap()
                .status,
            JobStatus::Pending,
            "a job that needs nothing is still perfectly claimable"
        );
        assert_eq!(
            pipelines.snapshot()[0].status,
            PipelineStatus::Running,
            "the pipeline stays open while an independent job can still run"
        );
    }

    #[tokio::test]
    async fn a_late_report_cannot_resurrect_an_already_canceled_job_or_its_pipeline() {
        let pipeline_id = Uuid::new_v4();
        let canceled = job(pipeline_id, JobStatus::Canceled);
        let jobs = Arc::new(FakeJobs::new(vec![canceled.clone()]));
        let pipelines = Arc::new(FakePipelines::new(vec![Pipeline {
            status: PipelineStatus::Canceled,
            ..pipeline(pipeline_id)
        }]));
        let events = Arc::new(FakeEvents::new());
        let execution = Arc::new(FakeExecution::new());
        let use_case = ReportJobResultUseCase::new(
            jobs.clone(),
            pipelines.clone(),
            events.clone(),
            Arc::new(JobExecutionResolver::new(
                execution.clone(),
                execution.clone(),
            )),
        );

        use_case
            .execute(canceled.id, JobStatus::Success)
            .await
            .unwrap();

        assert_eq!(
            jobs.snapshot()[0].status,
            JobStatus::Canceled,
            "a runner's late success report must not overwrite a canceled job"
        );
        assert_eq!(
            pipelines.snapshot()[0].status,
            PipelineStatus::Canceled,
            "and must not re-run aggregation to un-cancel the pipeline"
        );
        assert!(
            events.pipeline_events().is_empty() && events.job_events().is_empty(),
            "no event should be published for a dropped late report"
        );
    }

    #[tokio::test]
    async fn reporting_a_result_for_a_job_that_does_not_exist_is_a_not_found_error() {
        let jobs = Arc::new(FakeJobs::empty());
        let execution = Arc::new(FakeExecution::new());
        let use_case = ReportJobResultUseCase::new(
            jobs,
            Arc::new(FakePipelines::empty()),
            Arc::new(FakeEvents::new()),
            Arc::new(JobExecutionResolver::new(
                execution.clone(),
                execution.clone(),
            )),
        );

        let err = use_case
            .execute(Uuid::new_v4(), JobStatus::Success)
            .await
            .unwrap_err();

        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[tokio::test]
    async fn a_successful_job_causes_its_newly_runnable_dependent_to_be_submitted() {
        let pipeline_id = Uuid::new_v4();
        let build = named_job(pipeline_id, "build", &[], JobStatus::Running);
        let test_job = named_job(pipeline_id, "test", &["build"], JobStatus::Pending);
        let jobs = Arc::new(FakeJobs::new(vec![build.clone(), test_job.clone()]));
        let pipelines = Arc::new(FakePipelines::new(vec![pipeline(pipeline_id)]));
        let execution = Arc::new(FakeExecution::new());
        let use_case = ReportJobResultUseCase::new(
            jobs,
            pipelines,
            Arc::new(FakeEvents::new()),
            Arc::new(JobExecutionResolver::new(
                execution.clone(),
                execution.clone(),
            )),
        );

        use_case
            .execute(build.id, JobStatus::Success)
            .await
            .unwrap();

        assert_eq!(
            execution.submitted(),
            vec![test_job.id],
            "test's only need (build) just succeeded, so it must now be submitted"
        );
    }

    #[tokio::test]
    async fn progression_routes_to_the_pipelines_own_engine() {
        let pipeline_id = Uuid::new_v4();
        let build = named_job(pipeline_id, "build", &[], JobStatus::Running);
        let test_job = named_job(pipeline_id, "test", &["build"], JobStatus::Pending);
        let jobs = Arc::new(FakeJobs::new(vec![build.clone(), test_job.clone()]));
        let pipelines = Arc::new(FakePipelines::new(vec![Pipeline {
            execution_engine: ferrisgit_domain::settings::ExecutionEngine::Kubernetes,
            ..pipeline(pipeline_id)
        }]));
        let docker_execution = Arc::new(FakeExecution::new());
        let kubernetes_execution = Arc::new(FakeExecution::new());
        let use_case = ReportJobResultUseCase::new(
            jobs,
            pipelines,
            Arc::new(FakeEvents::new()),
            Arc::new(JobExecutionResolver::new(
                docker_execution.clone(),
                kubernetes_execution.clone(),
            )),
        );

        use_case
            .execute(build.id, JobStatus::Success)
            .await
            .unwrap();

        assert_eq!(kubernetes_execution.submitted(), vec![test_job.id]);
        assert!(docker_execution.submitted().is_empty());
    }

    #[tokio::test]
    async fn a_failure_to_submit_a_newly_runnable_job_marks_it_failed_and_still_terminates_the_pipeline()
     {
        let pipeline_id = Uuid::new_v4();
        let build = named_job(pipeline_id, "build", &[], JobStatus::Running);
        let test_job = named_job(pipeline_id, "test", &["build"], JobStatus::Pending);
        let jobs = Arc::new(FakeJobs::new(vec![build.clone(), test_job.clone()]));
        let pipelines = Arc::new(FakePipelines::new(vec![pipeline(pipeline_id)]));
        let failing = Arc::new(FailingExecution);
        let use_case = ReportJobResultUseCase::new(
            jobs.clone(),
            pipelines.clone(),
            Arc::new(FakeEvents::new()),
            Arc::new(JobExecutionResolver::new(failing.clone(), failing.clone())),
        );

        let notify = use_case
            .execute(build.id, JobStatus::Success)
            .await
            .unwrap();

        assert_eq!(
            jobs.snapshot()
                .iter()
                .find(|j| j.id == test_job.id)
                .unwrap()
                .status,
            JobStatus::Failed
        );
        assert_eq!(pipelines.snapshot()[0].status, PipelineStatus::Failed);
        assert_eq!(
            notify,
            Some(pipelines.snapshot()[0].triggered_by),
            "the pipeline-failure notification discovered inside the recursive cascade (submission failure -> execute(Failed)) must still reach the caller of the OUTER execute() call, not be silently dropped just because the outer call's own re-aggregation now correctly sees the pipeline as already Failed"
        );
    }

    #[tokio::test]
    async fn a_pipeline_already_failed_does_not_notify_again_for_a_second_failing_job() {
        let pipeline_id = Uuid::new_v4();
        let first = named_job(pipeline_id, "build", &[], JobStatus::Failed);
        let second = named_job(pipeline_id, "lint", &[], JobStatus::Running);
        let jobs = Arc::new(FakeJobs::new(vec![first, second.clone()]));
        let pipelines = Arc::new(FakePipelines::new(vec![Pipeline {
            status: PipelineStatus::Failed,
            ..pipeline(pipeline_id)
        }]));
        let events = Arc::new(FakeEvents::new());
        let execution = Arc::new(FakeExecution::new());
        let use_case = ReportJobResultUseCase::new(
            jobs,
            pipelines.clone(),
            events.clone(),
            Arc::new(JobExecutionResolver::new(
                execution.clone(),
                execution.clone(),
            )),
        );

        let notify = use_case
            .execute(second.id, JobStatus::Failed)
            .await
            .unwrap();

        assert_eq!(
            notify, None,
            "the pipeline was already Failed before this call, so a second job failing must not trigger a second notification"
        );
        assert_eq!(
            pipelines.snapshot()[0].status,
            PipelineStatus::Failed,
            "the pipeline correctly stays Failed"
        );
    }

    #[tokio::test]
    async fn a_pipeline_resolving_to_success_reports_no_one_to_notify() {
        let pipeline_id = Uuid::new_v4();
        let the_job = job(pipeline_id, JobStatus::Running);
        let jobs = Arc::new(FakeJobs::new(vec![the_job.clone()]));
        let pipelines = Arc::new(FakePipelines::new(vec![pipeline(pipeline_id)]));
        let execution = Arc::new(FakeExecution::new());
        let use_case = ReportJobResultUseCase::new(
            jobs,
            pipelines,
            Arc::new(FakeEvents::new()),
            Arc::new(JobExecutionResolver::new(
                execution.clone(),
                execution.clone(),
            )),
        );

        let notify = use_case
            .execute(the_job.id, JobStatus::Success)
            .await
            .unwrap();

        assert_eq!(
            notify, None,
            "a pipeline resolving to Success has no failure to notify anyone about"
        );
    }

    #[tokio::test]
    async fn notify_pipeline_failure_dispatches_a_webhook_and_a_notification() {
        let repository_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let pusher_id = Uuid::new_v4();
        let pipeline_id = Uuid::new_v4();
        let pipelines = Arc::new(FakePipelines::new(vec![Pipeline {
            id: pipeline_id,
            repository_id,
            commit_sha: "abc123".to_string(),
            execution_engine: ExecutionEngine::DockerRunners,
            status: PipelineStatus::Failed,
            triggered_by: pusher_id,
            created_at: Utc::now(),
            finished_at: None,
        }]));
        let repositories = Arc::new(FakeRepositories::new(vec![Repository {
            id: repository_id,
            owner_id,
            name: "hello".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "hello.git".to_string(),
            visibility: RepositoryVisibility::Private,
            created_at: Utc::now(),
        }]));
        let users = Arc::new(FakeUsers::new(vec![User {
            id: owner_id,
            username: "owner".to_string(),
            email: "owner@example.com".to_string(),
            password_hash: "h".to_string(),
            is_admin: false,
            created_at: Utc::now(),
        }]));
        let notifications = Arc::new(FakeNotifications::empty());
        let webhooks = Arc::new(FakeWebhooks::default());

        notify_pipeline_failure(
            Some(pusher_id),
            pipeline_id,
            &(pipelines as Arc<dyn PipelineStorePort>),
            &(repositories as Arc<dyn RepositoryStorePort>),
            &(users as Arc<dyn UserRepositoryPort>),
            &(notifications.clone() as Arc<dyn NotificationStorePort>),
            &(webhooks.clone() as Arc<dyn WebhookDispatcherPort>),
        )
        .await;

        assert_eq!(notifications.snapshot().len(), 1);
        let dispatched = webhooks.dispatched();
        assert_eq!(dispatched.len(), 1);
        assert_eq!(dispatched[0].0, repository_id);
        assert!(
            matches!(&dispatched[0].1, WebhookEvent::PipelineFailed { commit_sha, .. } if commit_sha == "abc123")
        );
    }

    #[tokio::test]
    async fn notify_pipeline_failure_dispatches_nothing_when_triggered_by_is_none() {
        let pipelines = Arc::new(FakePipelines::empty());
        let repositories = Arc::new(FakeRepositories::new(vec![Repository {
            id: Uuid::new_v4(),
            owner_id: Uuid::new_v4(),
            name: "hello".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "hello.git".to_string(),
            visibility: RepositoryVisibility::Private,
            created_at: Utc::now(),
        }]));
        let users = Arc::new(FakeUsers::new(vec![User {
            id: Uuid::new_v4(),
            username: "owner".to_string(),
            email: "owner@example.com".to_string(),
            password_hash: "h".to_string(),
            is_admin: false,
            created_at: Utc::now(),
        }]));
        let notifications = Arc::new(FakeNotifications::empty());
        let webhooks = Arc::new(FakeWebhooks::default());

        notify_pipeline_failure(
            None,
            Uuid::new_v4(),
            &(pipelines as Arc<dyn PipelineStorePort>),
            &(repositories as Arc<dyn RepositoryStorePort>),
            &(users as Arc<dyn UserRepositoryPort>),
            &(notifications.clone() as Arc<dyn NotificationStorePort>),
            &(webhooks.clone() as Arc<dyn WebhookDispatcherPort>),
        )
        .await;

        assert!(notifications.snapshot().is_empty());
        assert!(webhooks.dispatched().is_empty());
    }
}
