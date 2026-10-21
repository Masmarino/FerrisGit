use std::collections::BTreeMap;
use std::sync::Arc;

use ferrisgit_domain::branch::BranchReaderPort;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::{JobStatus, JobStorePort};
use ferrisgit_domain::merge_conditions::{
    CheckStatus, Mergeability, PIPELINE_CHECK, evaluate_mergeability, job_check_name,
};
use ferrisgit_domain::merge_request::{MergeRequest, MergeRequestReviewPort, ReviewDecision};
use ferrisgit_domain::pipeline::{PipelineStatus, PipelineStorePort};
use ferrisgit_domain::repository::Repository;
use ferrisgit_domain::settings::RepositorySettingsStorePort;

use crate::use_cases::protected_branches::ProtectedBranches;
use crate::use_cases::source_branch_tip::source_branch_tip;

/// Gathers what a merge request needs before it can be merged: the approvals the repository and the protection of its
/// target branch ask for, and the checks they name, with what the current source commit has. The merge refuses what
/// this refuses, and the interface lists it.
pub struct MergeabilityChecker {
    repository_settings: Arc<dyn RepositorySettingsStorePort>,
    protected_branches: Arc<ProtectedBranches>,
    reviews: Arc<dyn MergeRequestReviewPort>,
    branch_reader: Arc<dyn BranchReaderPort>,
    pipelines: Arc<dyn PipelineStorePort>,
    jobs: Arc<dyn JobStorePort>,
}

impl MergeabilityChecker {
    pub fn new(
        repository_settings: Arc<dyn RepositorySettingsStorePort>,
        protected_branches: Arc<ProtectedBranches>,
        reviews: Arc<dyn MergeRequestReviewPort>,
        branch_reader: Arc<dyn BranchReaderPort>,
        pipelines: Arc<dyn PipelineStorePort>,
        jobs: Arc<dyn JobStorePort>,
    ) -> Self {
        Self {
            repository_settings,
            protected_branches,
            reviews,
            branch_reader,
            pipelines,
            jobs,
        }
    }

    /// The approvals required are the most the repository or a protection of the target asks for. The checks are those
    /// the protections name, plus `pipeline` when the repository requires its pipelines to succeed. Only reviews and
    /// checks of the source branch's current commit count.
    pub async fn evaluate(
        &self,
        mr: &MergeRequest,
        repo: &Repository,
    ) -> Result<Mergeability, DomainError> {
        let settings = self
            .repository_settings
            .get_or_create_default(repo.id)
            .await?;
        let requirements = self
            .protected_branches
            .requirements(repo.id, &mr.target_branch)
            .await?;
        let required_approvals = settings
            .required_approvals
            .max(requirements.required_approvals);
        let mut required_checks = requirements.required_checks;
        if settings.pipeline_must_succeed && !required_checks.iter().any(|c| c == PIPELINE_CHECK) {
            required_checks.insert(0, PIPELINE_CHECK.to_string());
        }
        if required_approvals == 0 && required_checks.is_empty() {
            return Ok(Mergeability::default());
        }

        let tip_sha = source_branch_tip(
            self.branch_reader.as_ref(),
            &repo.disk_path,
            &mr.source_branch,
        )
        .await?;

        let (mut approved_by, mut changes_requested_by) = (0, Vec::new());
        if required_approvals > 0 {
            for review in self
                .reviews
                .list_reviews(mr.id)
                .await?
                .into_iter()
                .filter(|review| review.source_sha == tip_sha)
            {
                match review.decision {
                    ReviewDecision::Approved => approved_by += 1,
                    ReviewDecision::ChangesRequested => changes_requested_by.push(review.username),
                }
            }
        }

        let checks = if required_checks.is_empty() {
            BTreeMap::new()
        } else {
            self.checks_of(mr, &tip_sha).await?
        };
        Ok(evaluate_mergeability(
            required_approvals,
            approved_by,
            changes_requested_by,
            &required_checks,
            &checks,
        ))
    }

    /// The checks the source commit `tip_sha` has: its merge request pipeline as `pipeline`, and each of that pipeline's
    /// jobs as `pipeline/<job>`.
    async fn checks_of(
        &self,
        mr: &MergeRequest,
        tip_sha: &str,
    ) -> Result<BTreeMap<String, CheckStatus>, DomainError> {
        let mut checks = BTreeMap::new();
        let pipeline = self
            .pipelines
            .list_for_merge_request(mr.id)
            .await?
            .into_iter()
            .find(|pipeline| {
                pipeline
                    .merge_request
                    .as_ref()
                    .is_some_and(|run| run.source_sha == tip_sha)
            });
        let Some(pipeline) = pipeline else {
            return Ok(checks);
        };
        checks.insert(
            PIPELINE_CHECK.to_string(),
            pipeline_check_status(pipeline.status),
        );
        for job in self.jobs.list_for_pipeline(pipeline.id).await? {
            checks.insert(job_check_name(&job.name), job_check_status(job.status));
        }
        Ok(checks)
    }
}

fn pipeline_check_status(status: PipelineStatus) -> CheckStatus {
    match status {
        PipelineStatus::Pending => CheckStatus::Pending,
        PipelineStatus::Running => CheckStatus::Running,
        PipelineStatus::Success => CheckStatus::Success,
        PipelineStatus::Failed => CheckStatus::Failed,
        PipelineStatus::Canceled => CheckStatus::Canceled,
    }
}

fn job_check_status(status: JobStatus) -> CheckStatus {
    match status {
        JobStatus::Pending => CheckStatus::Pending,
        JobStatus::Running => CheckStatus::Running,
        JobStatus::Success => CheckStatus::Success,
        JobStatus::Failed => CheckStatus::Failed,
        JobStatus::Canceled => CheckStatus::Canceled,
        JobStatus::Skipped => CheckStatus::Skipped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeBranchReader, FakeCollaborators, FakeGroups, FakeJobs, FakeMergeRequests,
        FakePipelines, FakeProtectedBranches, FakeRepositorySettings,
    };
    use crate::use_cases::fixtures::{job, merge_request, pipeline, repository};
    use chrono::Utc;
    use ferrisgit_domain::branch::BranchInfo;
    use ferrisgit_domain::job::Job;
    use ferrisgit_domain::merge_conditions::MergeCondition;
    use ferrisgit_domain::merge_request::MergeRequestReview;
    use ferrisgit_domain::pipeline::{MergeRequestPipeline, Pipeline};
    use ferrisgit_domain::settings::RepositorySettings;
    use uuid::Uuid;

    struct World {
        checker: MergeabilityChecker,
        protected_branches: Arc<ProtectedBranches>,
        mr: MergeRequest,
        repo: Repository,
    }

    /// `runs` gives the merge request's pipelines and their jobs, once the merge request exists.
    fn world(
        required_approvals: i32,
        pipeline_must_succeed: bool,
        reviews: Vec<(&str, ReviewDecision, &str)>,
        runs: impl FnOnce(&MergeRequest) -> (Vec<Pipeline>, Vec<Job>),
    ) -> World {
        let repo = repository(Uuid::new_v4());
        let mr = merge_request(repo.id, Uuid::new_v4());
        let (pipelines, jobs) = runs(&mr);
        let reviews = reviews
            .into_iter()
            .map(|(username, decision, sha)| MergeRequestReview {
                merge_request_id: mr.id,
                user_id: Uuid::new_v4(),
                username: username.to_string(),
                decision,
                source_sha: sha.to_string(),
                created_at: Utc::now(),
            })
            .collect();
        let groups = Arc::new(FakeGroups::empty());
        let protected_branches = Arc::new(ProtectedBranches::new(
            Arc::new(FakeProtectedBranches::default()),
            Arc::new(FakeCollaborators::empty()),
            groups.clone(),
            groups,
        ));
        let checker = MergeabilityChecker::new(
            Arc::new(FakeRepositorySettings::new(RepositorySettings {
                repository_id: repo.id,
                pipeline_file_path: ".ferrisgit-ci.yml".to_string(),
                ci_enabled: true,
                required_approvals,
                pipeline_must_succeed,
                default_merge_method: Default::default(),
            })),
            protected_branches.clone(),
            Arc::new(FakeMergeRequests::new(vec![mr.clone()]).with_reviews(reviews)),
            Arc::new(FakeBranchReader::new(vec![BranchInfo {
                name: mr.source_branch.clone(),
                tip_sha: "tip".to_string(),
                is_default: false,
            }])),
            Arc::new(FakePipelines::new(pipelines)),
            Arc::new(FakeJobs::new(jobs)),
        );
        World {
            checker,
            protected_branches,
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
            merge_request: Some(MergeRequestPipeline {
                merge_request_id: mr.id,
                source_sha: source_sha.to_string(),
            }),
            ..pipeline(Uuid::new_v4(), status)
        }
    }

    #[tokio::test]
    async fn nothing_required_is_mergeable_without_looking_at_the_source_branch() {
        let w = world(0, false, vec![], |_| (vec![], vec![]));
        let mr = MergeRequest {
            source_branch: "gone".to_string(),
            ..w.mr.clone()
        };
        assert!(
            w.checker
                .evaluate(&mr, &w.repo)
                .await
                .unwrap()
                .is_mergeable()
        );
    }

    #[tokio::test]
    async fn the_protection_of_the_target_raises_the_approvals_and_only_current_reviews_count() {
        let w = world(
            1,
            false,
            vec![
                ("alice", ReviewDecision::Approved, "tip"),
                ("bob", ReviewDecision::Approved, "old"),
                ("carol", ReviewDecision::ChangesRequested, "old"),
            ],
            |_| (vec![], vec![]),
        );
        assert!(
            w.checker
                .evaluate(&w.mr, &w.repo)
                .await
                .unwrap()
                .is_mergeable()
        );

        w.protected_branches
            .protect(w.repo.id, &w.mr.target_branch, 2, &[])
            .await
            .unwrap();
        let mergeability = w.checker.evaluate(&w.mr, &w.repo).await.unwrap();
        assert_eq!(mergeability.refusal().as_deref(), Some("1/2 approvals"));
    }

    #[tokio::test]
    async fn the_pipeline_and_its_jobs_are_checks_of_the_current_source_commit() {
        let w = world(0, true, vec![], |mr| {
            let current = merge_request_pipeline(mr, "tip", PipelineStatus::Running);
            let older = merge_request_pipeline(mr, "old", PipelineStatus::Success);
            let lint = Job {
                name: "lint".to_string(),
                ..job(current.id, JobStatus::Success)
            };
            let test = Job {
                name: "test".to_string(),
                ..job(current.id, JobStatus::Running)
            };
            (vec![current, older], vec![lint, test])
        });
        w.protected_branches
            .protect(
                w.repo.id,
                &w.mr.target_branch,
                0,
                &["pipeline/lint".to_string(), "pipeline/test".to_string()],
            )
            .await
            .unwrap();

        let mergeability = w.checker.evaluate(&w.mr, &w.repo).await.unwrap();

        assert_eq!(
            mergeability.conditions,
            vec![
                MergeCondition::Check {
                    name: "pipeline".to_string(),
                    status: Some(CheckStatus::Running),
                },
                MergeCondition::Check {
                    name: "pipeline/lint".to_string(),
                    status: Some(CheckStatus::Success),
                },
                MergeCondition::Check {
                    name: "pipeline/test".to_string(),
                    status: Some(CheckStatus::Running),
                },
            ]
        );
        assert!(!mergeability.is_mergeable());
    }
}
