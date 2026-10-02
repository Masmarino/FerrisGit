use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::{JobStatus, JobStorePort, NewJob};
use ferrisgit_domain::notification::NotificationStorePort;
use ferrisgit_domain::pipeline::{NewPipeline, Pipeline, PipelineStatus, PipelineStorePort};
use ferrisgit_domain::pipeline_definition::parse_pipeline_definition;
use ferrisgit_domain::pipeline_events::{JobEvent, PipelineEvent, PipelineEventPublisherPort};
use ferrisgit_domain::pipeline_file_reader::PipelineFileReaderPort;
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::settings::{RepositorySettingsStorePort, SystemSettingsStorePort};
use ferrisgit_domain::user::UserRepositoryPort;
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use uuid::Uuid;

use crate::job_execution_resolver::JobExecutionResolver;
use crate::use_cases::report_job_result::{ReportJobResultUseCase, notify_pipeline_failure};

pub struct CreatePipelineUseCase {
    repository_settings: Arc<dyn RepositorySettingsStorePort>,
    system_settings: Arc<dyn SystemSettingsStorePort>,
    pipelines: Arc<dyn PipelineStorePort>,
    jobs: Arc<dyn JobStorePort>,
    file_reader: Arc<dyn PipelineFileReaderPort>,
    events: Arc<dyn PipelineEventPublisherPort>,
    job_execution: Arc<JobExecutionResolver>,
    repositories: Arc<dyn RepositoryStorePort>,
    users: Arc<dyn UserRepositoryPort>,
    notifications: Arc<dyn NotificationStorePort>,
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
        repositories: Arc<dyn RepositoryStorePort>,
        users: Arc<dyn UserRepositoryPort>,
        notifications: Arc<dyn NotificationStorePort>,
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

    /// `Ok(None)` when there's nothing to run (CI off, or no pipeline file). An invalid file still gives a pipeline,
    /// failed from the start with the parser's message and no jobs, so the pusher sees why. `Err` is for real failures.
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
        let engine = self.system_settings.get().await?.execution_engine;
        let new_pipeline = NewPipeline {
            repository_id,
            commit_sha: commit_sha.to_string(),
            execution_engine: engine,
            triggered_by,
        };

        let definition = match String::from_utf8(yaml_bytes) {
            Ok(yaml) => parse_pipeline_definition(&yaml).map_err(|e| e.to_string()),
            Err(e) => Err(format!("pipeline file is not valid UTF-8: {e}")),
        };
        let definition = match definition {
            Ok(definition) => definition,
            Err(message) => return self.record_invalid_pipeline(new_pipeline, &message).await,
        };

        let executor = self.job_execution.resolve(engine);

        let pipeline = self.pipelines.create(new_pipeline).await?;
        self.events
            .publish_pipeline_event(
                pipeline.id,
                PipelineEvent::StatusChanged {
                    status: PipelineStatus::Pending,
                },
            )
            .await
            .ok();

        // Create in stage order, then by name. Jobs are listed by creation time, and that's the only place the
        // pipeline page can read the stage order from.
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
        }

        // Same rule as for every later release: the report-result use case asks `list_runnable` again as jobs succeed.
        for job in self.jobs.list_runnable(pipeline.id).await? {
            if let Err(err) = executor.submit(&job).await {
                tracing::error!(error = %err, job_id = %job.id, "failed to submit job to execution engine; marking it failed");
                let report = ReportJobResultUseCase::new(
                    self.jobs.clone(),
                    self.pipelines.clone(),
                    self.events.clone(),
                    self.job_execution.clone(),
                );
                let notify = report.execute(job.id, JobStatus::Failed).await?;
                notify_pipeline_failure(
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

    async fn record_invalid_pipeline(
        &self,
        new_pipeline: NewPipeline,
        message: &str,
    ) -> Result<Option<Pipeline>, DomainError> {
        let pipeline = self.pipelines.create_failed(new_pipeline, message).await?;
        self.events
            .publish_pipeline_event(
                pipeline.id,
                PipelineEvent::StatusChanged {
                    status: PipelineStatus::Failed,
                },
            )
            .await
            .ok();
        notify_pipeline_failure(
            Some(pipeline.triggered_by),
            pipeline.id,
            &self.pipelines,
            &self.repositories,
            &self.users,
            &self.notifications,
            &self.webhooks,
        )
        .await;
        Ok(Some(pipeline))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeEvents, FakeExecution, FakeFileReader, FakeJobs, FakeNotifications, FakePipelines,
        FakeRepositories, FakeRepositorySettings, FakeSystemSettings, FakeUsers, FakeWebhooks,
    };
    use crate::use_cases::fixtures::{repository, system_settings, user};
    use async_trait::async_trait;
    use ferrisgit_domain::job::Job;
    use ferrisgit_domain::job_execution::JobExecutionPort;
    use ferrisgit_domain::settings::{ExecutionEngine, RepositorySettings, SystemSettings};
    use ferrisgit_domain::user::User;

    const COMPILE_ONLY_YAML: &[u8] = b"stages: [build]\njobs:\n  compile:\n    stage: build\n    image: rust:1.82\n    script: [\"cargo build\"]\n";

    /// What a test can change: the pipeline file in the commit, CI on or off, the active engine and, unless `executor`
    /// replaces it, a recording `FakeExecution` behind both engines.
    struct Config {
        file: Option<Vec<u8>>,
        ci_enabled: bool,
        engine: ExecutionEngine,
        executor: Option<Arc<dyn JobExecutionPort>>,
    }

    impl Config {
        fn with_file(file: &[u8]) -> Self {
            Self {
                file: Some(file.to_vec()),
                ci_enabled: true,
                engine: ExecutionEngine::DockerRunners,
                executor: None,
            }
        }
    }

    struct Setup {
        use_case: CreatePipelineUseCase,
        repository_id: Uuid,
        pipelines: Arc<FakePipelines>,
        jobs: Arc<FakeJobs>,
        events: Arc<FakeEvents>,
        execution: Arc<FakeExecution>,
        notifications: Arc<FakeNotifications>,
    }

    fn setup(config: Config) -> Setup {
        let repo = repository(Uuid::new_v4());
        let repository_id = repo.id;
        // The failure notification looks the owner up by `repo.owner_id`, so seed that exact id.
        let owner = User {
            id: repo.owner_id,
            ..user("florian")
        };
        let pipelines = Arc::new(FakePipelines::empty());
        let jobs = Arc::new(FakeJobs::empty());
        let events = Arc::new(FakeEvents::new());
        let execution = Arc::new(FakeExecution::new());
        let notifications = Arc::new(FakeNotifications::empty());
        let executor = config.executor.unwrap_or_else(|| execution.clone());
        let use_case = CreatePipelineUseCase::new(
            Arc::new(FakeRepositorySettings::new(RepositorySettings {
                repository_id,
                pipeline_file_path: ".ferrisgit-ci.yml".to_string(),
                ci_enabled: config.ci_enabled,
                required_approvals: 0,
            })),
            Arc::new(FakeSystemSettings::new(SystemSettings {
                execution_engine: config.engine,
                ..system_settings()
            })),
            pipelines.clone(),
            jobs.clone(),
            Arc::new(FakeFileReader::new(config.file)),
            events.clone(),
            Arc::new(JobExecutionResolver::new(executor.clone(), executor)),
            Arc::new(FakeRepositories::new(vec![repo])),
            Arc::new(FakeUsers::new(vec![owner])),
            notifications.clone(),
            Arc::new(FakeWebhooks::default()),
        );
        Setup {
            use_case,
            repository_id,
            pipelines,
            jobs,
            events,
            execution,
            notifications,
        }
    }

    fn setup_with_file(file: Option<Vec<u8>>) -> Setup {
        setup(Config {
            file,
            ..Config::with_file(b"")
        })
    }

    async fn push(setup: &Setup) -> Option<Pipeline> {
        setup
            .use_case
            .execute(setup.repository_id, "path", "abc123", Uuid::new_v4())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn ci_disabled_returns_none_without_touching_anything_else() {
        let setup = setup(Config {
            ci_enabled: false,
            ..Config::with_file(b"stages: []")
        });

        assert!(push(&setup).await.is_none());

        assert!(setup.pipelines.snapshot().is_empty());
        assert!(setup.jobs.snapshot().is_empty());
    }

    #[tokio::test]
    async fn a_commit_with_no_pipeline_file_returns_none() {
        let setup = setup_with_file(None);

        assert!(push(&setup).await.is_none());
    }

    #[tokio::test]
    async fn a_malformed_pipeline_file_still_creates_a_failed_pipeline_carrying_the_parser_message()
    {
        let setup = setup_with_file(Some(b"not: [valid, yaml".to_vec()));

        let pipeline = push(&setup)
            .await
            .expect("an invalid file must still produce a pipeline the pusher can see");

        assert_eq!(pipeline.status, PipelineStatus::Failed);
        assert_eq!(pipeline.commit_sha, "abc123");
        assert!(
            pipeline
                .error
                .as_deref()
                .unwrap()
                .starts_with("invalid YAML"),
            "the message is the parser's, got {:?}",
            pipeline.error
        );
        let stored = setup.pipelines.snapshot();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].status, PipelineStatus::Failed);
        assert_eq!(stored[0].error, pipeline.error);
        assert!(setup.jobs.snapshot().is_empty(), "no job is created");
        assert!(setup.execution.submitted().is_empty());
    }

    #[tokio::test]
    async fn an_invalid_pipeline_announces_its_failure_and_notifies_the_pusher() {
        let setup = setup_with_file(Some(b"stages: [a]\njobs: {}\nbogus".to_vec()));

        push(&setup).await.unwrap();

        let events = setup.events.pipeline_events();
        assert_eq!(events.len(), 1);
        let (_, PipelineEvent::StatusChanged { status }) = &events[0];
        assert_eq!(*status, PipelineStatus::Failed);
        assert_eq!(setup.notifications.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn each_kind_of_invalid_file_reports_its_own_message() {
        let cases: [(&str, &str); 4] = [
            (
                "stages: [build]\njobs:\n  a:\n    stage: deploy\n    image: alpine\n    script: [\"true\"]\n",
                "stage 'deploy'",
            ),
            (
                "stages: [build]\njobs:\n  a:\n    stage: build\n    image: alpine\n    script: [\"true\"]\n    needs: [ghost]\n",
                "ghost",
            ),
            (
                "stages: [build]\njobs:\n  a:\n    stage: build\n    image: alpine\n    script: [\"true\"]\n    needs: [b]\n  b:\n    stage: build\n    image: alpine\n    script: [\"true\"]\n    needs: [a]\n",
                "a -> b -> a",
            ),
            (
                "stages: [build]\njobs:\n  a:\n    stage: build\n    image: alpine\n    script: [\"true\"]\n    cache: [\"Bad Key\"]\n",
                "Bad Key",
            ),
        ];
        for (yaml, expected) in cases {
            let setup = setup_with_file(Some(yaml.as_bytes().to_vec()));

            let pipeline = push(&setup).await.unwrap();

            assert_eq!(pipeline.status, PipelineStatus::Failed, "{expected}");
            assert!(
                pipeline.error.as_deref().unwrap().contains(expected),
                "expected {expected:?} in {:?}",
                pipeline.error
            );
            assert!(setup.jobs.snapshot().is_empty());
        }
    }

    #[tokio::test]
    async fn a_pipeline_file_that_is_not_utf8_is_an_invalid_file_too() {
        let setup = setup_with_file(Some(vec![0xff, 0xfe, 0x00]));

        let pipeline = push(&setup).await.unwrap();

        assert_eq!(pipeline.status, PipelineStatus::Failed);
        assert!(pipeline.error.as_deref().unwrap().contains("UTF-8"));
    }

    #[tokio::test]
    async fn a_missing_pipeline_file_creates_no_pipeline_at_all() {
        let setup = setup_with_file(None);

        assert!(push(&setup).await.is_none());

        assert!(setup.pipelines.snapshot().is_empty());
        assert!(setup.events.pipeline_events().is_empty());
        assert!(setup.notifications.snapshot().is_empty());
    }

    #[tokio::test]
    async fn only_the_first_stage_is_submitted_at_creation_time() {
        let yaml = b"stages: [build, test, deploy]\njobs:\n  compile:\n    stage: build\n    image: rust:1.82\n    script: [\"cargo build\"]\n  lint:\n    stage: build\n    image: rust:1.82\n    script: [\"cargo clippy\"]\n  unit:\n    stage: test\n    image: rust:1.82\n    script: [\"cargo test\"]\n  ship:\n    stage: deploy\n    image: rust:1.82\n    script: [\"true\"]\n";
        let setup = setup(Config::with_file(yaml));

        push(&setup).await.unwrap();

        let submitted = setup.execution.submitted();
        let names: Vec<String> = setup
            .jobs
            .snapshot()
            .into_iter()
            .filter(|j| submitted.contains(&j.id))
            .map(|j| j.name)
            .collect();
        assert_eq!(
            names,
            vec!["compile", "lint"],
            "later stages wait for the barrier even though they declare no needs"
        );
    }

    #[tokio::test]
    async fn a_valid_pipeline_file_creates_the_pipeline_and_submits_every_job() {
        let setup = setup(Config::with_file(COMPILE_ONLY_YAML));

        let pipeline = push(&setup)
            .await
            .expect("a valid pipeline file must produce a pipeline");

        assert_eq!(pipeline.commit_sha, "abc123");
        assert_eq!(setup.jobs.snapshot().len(), 1);
        assert_eq!(
            setup.execution.submitted().len(),
            1,
            "the created job must be submitted to the execution engine"
        );
        assert!(
            setup
                .events
                .pipeline_events()
                .iter()
                .any(|(_, e)| e.event_type() == "PipelineStatusChanged")
        );
        assert!(
            setup
                .events
                .job_events()
                .iter()
                .any(|(_, e)| e.event_type() == "JobStatusChanged")
        );
    }

    #[tokio::test]
    async fn the_created_pipeline_is_tagged_with_the_currently_active_engine() {
        let setup = setup(Config {
            engine: ExecutionEngine::Kubernetes,
            ..Config::with_file(COMPILE_ONLY_YAML)
        });

        let pipeline = push(&setup).await.unwrap();

        assert_eq!(pipeline.execution_engine, ExecutionEngine::Kubernetes);
    }

    #[tokio::test]
    async fn jobs_are_created_in_stage_order_not_alphabetical_order() {
        // Creation order has to follow `stages`, not the BTreeMap's alphabetical one.
        let yaml = b"stages: [prepare, check]\njobs:\n  a-check:\n    stage: check\n    image: alpine:3.20\n    script: [\"true\"]\n  z-prepare:\n    stage: prepare\n    image: alpine:3.20\n    script: [\"true\"]\n";
        let setup = setup(Config::with_file(yaml));

        push(&setup).await.unwrap();

        let stages: Vec<String> = setup.jobs.snapshot().into_iter().map(|j| j.stage).collect();
        assert_eq!(stages, vec!["prepare", "check"]);
    }

    #[tokio::test]
    async fn a_job_with_unmet_needs_is_not_submitted_at_creation_time() {
        let yaml = b"stages: [build, test]\njobs:\n  compile:\n    stage: build\n    image: rust:1.82\n    script: [\"cargo build\"]\n  unit-tests:\n    stage: test\n    image: rust:1.82\n    script: [\"cargo test\"]\n    needs: [compile]\n";
        let setup = setup(Config::with_file(yaml));

        push(&setup).await.unwrap();

        assert_eq!(
            setup.execution.submitted().len(),
            1,
            "only the needs-less job (compile) should be submitted; unit-tests must wait"
        );
    }

    #[tokio::test]
    async fn a_submission_failure_marks_the_job_failed_instead_of_aborting_pipeline_creation() {
        let setup = setup(Config {
            engine: ExecutionEngine::Kubernetes,
            executor: Some(Arc::new(FailingExecution)),
            ..Config::with_file(COMPILE_ONLY_YAML)
        });

        let _pipeline = push(&setup)
            .await
            .expect("the pipeline row itself must still be created");

        assert_eq!(
            setup.jobs.snapshot()[0].status,
            JobStatus::Failed,
            "a job whose submission fails must end up Failed, not stuck Pending forever"
        );
        assert_eq!(
            setup.pipelines.snapshot()[0].status,
            PipelineStatus::Failed,
            "the pipeline must reach a terminal status too, via the same aggregation ReportJobResultUseCase already does"
        );
        assert_eq!(
            setup.notifications.snapshot().len(),
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
