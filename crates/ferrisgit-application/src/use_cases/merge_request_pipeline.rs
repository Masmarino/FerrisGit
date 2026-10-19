use std::sync::Arc;

use ferrisgit_domain::branch::BranchReaderPort;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::merge_executor::MergeExecutorPort;
use ferrisgit_domain::merge_request::{MergeRequest, MergeRequestStatus};
use ferrisgit_domain::pipeline::{
    MergeRequestPipeline, Pipeline, PipelineStatus, PipelineStorePort,
};
use ferrisgit_domain::repository::Repository;
use uuid::Uuid;

use crate::use_cases::cancel_pipeline::CancelPipelineUseCase;
use crate::use_cases::create_pipeline::CreatePipelineUseCase;
use crate::use_cases::source_branch_tip::source_branch_tip;

/// The ref that holds the merge commit the pipelines of a merge request run on.
pub fn merge_request_ref(merge_request_id: Uuid) -> String {
    format!("refs/merge-requests/{merge_request_id}/merge")
}

/// Starts the pipeline of a merge request: the source branch merged into the target one, run with the pipeline file of
/// that merge. A merge request has one pipeline per source commit: a push to the source branch starts a new one and
/// cancels the ones still running for older commits, which no longer say anything about what would be merged.
pub struct RunMergeRequestPipelineUseCase {
    merge_executor: Arc<dyn MergeExecutorPort>,
    branch_reader: Arc<dyn BranchReaderPort>,
    pipelines: Arc<dyn PipelineStorePort>,
    create_pipeline: CreatePipelineUseCase,
    cancel_pipeline: CancelPipelineUseCase,
}

impl RunMergeRequestPipelineUseCase {
    pub fn new(
        merge_executor: Arc<dyn MergeExecutorPort>,
        branch_reader: Arc<dyn BranchReaderPort>,
        pipelines: Arc<dyn PipelineStorePort>,
        create_pipeline: CreatePipelineUseCase,
        cancel_pipeline: CancelPipelineUseCase,
    ) -> Self {
        Self {
            merge_executor,
            branch_reader,
            pipelines,
            create_pipeline,
            cancel_pipeline,
        }
    }

    /// `Ok(None)` when nothing was started: the merge request is not open, its source commit already has a pipeline,
    /// the branches conflict, or the repository runs no pipeline (CI off, no pipeline file).
    pub async fn execute(
        &self,
        mr: &MergeRequest,
        repo: &Repository,
        triggered_by: Uuid,
    ) -> Result<Option<Pipeline>, DomainError> {
        if mr.status != MergeRequestStatus::Open {
            return Ok(None);
        }
        let source_sha = source_branch_tip(
            self.branch_reader.as_ref(),
            &repo.disk_path,
            &mr.source_branch,
        )
        .await?;
        let existing = self.pipelines.list_for_merge_request(mr.id).await?;
        if existing.iter().any(|pipeline| {
            pipeline
                .merge_request
                .as_ref()
                .is_some_and(|run| run.source_sha == source_sha)
        }) {
            return Ok(None);
        }

        // Before the ref moves: a pipeline still running fetches the commit it was given, and stops being worth it.
        for superseded in existing.iter().filter(|p| p.status.is_active()) {
            self.cancel_pipeline.execute(superseded.id).await?;
        }

        let message = format!(
            "Merge branch '{}' into {} (merge request pipeline)",
            mr.source_branch, mr.target_branch
        );
        let Some(merge) = self
            .merge_executor
            .write_merge_ref(
                &repo.disk_path,
                &mr.source_branch,
                &mr.target_branch,
                &merge_request_ref(mr.id),
                &message,
            )
            .await?
        else {
            return Ok(None);
        };
        self.create_pipeline
            .execute_for_merge_request(
                repo.id,
                &repo.disk_path,
                &merge.commit_sha,
                triggered_by,
                MergeRequestPipeline {
                    merge_request_id: mr.id,
                    source_sha: merge.source_sha,
                },
            )
            .await
    }
}

/// Why the merge request can't be merged yet under "pipelines must succeed", or `None` when its latest pipeline, the
/// one of the current source commit, succeeded. `pipelines` is the merge request's, newest first.
pub fn pipeline_requirement_problem(pipelines: &[Pipeline], source_sha: &str) -> Option<String> {
    let latest = pipelines.first().filter(|pipeline| {
        pipeline
            .merge_request
            .as_ref()
            .is_some_and(|run| run.source_sha == source_sha)
    });
    match latest.map(|pipeline| pipeline.status) {
        Some(PipelineStatus::Success) => None,
        Some(PipelineStatus::Pending | PipelineStatus::Running) => {
            Some("the merge request pipeline has not finished yet: it must succeed before the merge".to_string())
        }
        Some(status) => Some(format!(
            "the merge request pipeline is {}: it must succeed before the merge",
            status.as_str()
        )),
        None => Some(
            "no merge request pipeline has run on the latest commit of the source branch: one must succeed before the merge"
                .to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job_execution_resolver::JobExecutionResolver;
    use crate::test_support::{
        FakeBranchReader, FakeEvents, FakeExecution, FakeFileReader, FakeJobs, FakeNotifications,
        FakePipelines, FakeRepositories, FakeRepositorySettings, FakeSystemSettings, FakeUsers,
        FakeWebhooks,
    };
    use crate::use_cases::fixtures::{merge_request, pipeline, repository, system_settings};
    use async_trait::async_trait;
    use ferrisgit_domain::branch::BranchInfo;
    use ferrisgit_domain::merge_executor::{MergeOutcome, MergeRef};
    use ferrisgit_domain::settings::RepositorySettings;
    use std::sync::Mutex;

    const PIPELINE_FILE: &str = "stages: [test]\njobs:\n  unit:\n    stage: test\n    image: alpine\n    script: [\"true\"]\n";

    /// Writes merge refs whose commit is `merge-<n>`, or reports a conflict.
    struct FakeMergeRefs {
        conflicting: bool,
        written: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl MergeExecutorPort for FakeMergeRefs {
        async fn merge(
            &self,
            _repository_disk_path: &str,
            _source_branch: &str,
            _target_branch: &str,
            _message: &str,
        ) -> Result<MergeOutcome, DomainError> {
            unimplemented!()
        }

        async fn write_merge_ref(
            &self,
            _repository_disk_path: &str,
            _source_branch: &str,
            _target_branch: &str,
            ref_name: &str,
            _message: &str,
        ) -> Result<Option<MergeRef>, DomainError> {
            if self.conflicting {
                return Ok(None);
            }
            let mut written = self.written.lock().unwrap();
            written.push(ref_name.to_string());
            Ok(Some(MergeRef {
                commit_sha: format!("merge-{}", written.len()),
                source_sha: "tip".to_string(),
                target_sha: "base".to_string(),
            }))
        }
    }

    struct World {
        use_case: RunMergeRequestPipelineUseCase,
        pipelines: Arc<FakePipelines>,
        merge_refs: Arc<FakeMergeRefs>,
        mr: MergeRequest,
        repo: Repository,
    }

    fn world(conflicting: bool, pipeline_file: Option<&str>) -> World {
        let repo = repository(Uuid::new_v4());
        let mr = merge_request(repo.id, Uuid::new_v4());
        let pipelines = Arc::new(FakePipelines::empty());
        let jobs = Arc::new(FakeJobs::new(vec![]));
        let events = Arc::new(FakeEvents::new());
        let execution = Arc::new(FakeExecution::new());
        let job_execution = Arc::new(JobExecutionResolver::new(
            execution.clone(),
            execution.clone(),
        ));
        let merge_refs = Arc::new(FakeMergeRefs {
            conflicting,
            written: Mutex::new(vec![]),
        });
        let create_pipeline = CreatePipelineUseCase::new(
            Arc::new(FakeRepositorySettings::new(RepositorySettings {
                repository_id: repo.id,
                pipeline_file_path: ".ferrisgit-ci.yml".to_string(),
                ci_enabled: true,
                required_approvals: 0,
                pipeline_must_succeed: false,
            })),
            Arc::new(FakeSystemSettings::new(system_settings())),
            pipelines.clone(),
            jobs.clone(),
            Arc::new(FakeFileReader::new(
                pipeline_file.map(|f| f.as_bytes().to_vec()),
            )),
            events.clone(),
            job_execution.clone(),
            Arc::new(FakeRepositories::new(vec![repo.clone()])),
            Arc::new(FakeUsers::new(vec![])),
            Arc::new(FakeNotifications::new(vec![])),
            Arc::new(FakeWebhooks::new()),
        );
        let cancel_pipeline =
            CancelPipelineUseCase::new(pipelines.clone(), jobs, job_execution, events);
        let use_case = RunMergeRequestPipelineUseCase::new(
            merge_refs.clone(),
            Arc::new(FakeBranchReader::new(vec![BranchInfo {
                name: mr.source_branch.clone(),
                tip_sha: "tip".to_string(),
                is_default: false,
            }])),
            pipelines.clone(),
            create_pipeline,
            cancel_pipeline,
        );
        World {
            use_case,
            pipelines,
            merge_refs,
            mr,
            repo,
        }
    }

    fn merge_request_pipeline(
        mr: &MergeRequest,
        source_sha: &str,
        status: PipelineStatus,
    ) -> Pipeline {
        Pipeline {
            repository_id: mr.repository_id,
            merge_request: Some(MergeRequestPipeline {
                merge_request_id: mr.id,
                source_sha: source_sha.to_string(),
            }),
            ..pipeline(Uuid::new_v4(), status)
        }
    }

    #[tokio::test]
    async fn a_pipeline_runs_on_the_merge_of_the_source_into_the_target() {
        let w = world(false, Some(PIPELINE_FILE));

        let started = w
            .use_case
            .execute(&w.mr, &w.repo, Uuid::new_v4())
            .await
            .unwrap()
            .expect("a pipeline");

        assert_eq!(started.commit_sha, "merge-1");
        assert_eq!(
            started.merge_request,
            Some(MergeRequestPipeline {
                merge_request_id: w.mr.id,
                source_sha: "tip".to_string(),
            })
        );
        assert_eq!(
            *w.merge_refs.written.lock().unwrap(),
            vec![merge_request_ref(w.mr.id)]
        );
    }

    #[tokio::test]
    async fn a_source_commit_that_already_has_a_pipeline_gets_no_second_one() {
        let w = world(false, Some(PIPELINE_FILE));
        w.pipelines.insert(merge_request_pipeline(
            &w.mr,
            "tip",
            PipelineStatus::Success,
        ));

        assert!(
            w.use_case
                .execute(&w.mr, &w.repo, Uuid::new_v4())
                .await
                .unwrap()
                .is_none()
        );
        assert!(w.merge_refs.written.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_new_source_commit_cancels_the_pipeline_still_running_for_the_old_one() {
        let w = world(false, Some(PIPELINE_FILE));
        let old_running = merge_request_pipeline(&w.mr, "old", PipelineStatus::Running);
        let old_failed = merge_request_pipeline(&w.mr, "older", PipelineStatus::Failed);
        for pipeline in [old_running.clone(), old_failed.clone()] {
            w.pipelines.insert(pipeline);
        }

        let started = w
            .use_case
            .execute(&w.mr, &w.repo, Uuid::new_v4())
            .await
            .unwrap();

        assert!(started.is_some());
        assert_eq!(
            w.pipelines.get(old_running.id).unwrap().status,
            PipelineStatus::Canceled
        );
        assert_eq!(
            w.pipelines.get(old_failed.id).unwrap().status,
            PipelineStatus::Failed,
            "a finished pipeline keeps its outcome"
        );
    }

    #[tokio::test]
    async fn nothing_runs_for_conflicting_branches_a_closed_merge_request_or_a_repository_without_pipeline_file()
     {
        let w = world(true, Some(PIPELINE_FILE));
        assert!(
            w.use_case
                .execute(&w.mr, &w.repo, Uuid::new_v4())
                .await
                .unwrap()
                .is_none()
        );

        let w = world(false, Some(PIPELINE_FILE));
        let closed = MergeRequest {
            status: MergeRequestStatus::Closed,
            ..w.mr.clone()
        };
        assert!(
            w.use_case
                .execute(&closed, &w.repo, Uuid::new_v4())
                .await
                .unwrap()
                .is_none()
        );

        let w = world(false, None);
        assert!(
            w.use_case
                .execute(&w.mr, &w.repo, Uuid::new_v4())
                .await
                .unwrap()
                .is_none()
        );
        assert!(w.pipelines.snapshot().is_empty());
    }

    #[test]
    fn only_a_successful_pipeline_of_the_current_source_commit_lets_the_merge_through() {
        let mr = merge_request(Uuid::new_v4(), Uuid::new_v4());
        let ok = merge_request_pipeline(&mr, "tip", PipelineStatus::Success);
        assert_eq!(
            pipeline_requirement_problem(std::slice::from_ref(&ok), "tip"),
            None
        );

        let problem =
            |pipelines: &[Pipeline]| pipeline_requirement_problem(pipelines, "tip").unwrap();
        assert!(problem(&[]).contains("no merge request pipeline"));
        let stale = merge_request_pipeline(&mr, "old", PipelineStatus::Success);
        assert!(
            pipeline_requirement_problem(&[stale], "tip")
                .unwrap()
                .contains("no merge request pipeline")
        );
        let running = merge_request_pipeline(&mr, "tip", PipelineStatus::Running);
        assert!(problem(&[running]).contains("not finished"));
        let failed = merge_request_pipeline(&mr, "tip", PipelineStatus::Failed);
        assert!(problem(&[failed]).contains("is failed"));
    }
}
