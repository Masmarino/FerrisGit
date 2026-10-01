use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::{JobStatus, JobStorePort, NewJob};
use ferrisgit_domain::pipeline::{NewPipeline, Pipeline, PipelineStatus, PipelineStorePort};
use ferrisgit_domain::pipeline_definition::parse_pipeline_definition;
use ferrisgit_domain::pipeline_events::{JobEvent, PipelineEvent, PipelineEventPublisherPort};
use ferrisgit_domain::pipeline_file_reader::PipelineFileReaderPort;
use ferrisgit_domain::settings::{RepositorySettingsStorePort, SystemSettingsStorePort};
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use uuid::Uuid;

use crate::job_execution_resolver::JobExecutionResolver;
use crate::use_cases::report_job_result::ReportJobResultUseCase;

pub struct CreatePipelineUseCase {
    repository_settings: Arc<dyn RepositorySettingsStorePort>,
    system_settings: Arc<dyn SystemSettingsStorePort>,
    pipelines: Arc<dyn PipelineStorePort>,
    jobs: Arc<dyn JobStorePort>,
    file_reader: Arc<dyn PipelineFileReaderPort>,
    events: Arc<dyn PipelineEventPublisherPort>,
    job_execution: Arc<JobExecutionResolver>,
    repositories: Arc<dyn ferrisgit_domain::repository::RepositoryStorePort>,
    users: Arc<dyn ferrisgit_domain::user::UserRepositoryPort>,
    notifications: Arc<dyn ferrisgit_domain::notification::NotificationStorePort>,
    webhooks: Arc<dyn WebhookDispatcherPort>,
}

impl CreatePipelineUseCase {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repository_settings: Arc<dyn RepositorySettingsStorePort>,
        system_settings: Arc<dyn SystemSettingsStorePort>,
        pipelines: Arc<dyn PipelineStorePort>,
        jobs: Arc<dyn JobStorePort>,
        file_reader: Arc<dyn PipelineFileReaderPort>,
        events: Arc<dyn PipelineEventPublisherPort>,
        job_execution: Arc<JobExecutionResolver>,
        repositories: Arc<dyn ferrisgit_domain::repository::RepositoryStorePort>,
        users: Arc<dyn ferrisgit_domain::user::UserRepositoryPort>,
        notifications: Arc<dyn ferrisgit_domain::notification::NotificationStorePort>,
        webhooks: Arc<dyn WebhookDispatcherPort>,
    ) -> Self {
        Self {
            repository_settings,
            system_settings,
            pipelines,
            jobs,
            file_reader,
            events,
            job_execution,
            repositories,
            users,
            notifications,
            webhooks,
        }
    }

    /// `Ok(None)` when there is nothing to run: CI disabled, or the commit doesn't touch the pipeline file. `Err` for a
    /// malformed pipeline file, which the pusher should hear about. The caller surfaces it, since the push itself
    /// already succeeded.
    pub async fn execute(
        &self,
        repository_id: Uuid,
        repository_disk_path: &str,
        commit_sha: &str,
        triggered_by: Uuid,
    ) -> Result<Option<Pipeline>, DomainError> {
        let settings = self
            .repository_settings
            .get_or_create_default(repository_id)
            .await?;
        if !settings.ci_enabled {
            return Ok(None);
        }

        let Some(yaml_bytes) = self
            .file_reader
            .read_file_at_revision(
                repository_disk_path,
                commit_sha,
                &settings.pipeline_file_path,
            )
            .await?
        else {
            return Ok(None);
        };
        let yaml = String::from_utf8(yaml_bytes).map_err(|e| {
            DomainError::Validation(format!("pipeline file is not valid UTF-8: {e}"))
        })?;
        let definition =
            parse_pipeline_definition(&yaml).map_err(|e| DomainError::Validation(e.to_string()))?;

        let engine = self.system_settings.get().await?.execution_engine;
        let executor = self.job_execution.resolve(engine);

        let pipeline = self
            .pipelines
            .create(NewPipeline {
                repository_id,
                commit_sha: commit_sha.to_string(),
                execution_engine: engine,
                triggered_by,
            })
            .await?;
        self.events
            .publish_pipeline_event(
                pipeline.id,
                PipelineEvent::StatusChanged {
                    status: PipelineStatus::Pending,
                },
            )
            .await
            .ok();

        // Stage order, then name: jobs are listed by creation time, and that is the only place
        // the pipeline page can recover the `stages` order from (it isn't stored anywhere else).
        let mut ordered_jobs: Vec<_> = definition.jobs.iter().collect();
        ordered_jobs.sort_by_key(|(job_name, job_def)| {
            (
                definition.stages.iter().position(|s| s == &job_def.stage),
                job_name.as_str(),
            )
        });

        for (job_name, job_def) in ordered_jobs {
            let job = self
                .jobs
                .create(NewJob {
                    pipeline_id: pipeline.id,
                    stage: job_def.stage.clone(),
                    name: job_name.clone(),
                    image: job_def.image.clone(),
                    script: job_def.script.clone(),
                    variables: job_def.variables.clone(),
                    needs: job_def.needs.clone(),
                    tags: job_def.tags.clone(),
                    cache: job_def.cache.clone(),
                })
                .await?;
            self.events
                .publish_job_event(
                    job.id,
                    JobEvent::StatusChanged {
                        status: JobStatus::Pending,
                    },
                )
                .await
                .ok();

            // Only a job without `needs` can run this early. `ReportJobResultUseCase` (through `list_runnable`) picks
            // up later stages as their dependencies finish.
            if job.needs.is_empty()
                && let Err(err) = executor.submit(&job).await
            {
                tracing::error!(error = %err, job_id = %job.id, "failed to submit job to execution engine; marking it failed");
                let report = ReportJobResultUseCase::new(
                    self.jobs.clone(),
                    self.pipelines.clone(),
                    self.events.clone(),
                    self.job_execution.clone(),
                );
                let notify = report.execute(job.id, JobStatus::Failed).await?;
                crate::use_cases::report_job_result::notify_pipeline_failure(
                    notify,
                    pipeline.id,
                    &self.pipelines,
                    &self.repositories,
                    &self.users,
                    &self.notifications,
                    &self.webhooks,
                )
                .await;
            }
        }

        Ok(Some(pipeline))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeEvents, FakeExecution, FakeFileReader, FakeJobs, FakeNotifications, FakePipelines,
        FakeRepositories, FakeRepositorySettings, FakeUsers, FakeWebhooks,
    };
    use async_trait::async_trait;
    use chrono::Utc;
    use ferrisgit_domain::job::Job;
    use ferrisgit_domain::job_execution::JobExecutionPort;
    use ferrisgit_domain::repository::{Repository, RepositoryVisibility};
    use ferrisgit_domain::settings::RepositorySettings;
    use ferrisgit_domain::user::User;

    fn repository(id: Uuid) -> Repository {
        Repository {
            id,
            owner_id: Uuid::new_v4(),
            name: "hello".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "hello.git".to_string(),
            visibility: RepositoryVisibility::Private,
            created_at: Utc::now(),
        }
    }

    fn user(id: Uuid) -> User {
        User {
            id,
            username: "florian".to_string(),
            email: "florian@example.com".to_string(),
            password_hash: "hash".to_string(),
            is_admin: false,
            created_at: Utc::now(),
        }
    }

    fn default_settings(repository_id: Uuid) -> RepositorySettings {
        RepositorySettings {
            repository_id,
            pipeline_file_path: ".ferrisgit-ci.yml".to_string(),
            ci_enabled: true,
            required_approvals: 0,
        }
    }

    struct FakeSystemSettingsWithEngine(ferrisgit_domain::settings::ExecutionEngine);
    impl Default for FakeSystemSettingsWithEngine {
        fn default() -> Self {
            Self(ferrisgit_domain::settings::ExecutionEngine::DockerRunners)
        }
    }
    #[async_trait]
    impl SystemSettingsStorePort for FakeSystemSettingsWithEngine {
        async fn get(&self) -> Result<ferrisgit_domain::settings::SystemSettings, DomainError> {
            Ok(ferrisgit_domain::settings::SystemSettings {
                execution_engine: self.0,
                k8s_namespace: None,
                k8s_cache_storage_class: None,
                runner_registration_token: None,
                log_retention_days: None,
                max_concurrent_jobs: None,
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

    #[tokio::test]
    async fn ci_disabled_returns_none_without_touching_anything_else() {
        let repository_id = Uuid::new_v4();
        let execution = Arc::new(FakeExecution::new());
        let use_case = CreatePipelineUseCase::new(
            Arc::new(FakeRepositorySettings::new(RepositorySettings {
                ci_enabled: false,
                ..default_settings(repository_id)
            })),
            Arc::new(FakeSystemSettingsWithEngine::default()),
            Arc::new(FakePipelines::empty()),
            Arc::new(FakeJobs::empty()),
            Arc::new(FakeFileReader::new(Some(b"stages: []".to_vec()))),
            Arc::new(FakeEvents::new()),
            Arc::new(JobExecutionResolver::new(
                execution.clone(),
                execution.clone(),
            )),
            Arc::new(FakeRepositories::new(vec![repository(Uuid::new_v4())])),
            Arc::new(FakeUsers::new(vec![user(Uuid::new_v4())])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(repository_id, "path", "sha", Uuid::new_v4())
            .await
            .unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn a_commit_with_no_pipeline_file_returns_none() {
        let repository_id = Uuid::new_v4();
        let execution = Arc::new(FakeExecution::new());
        let use_case = CreatePipelineUseCase::new(
            Arc::new(FakeRepositorySettings::new(default_settings(repository_id))),
            Arc::new(FakeSystemSettingsWithEngine::default()),
            Arc::new(FakePipelines::empty()),
            Arc::new(FakeJobs::empty()),
            Arc::new(FakeFileReader::none()),
            Arc::new(FakeEvents::new()),
            Arc::new(JobExecutionResolver::new(
                execution.clone(),
                execution.clone(),
            )),
            Arc::new(FakeRepositories::new(vec![repository(Uuid::new_v4())])),
            Arc::new(FakeUsers::new(vec![user(Uuid::new_v4())])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(repository_id, "path", "sha", Uuid::new_v4())
            .await
            .unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn a_malformed_pipeline_file_is_a_validation_error() {
        let repository_id = Uuid::new_v4();
        let execution = Arc::new(FakeExecution::new());
        let use_case = CreatePipelineUseCase::new(
            Arc::new(FakeRepositorySettings::new(default_settings(repository_id))),
            Arc::new(FakeSystemSettingsWithEngine::default()),
            Arc::new(FakePipelines::empty()),
            Arc::new(FakeJobs::empty()),
            Arc::new(FakeFileReader::new(Some(b"not: [valid, yaml".to_vec()))),
            Arc::new(FakeEvents::new()),
            Arc::new(JobExecutionResolver::new(
                execution.clone(),
                execution.clone(),
            )),
            Arc::new(FakeRepositories::new(vec![repository(Uuid::new_v4())])),
            Arc::new(FakeUsers::new(vec![user(Uuid::new_v4())])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(repository_id, "path", "sha", Uuid::new_v4())
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn a_valid_pipeline_file_creates_the_pipeline_and_submits_every_job() {
        let repository_id = Uuid::new_v4();
        let yaml = b"stages: [build]\njobs:\n  compile:\n    stage: build\n    image: rust:1.82\n    script: [\"cargo build\"]\n".to_vec();
        let pipelines = Arc::new(FakePipelines::empty());
        let jobs = Arc::new(FakeJobs::empty());
        let events = Arc::new(FakeEvents::new());
        let execution = Arc::new(FakeExecution::new());
        let use_case = CreatePipelineUseCase::new(
            Arc::new(FakeRepositorySettings::new(default_settings(repository_id))),
            Arc::new(FakeSystemSettingsWithEngine::default()),
            pipelines.clone(),
            jobs.clone(),
            Arc::new(FakeFileReader::new(Some(yaml))),
            events.clone(),
            Arc::new(JobExecutionResolver::new(
                execution.clone(),
                execution.clone(),
            )),
            Arc::new(FakeRepositories::new(vec![repository(Uuid::new_v4())])),
            Arc::new(FakeUsers::new(vec![user(Uuid::new_v4())])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(repository_id, "path", "abc123", Uuid::new_v4())
            .await
            .unwrap();

        let pipeline = result.expect("a valid pipeline file must produce a pipeline");
        assert_eq!(pipeline.commit_sha, "abc123");
        assert_eq!(jobs.snapshot().len(), 1);
        assert_eq!(
            execution.submitted().len(),
            1,
            "the created job must be submitted to the execution engine"
        );
        assert!(
            events
                .pipeline_events()
                .iter()
                .any(|(_, e)| e.event_type() == "PipelineStatusChanged")
        );
        assert!(
            events
                .job_events()
                .iter()
                .any(|(_, e)| e.event_type() == "JobStatusChanged")
        );
    }

    #[tokio::test]
    async fn the_created_pipeline_is_tagged_with_the_currently_active_engine() {
        let repository_id = Uuid::new_v4();
        let yaml = b"stages: [build]\njobs:\n  compile:\n    stage: build\n    image: rust:1.82\n    script: [\"cargo build\"]\n".to_vec();
        let execution = Arc::new(FakeExecution::new());
        let use_case = CreatePipelineUseCase::new(
            Arc::new(FakeRepositorySettings::new(default_settings(repository_id))),
            Arc::new(FakeSystemSettingsWithEngine(
                ferrisgit_domain::settings::ExecutionEngine::Kubernetes,
            )),
            Arc::new(FakePipelines::empty()),
            Arc::new(FakeJobs::empty()),
            Arc::new(FakeFileReader::new(Some(yaml))),
            Arc::new(FakeEvents::new()),
            Arc::new(JobExecutionResolver::new(
                execution.clone(),
                execution.clone(),
            )),
            Arc::new(FakeRepositories::new(vec![repository(Uuid::new_v4())])),
            Arc::new(FakeUsers::new(vec![user(Uuid::new_v4())])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let pipeline = use_case
            .execute(repository_id, "path", "abc123", Uuid::new_v4())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            pipeline.execution_engine,
            ferrisgit_domain::settings::ExecutionEngine::Kubernetes
        );
    }

    #[tokio::test]
    async fn jobs_are_created_in_stage_order_not_alphabetical_order() {
        // Creation order must follow `stages`, not the alphabetical BTreeMap order: the pipeline page derives stage
        // order from creation time.
        let repository_id = Uuid::new_v4();
        let yaml = b"stages: [prepare, check]\njobs:\n  a-check:\n    stage: check\n    image: alpine:3.20\n    script: [\"true\"]\n  z-prepare:\n    stage: prepare\n    image: alpine:3.20\n    script: [\"true\"]\n".to_vec();
        let jobs = Arc::new(FakeJobs::empty());
        let execution = Arc::new(FakeExecution::new());
        let use_case = CreatePipelineUseCase::new(
            Arc::new(FakeRepositorySettings::new(default_settings(repository_id))),
            Arc::new(FakeSystemSettingsWithEngine(
                ferrisgit_domain::settings::ExecutionEngine::DockerRunners,
            )),
            Arc::new(FakePipelines::empty()),
            jobs.clone(),
            Arc::new(FakeFileReader::new(Some(yaml))),
            Arc::new(FakeEvents::new()),
            Arc::new(JobExecutionResolver::new(
                execution.clone(),
                execution.clone(),
            )),
            Arc::new(FakeRepositories::new(vec![repository(Uuid::new_v4())])),
            Arc::new(FakeUsers::new(vec![user(Uuid::new_v4())])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        use_case
            .execute(repository_id, "path", "abc123", Uuid::new_v4())
            .await
            .unwrap();

        let stages: Vec<String> = jobs.snapshot().into_iter().map(|j| j.stage).collect();
        assert_eq!(stages, vec!["prepare", "check"]);
    }

    #[tokio::test]
    async fn a_job_with_unmet_needs_is_not_submitted_at_creation_time() {
        let repository_id = Uuid::new_v4();
        let yaml = b"stages: [build, test]\njobs:\n  compile:\n    stage: build\n    image: rust:1.82\n    script: [\"cargo build\"]\n  unit-tests:\n    stage: test\n    image: rust:1.82\n    script: [\"cargo test\"]\n    needs: [compile]\n".to_vec();
        let execution = Arc::new(FakeExecution::new());
        let use_case = CreatePipelineUseCase::new(
            Arc::new(FakeRepositorySettings::new(default_settings(repository_id))),
            Arc::new(FakeSystemSettingsWithEngine(
                ferrisgit_domain::settings::ExecutionEngine::DockerRunners,
            )),
            Arc::new(FakePipelines::empty()),
            Arc::new(FakeJobs::empty()),
            Arc::new(FakeFileReader::new(Some(yaml))),
            Arc::new(FakeEvents::new()),
            Arc::new(JobExecutionResolver::new(
                execution.clone(),
                execution.clone(),
            )),
            Arc::new(FakeRepositories::new(vec![repository(Uuid::new_v4())])),
            Arc::new(FakeUsers::new(vec![user(Uuid::new_v4())])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        use_case
            .execute(repository_id, "path", "abc123", Uuid::new_v4())
            .await
            .unwrap();

        assert_eq!(
            execution.submitted().len(),
            1,
            "only the needs-less job (compile) should be submitted; unit-tests must wait"
        );
    }

    #[tokio::test]
    async fn a_submission_failure_marks_the_job_failed_instead_of_aborting_pipeline_creation() {
        let repository_id = Uuid::new_v4();
        let yaml = b"stages: [build]\njobs:\n  compile:\n    stage: build\n    image: rust:1.82\n    script: [\"cargo build\"]\n".to_vec();
        let pipelines = Arc::new(FakePipelines::empty());
        let jobs = Arc::new(FakeJobs::empty());
        let events = Arc::new(FakeEvents::new());
        let failing_execution = Arc::new(FailingExecution);
        let notifications = Arc::new(FakeNotifications::empty());
        let repo = repository(repository_id);
        // `notify_pipeline_failure` resolves the owner via `find_by_id(repo.owner_id)`: seed that exact id.
        let owner_id = repo.owner_id;
        let use_case = CreatePipelineUseCase::new(
            Arc::new(FakeRepositorySettings::new(default_settings(repository_id))),
            Arc::new(FakeSystemSettingsWithEngine(
                ferrisgit_domain::settings::ExecutionEngine::Kubernetes,
            )),
            pipelines.clone(),
            jobs.clone(),
            Arc::new(FakeFileReader::new(Some(yaml))),
            events.clone(),
            Arc::new(JobExecutionResolver::new(
                failing_execution.clone(),
                failing_execution.clone(),
            )),
            Arc::new(FakeRepositories::new(vec![repo])),
            Arc::new(FakeUsers::new(vec![user(owner_id)])),
            notifications.clone(),
            Arc::new(FakeWebhooks::default()),
        );

        let _pipeline = use_case
            .execute(repository_id, "path", "abc123", Uuid::new_v4())
            .await
            .unwrap()
            .expect("the pipeline row itself must still be created");

        assert_eq!(
            jobs.snapshot()[0].status,
            JobStatus::Failed,
            "a job whose submission fails must end up Failed, not stuck Pending forever"
        );
        assert_eq!(
            pipelines.snapshot()[0].status,
            PipelineStatus::Failed,
            "the pipeline must reach a terminal status too, via the same aggregation ReportJobResultUseCase already does"
        );
        assert_eq!(
            notifications.snapshot().len(),
            1,
            "the submission-failure path must notify the pipeline's pusher exactly once"
        );
    }

    struct FailingExecution;
    #[async_trait]
    impl JobExecutionPort for FailingExecution {
        async fn submit(&self, _job: &Job) -> Result<(), DomainError> {
            Err(DomainError::Infrastructure(
                "simulated Kubernetes API failure".to_string(),
            ))
        }
        async fn cancel(&self, _job: &Job) -> Result<(), DomainError> {
            unimplemented!()
        }
    }
}
