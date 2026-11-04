use std::sync::Arc;

use ferrisgit_domain::branch::BranchInfo;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::{JobStatus, JobStorePort, NewJob};
use ferrisgit_domain::notification::NotificationStorePort;
use ferrisgit_domain::pipeline::{
    MergeRequestPipeline, NewPipeline, Pipeline, PipelineStatus, PipelineStorePort,
};
use ferrisgit_domain::pipeline_definition::{PipelineDefinition, parse_pipeline_definition};
use ferrisgit_domain::pipeline_events::{JobEvent, PipelineEvent, PipelineEventPublisherPort};
use ferrisgit_domain::pipeline_file_reader::PipelineFileReaderPort;
use ferrisgit_domain::pipeline_rules::{RuleEvent, reads_changed_paths, select_jobs};
use ferrisgit_domain::protected_branch::RefUpdate;
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::settings::{RepositorySettingsStorePort, SystemSettingsStorePort};
use ferrisgit_domain::tag::TagInfo;
use ferrisgit_domain::user::UserRepositoryPort;
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use uuid::Uuid;

use crate::job_execution_resolver::JobExecutionResolver;
use crate::use_cases::report_job_result::{ReportJobResultUseCase, notify_pipeline_failure};

/// What a pipeline is created for, which the `rules` of its jobs decide on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineTrigger {
    /// A branch moved, by a push or a merge. `previous_sha` is where it pointed, `None` for a new branch.
    Branch {
        name: String,
        previous_sha: Option<String>,
    },
    Tag {
        name: String,
    },
    /// The commit is the source branch merged into the target one, whose tip was `target_sha`.
    MergeRequest {
        run: MergeRequestPipeline,
        target_sha: String,
    },
}

impl PipelineTrigger {
    fn rule_event(&self) -> RuleEvent<'_> {
        match self {
            Self::Branch { name, .. } => RuleEvent::Branch(name),
            Self::Tag { name } => RuleEvent::Tag(name),
            Self::MergeRequest { .. } => RuleEvent::MergeRequest,
        }
    }

    /// The commit to compare with to know what this pipeline brings. A new branch and a tag have none.
    fn base_sha(&self) -> Option<&str> {
        match self {
            Self::Branch { previous_sha, .. } => previous_sha.as_deref(),
            Self::Tag { .. } => None,
            Self::MergeRequest { target_sha, .. } => Some(target_sha),
        }
    }
}

/// A pipeline a push asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushedPipeline {
    pub commit_sha: String,
    pub trigger: PipelineTrigger,
}

/// One pipeline per branch or tag a push created or moved, read from the refs as they are once it is over: a command is
/// only a request, and git may have refused it. A tag runs on the commit it peels to, not on its annotated tag object.
/// Other refs and deletions run nothing.
pub fn pipelines_of_push(
    updates: &[RefUpdate],
    branches: &[BranchInfo],
    tags: &[TagInfo],
) -> Vec<PushedPipeline> {
    updates
        .iter()
        .filter(|update| !update.is_deletion())
        .filter_map(|update| {
            if let Some(name) = update.branch() {
                let moved = branches
                    .iter()
                    .any(|branch| branch.name == name && branch.tip_sha == update.new_sha);
                return moved.then(|| PushedPipeline {
                    commit_sha: update.new_sha.clone(),
                    trigger: PipelineTrigger::Branch {
                        name: name.to_string(),
                        previous_sha: (!update.is_creation()).then(|| update.old_sha.clone()),
                    },
                });
            }
            let name = update.tag()?;
            let tag = tags.iter().find(|tag| tag.name == name)?;
            Some(PushedPipeline {
                commit_sha: tag.target_sha.clone(),
                trigger: PipelineTrigger::Tag {
                    name: name.to_string(),
                },
            })
        })
        .collect()
}

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

    /// `Ok(None)` when there's nothing to run (CI off, no pipeline file, or no job left by the `rules`). An invalid file
    /// still gives a pipeline, failed from the start with the parser's message and no jobs, so the pusher sees why. `Err`
    /// is for real failures. The pipeline file that counts is the one in `commit_sha`.
    pub async fn execute(
        &self,
        repository_id: Uuid,
        repository_disk_path: &str,
        commit_sha: &str,
        triggered_by: Uuid,
        trigger: PipelineTrigger,
    ) -> Result<Option<Pipeline>, DomainError> {
        let merge_request = match &trigger {
            PipelineTrigger::MergeRequest { run, .. } => Some(run.clone()),
            _ => None,
        };
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
            merge_request,
        };

        let definition = match String::from_utf8(yaml_bytes) {
            Ok(yaml) => parse_pipeline_definition(&yaml).map_err(|e| e.to_string()),
            Err(e) => Err(format!("pipeline file is not valid UTF-8: {e}")),
        };
        let definition = match definition {
            Ok(definition) => definition,
            Err(message) => return self.record_invalid_pipeline(new_pipeline, &message).await,
        };

        let changed_paths = self
            .changed_paths(&definition, repository_disk_path, commit_sha, &trigger)
            .await;
        let selected = select_jobs(&definition, trigger.rule_event(), changed_paths.as_deref());
        if selected.is_empty() {
            return match new_pipeline.merge_request {
                Some(_) => self.record_pipeline_with_nothing_to_run(new_pipeline).await,
                None => Ok(None),
            };
        }

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
        let mut ordered_jobs: Vec<_> = selected.into_iter().collect();
        ordered_jobs.sort_by_key(|(job_name, job_def)| {
            (
                definition.stages.iter().position(|s| s == &job_def.stage),
                *job_name,
            )
        });

        for (job_name, job_def) in ordered_jobs {
            let job = self
                .jobs
                .create(NewJob {
                    pipeline_id: pipeline.id,
                    stage: job_def.stage.clone(),
                    name: job_name.to_string(),
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

    /// What this pipeline changes, when a job asks. `None` when it can't be told, which runs the job rather than skips it.
    async fn changed_paths(
        &self,
        definition: &PipelineDefinition,
        repository_disk_path: &str,
        commit_sha: &str,
        trigger: &PipelineTrigger,
    ) -> Option<Vec<String>> {
        if !reads_changed_paths(definition) {
            return None;
        }
        let base_sha = trigger.base_sha()?;
        self.file_reader
            .changed_paths(repository_disk_path, base_sha, commit_sha)
            .await
            .inspect_err(|err| {
                tracing::warn!(error = %err, base_sha, commit_sha, "failed to compare two commits for `rules: changes`");
            })
            .ok()
            .flatten()
    }

    /// A merge request whose `rules` leave no job still gets a pipeline, succeeded from the start: nothing had to be
    /// checked on this commit, and a merge that requires a successful pipeline must not wait for one that never comes.
    async fn record_pipeline_with_nothing_to_run(
        &self,
        new_pipeline: NewPipeline,
    ) -> Result<Option<Pipeline>, DomainError> {
        let pipeline = self.pipelines.create(new_pipeline).await?;
        self.pipelines
            .update_status(pipeline.id, PipelineStatus::Success)
            .await?;
        self.events
            .publish_pipeline_event(
                pipeline.id,
                PipelineEvent::StatusChanged {
                    status: PipelineStatus::Success,
                },
            )
            .await
            .ok();
        self.pipelines.find_by_id(pipeline.id).await
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
                pipeline_must_succeed: false,
                default_merge_method: Default::default(),
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

    fn pushed_branch(name: &str) -> PipelineTrigger {
        PipelineTrigger::Branch {
            name: name.to_string(),
            previous_sha: Some("before".to_string()),
        }
    }

    async fn run(setup: &Setup, trigger: PipelineTrigger) -> Option<Pipeline> {
        setup
            .use_case
            .execute(
                setup.repository_id,
                "path",
                "abc123",
                Uuid::new_v4(),
                trigger,
            )
            .await
            .unwrap()
    }

    async fn push(setup: &Setup) -> Option<Pipeline> {
        run(setup, pushed_branch("main")).await
    }

    fn merge_request_trigger() -> PipelineTrigger {
        PipelineTrigger::MergeRequest {
            run: MergeRequestPipeline {
                merge_request_id: Uuid::new_v4(),
                source_sha: "tip".to_string(),
            },
            target_sha: "base".to_string(),
        }
    }

    fn job_names(setup: &Setup) -> Vec<String> {
        setup.jobs.snapshot().into_iter().map(|j| j.name).collect()
    }

    /// `build` has the given rules, `deploy` needs it, and `always` has neither.
    fn file_with_rules(rules: &str) -> Vec<u8> {
        format!(
            "stages: [build, deploy]\njobs:\n  always:\n    stage: build\n    image: alpine\n    script: [\"true\"]\n  build:\n    stage: build\n    image: alpine\n    script: [\"true\"]\n    rules: {rules}\n  deploy:\n    stage: deploy\n    image: alpine\n    script: [\"true\"]\n    needs: [build]\n"
        )
        .into_bytes()
    }

    #[tokio::test]
    async fn the_rules_decide_which_jobs_each_kind_of_pipeline_gets() {
        let file = file_with_rules("{branches: ['release/*'], tags: ['v*']}");
        let with_build = vec!["always", "build", "deploy"];
        let without_build = vec!["always"];
        for (trigger, expected) in [
            (pushed_branch("release/1"), &with_build),
            (pushed_branch("main"), &without_build),
            (
                PipelineTrigger::Tag {
                    name: "v1".to_string(),
                },
                &with_build,
            ),
            (
                PipelineTrigger::Tag {
                    name: "nightly".to_string(),
                },
                &without_build,
            ),
            (merge_request_trigger(), &without_build),
        ] {
            let setup = setup(Config::with_file(&file));

            run(&setup, trigger.clone()).await.expect("a pipeline");

            assert_eq!(&job_names(&setup), expected, "{trigger:?}");
        }
    }

    #[tokio::test]
    async fn a_job_left_out_is_never_submitted_and_the_stage_after_it_can_still_run() {
        let yaml = b"stages: [build, test]\njobs:\n  skipped:\n    stage: build\n    image: alpine\n    script: [\"true\"]\n    rules: {tags: ['v*']}\n  unit:\n    stage: test\n    image: alpine\n    script: [\"true\"]\n";
        let setup = setup(Config::with_file(yaml));

        push(&setup).await.unwrap();

        let jobs = setup.jobs.snapshot();
        assert_eq!(job_names(&setup), vec!["unit"]);
        assert_eq!(
            setup.execution.submitted(),
            vec![jobs[0].id],
            "no job of an earlier stage is left to wait for"
        );
    }

    #[tokio::test]
    async fn a_push_that_leaves_no_job_creates_no_pipeline() {
        let yaml = b"stages: [build]\njobs:\n  release:\n    stage: build\n    image: alpine\n    script: [\"true\"]\n    rules: {tags: ['v*']}\n";
        let setup = setup(Config::with_file(yaml));

        assert!(push(&setup).await.is_none());

        assert!(setup.pipelines.snapshot().is_empty());
        assert!(setup.events.pipeline_events().is_empty());
    }

    #[tokio::test]
    async fn a_merge_request_that_leaves_no_job_gets_a_pipeline_that_has_already_succeeded() {
        let yaml = b"stages: [build]\njobs:\n  release:\n    stage: build\n    image: alpine\n    script: [\"true\"]\n    rules: {tags: ['v*']}\n";
        let setup = setup(Config::with_file(yaml));

        let pipeline = run(&setup, merge_request_trigger()).await.unwrap();

        assert_eq!(pipeline.status, PipelineStatus::Success);
        assert!(pipeline.merge_request.is_some());
        assert_eq!(
            setup.pipelines.snapshot()[0].status,
            PipelineStatus::Success
        );
        assert!(setup.jobs.snapshot().is_empty());
        let events = setup.events.pipeline_events();
        let (_, PipelineEvent::StatusChanged { status }) = &events[0];
        assert_eq!(*status, PipelineStatus::Success);
        assert!(setup.notifications.snapshot().is_empty());
    }

    #[tokio::test]
    async fn invalid_rules_make_the_pipeline_fail_with_their_message() {
        let setup = setup(Config::with_file(&file_with_rules("{tags: []}")));

        let pipeline = push(&setup).await.unwrap();

        assert_eq!(pipeline.status, PipelineStatus::Failed);
        assert_eq!(
            pipeline.error.as_deref(),
            Some("job 'build' has invalid `rules`: `tags` is empty")
        );
        assert!(setup.jobs.snapshot().is_empty());
    }

    #[tokio::test]
    async fn an_unknown_key_in_the_rules_makes_the_pipeline_fail_too() {
        let setup = setup(Config::with_file(&file_with_rules("{branch: [main]}")));

        let pipeline = push(&setup).await.unwrap();

        assert_eq!(pipeline.status, PipelineStatus::Failed);
        let error = pipeline.error.unwrap();
        assert!(error.contains("unknown field `branch`"), "{error}");
    }

    /// A `Setup` around a file reader the test keeps, to see what it was asked to compare.
    fn setup_with_changed_paths(
        paths: &[&str],
    ) -> (Setup, CreatePipelineUseCase, Arc<FakeFileReader>) {
        let setup = setup(Config::with_file(b""));
        let reader = Arc::new(
            FakeFileReader::new(Some(file_with_rules("{changes: ['src/**']}")))
                .with_changed_paths(paths),
        );
        let use_case = CreatePipelineUseCase::new(
            Arc::new(FakeRepositorySettings::new(RepositorySettings {
                repository_id: setup.repository_id,
                pipeline_file_path: ".ferrisgit-ci.yml".to_string(),
                ci_enabled: true,
                required_approvals: 0,
                pipeline_must_succeed: false,
                default_merge_method: Default::default(),
            })),
            Arc::new(FakeSystemSettings::new(system_settings())),
            setup.pipelines.clone(),
            setup.jobs.clone(),
            reader.clone(),
            setup.events.clone(),
            Arc::new(JobExecutionResolver::new(
                setup.execution.clone(),
                setup.execution.clone(),
            )),
            Arc::new(FakeRepositories::new(vec![])),
            Arc::new(FakeUsers::new(vec![])),
            setup.notifications.clone(),
            Arc::new(FakeWebhooks::default()),
        );
        (setup, use_case, reader)
    }

    async fn run_with(
        use_case: &CreatePipelineUseCase,
        setup: &Setup,
        trigger: PipelineTrigger,
    ) -> Option<Pipeline> {
        use_case
            .execute(
                setup.repository_id,
                "path",
                "abc123",
                Uuid::new_v4(),
                trigger,
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn changes_compares_a_moved_branch_with_where_it_was() {
        let (setup, use_case, reader) = setup_with_changed_paths(&["src/main.rs"]);

        run_with(&use_case, &setup, pushed_branch("main")).await;

        assert_eq!(
            reader.compared(),
            vec![("before".to_string(), "abc123".to_string())]
        );
        assert_eq!(job_names(&setup), vec!["always", "build", "deploy"]);
    }

    #[tokio::test]
    async fn changes_compares_a_merge_request_with_the_tip_of_its_target() {
        let (setup, use_case, reader) = setup_with_changed_paths(&["README.md"]);

        run_with(&use_case, &setup, merge_request_trigger()).await;

        assert_eq!(
            reader.compared(),
            vec![("base".to_string(), "abc123".to_string())]
        );
        assert_eq!(job_names(&setup), vec!["always"]);
    }

    #[tokio::test]
    async fn changes_is_met_on_a_new_branch_and_on_a_tag_where_nothing_can_be_compared() {
        for trigger in [
            PipelineTrigger::Branch {
                name: "feature".to_string(),
                previous_sha: None,
            },
            PipelineTrigger::Tag {
                name: "v1".to_string(),
            },
        ] {
            let (setup, use_case, reader) = setup_with_changed_paths(&["README.md"]);

            run_with(&use_case, &setup, trigger).await;

            assert!(reader.compared().is_empty());
            assert_eq!(job_names(&setup), vec!["always", "build", "deploy"]);
        }
    }

    #[tokio::test]
    async fn nothing_is_compared_when_no_job_asks() {
        let setup = setup(Config::with_file(COMPILE_ONLY_YAML));
        let reader = Arc::new(FakeFileReader::new(Some(COMPILE_ONLY_YAML.to_vec())));
        let use_case = CreatePipelineUseCase::new(
            Arc::new(FakeRepositorySettings::new(RepositorySettings {
                repository_id: setup.repository_id,
                pipeline_file_path: ".ferrisgit-ci.yml".to_string(),
                ci_enabled: true,
                required_approvals: 0,
                pipeline_must_succeed: false,
                default_merge_method: Default::default(),
            })),
            Arc::new(FakeSystemSettings::new(system_settings())),
            setup.pipelines.clone(),
            setup.jobs.clone(),
            reader.clone(),
            setup.events.clone(),
            Arc::new(JobExecutionResolver::new(
                setup.execution.clone(),
                setup.execution.clone(),
            )),
            Arc::new(FakeRepositories::new(vec![])),
            Arc::new(FakeUsers::new(vec![])),
            setup.notifications.clone(),
            Arc::new(FakeWebhooks::default()),
        );

        run_with(&use_case, &setup, pushed_branch("main")).await;

        assert!(reader.compared().is_empty());
    }

    const ZERO: &str = "0000000000000000000000000000000000000000";
    const OLD: &str = "1111111111111111111111111111111111111111";
    const NEW: &str = "2222222222222222222222222222222222222222";
    const COMMIT: &str = "3333333333333333333333333333333333333333";

    fn update(old_sha: &str, new_sha: &str, ref_name: &str) -> RefUpdate {
        RefUpdate {
            old_sha: old_sha.to_string(),
            new_sha: new_sha.to_string(),
            ref_name: ref_name.to_string(),
        }
    }

    fn branch(name: &str, tip_sha: &str) -> BranchInfo {
        BranchInfo {
            name: name.to_string(),
            tip_sha: tip_sha.to_string(),
            is_default: false,
        }
    }

    fn tag(name: &str, target_sha: &str) -> TagInfo {
        TagInfo {
            name: name.to_string(),
            target_sha: target_sha.to_string(),
        }
    }

    #[test]
    fn a_push_asks_for_one_pipeline_per_branch_and_tag() {
        let pushed = pipelines_of_push(
            &[
                update(OLD, NEW, "refs/heads/main"),
                update(ZERO, NEW, "refs/heads/feature/x"),
                update(ZERO, NEW, "refs/tags/v1"),
            ],
            &[branch("main", NEW), branch("feature/x", NEW)],
            &[tag("v1", COMMIT)],
        );

        assert_eq!(
            pushed,
            vec![
                PushedPipeline {
                    commit_sha: NEW.to_string(),
                    trigger: PipelineTrigger::Branch {
                        name: "main".to_string(),
                        previous_sha: Some(OLD.to_string()),
                    },
                },
                PushedPipeline {
                    commit_sha: NEW.to_string(),
                    trigger: PipelineTrigger::Branch {
                        name: "feature/x".to_string(),
                        previous_sha: None,
                    },
                },
                PushedPipeline {
                    commit_sha: COMMIT.to_string(),
                    trigger: PipelineTrigger::Tag {
                        name: "v1".to_string(),
                    },
                },
            ],
            "the tag runs on the commit it peels to, not on the object the command names"
        );
    }

    #[test]
    fn deletions_other_refs_and_refused_updates_ask_for_nothing() {
        let pushed = pipelines_of_push(
            &[
                update(OLD, ZERO, "refs/heads/gone"),
                update(OLD, ZERO, "refs/tags/gone"),
                update(ZERO, NEW, "refs/notes/commits"),
                update(OLD, NEW, "refs/heads/refused"),
                update(ZERO, NEW, "refs/tags/refused"),
            ],
            &[branch("refused", OLD), branch("gone", OLD)],
            &[tag("gone", OLD)],
        );

        assert!(pushed.is_empty(), "{pushed:?}");
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
        let yaml = b"stages: [prepare, check]\njobs:\n  a-check:\n    stage: check\n    image: alpine:3.24\n    script: [\"true\"]\n  z-prepare:\n    stage: prepare\n    image: alpine:3.24\n    script: [\"true\"]\n";
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
