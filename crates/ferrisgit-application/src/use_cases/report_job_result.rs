use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::{JobStatus, JobStorePort, unreachable_jobs};
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

    /// `status` must be terminal. Once every job is terminal the pipeline follows: failed if any job failed, else
    /// canceled if any was canceled, else failed if any was skipped, else success.
    ///
    /// A report for a job that is already terminal is dropped, so a runner finishing late can't un-cancel a canceled
    /// pipeline. The store enforces that atomically.
    ///
    /// A failed or canceled job also skips everything that can no longer start because of it (its dependents through
    /// `needs` and stage barriers, transitively). Nothing would claim those, and the pipeline would never finish.
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
            self.skip_unreachable_jobs(job.pipeline_id).await?;
        }

        let cascade_triggered_by = if status == JobStatus::Success {
            self.submit_newly_runnable_jobs(job.pipeline_id).await?
        } else {
            None
        };

        let siblings = self.jobs.list_for_pipeline(job.pipeline_id).await?;
        if !siblings.iter().all(|j| j.status.is_terminal()) {
            return Ok(None);
        }

        let pipeline_status = if siblings.iter().any(|j| j.status == JobStatus::Failed) {
            PipelineStatus::Failed
        } else if siblings.iter().any(|j| j.status == JobStatus::Canceled) {
            PipelineStatus::Canceled
        } else if siblings.iter().any(|j| j.status == JobStatus::Skipped) {
            PipelineStatus::Failed
        } else {
            PipelineStatus::Success
        };

        // Only the first failure notifies, re-aggregating an already failed pipeline stays quiet.
        let triggered_by = if pipeline_status == PipelineStatus::Failed {
            self.pipelines
                .find_by_id(job.pipeline_id)
                .await
                .ok()
                .flatten()
                .filter(|p| p.status != PipelineStatus::Failed)
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

    /// Looks at the whole pipeline, not just the reported job, so it can be repeated and also catches jobs stranded
    /// earlier.
    async fn skip_unreachable_jobs(&self, pipeline_id: Uuid) -> Result<(), DomainError> {
        let siblings = self.jobs.list_for_pipeline(pipeline_id).await?;
        let to_skip: Vec<Uuid> = unreachable_jobs(&siblings)
            .into_iter()
            .map(|j| j.id)
            .collect();

        for id in to_skip {
            if self.jobs.update_status(id, JobStatus::Skipped).await? {
                self.events
                    .publish_job_event(
                        id,
                        JobEvent::StatusChanged {
                            status: JobStatus::Skipped,
                        },
                    )
                    .await
                    .ok();
            }
        }
        Ok(())
    }

    /// Uses the engine the pipeline was created with, not the live setting, so switching engines doesn't affect running
    /// pipelines. A job that fails to submit is reported as failed through `execute` (recursion, bounded by the job
    /// count). Who to notify is passed up, since the caller's own aggregation would see the pipeline as already failed.
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

/// Flips a pending pipeline to running when its first job starts, whichever engine started it. Already running or
/// finished pipelines are left alone and nothing is published.
pub async fn mark_pipeline_running(
    pipelines: &Arc<dyn PipelineStorePort>,
    events: &Arc<dyn PipelineEventPublisherPort>,
    pipeline_id: Uuid,
) -> Result<(), DomainError> {
    if pipelines.mark_running(pipeline_id).await? {
        events
            .publish_pipeline_event(
                pipeline_id,
                PipelineEvent::StatusChanged {
                    status: PipelineStatus::Running,
                },
            )
            .await
            .ok();
    }
    Ok(())
}

/// Takes what `execute` returned and sends the pipeline-failed webhook and notification. Best effort: it gives up
/// silently on `None` or a missing record and never fails the caller.
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
mod tests;
