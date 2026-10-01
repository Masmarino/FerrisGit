use std::sync::Arc;

use ferrisgit_domain::branch::BranchReaderPort;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::JobStorePort;
use ferrisgit_domain::merge_executor::{MergeExecutorPort, MergeOutcome};
use ferrisgit_domain::merge_request::{
    MergeRequest, MergeRequestReviewPort, MergeRequestStatus, MergeRequestStorePort, ReviewDecision,
};
use ferrisgit_domain::notification::{NewNotification, NotificationKind, NotificationStorePort};
use ferrisgit_domain::pipeline::PipelineStorePort;
use ferrisgit_domain::pipeline_events::PipelineEventPublisherPort;
use ferrisgit_domain::pipeline_file_reader::PipelineFileReaderPort;
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::settings::{RepositorySettingsStorePort, SystemSettingsStorePort};
use ferrisgit_domain::user::UserRepositoryPort;
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use ferrisgit_domain::webhook_event::WebhookEvent;
use uuid::Uuid;

use crate::job_execution_resolver::JobExecutionResolver;
use crate::use_cases::create_pipeline::CreatePipelineUseCase;

#[derive(Debug)]
pub enum MergeMergeRequestResult {
    Merged(Box<MergeRequest>),
    Conflicting,
}

impl PartialEq for MergeMergeRequestResult {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (MergeMergeRequestResult::Conflicting, MergeMergeRequestResult::Conflicting) => true,
            (MergeMergeRequestResult::Merged(a), MergeMergeRequestResult::Merged(b)) => {
                a.id == b.id && a.status == b.status && a.merge_commit_sha == b.merge_commit_sha
            }
            _ => false,
        }
    }
}

pub struct MergeMergeRequestUseCase {
    merge_requests: Arc<dyn MergeRequestStorePort>,
    merge_request_reviews: Arc<dyn MergeRequestReviewPort>,
    merge_executor: Arc<dyn MergeExecutorPort>,
    repositories: Arc<dyn RepositoryStorePort>,
    repository_settings: Arc<dyn RepositorySettingsStorePort>,
    system_settings: Arc<dyn SystemSettingsStorePort>,
    pipelines: Arc<dyn PipelineStorePort>,
    jobs: Arc<dyn JobStorePort>,
    pipeline_file_reader: Arc<dyn PipelineFileReaderPort>,
    pipeline_events: Arc<dyn PipelineEventPublisherPort>,
    job_execution: Arc<JobExecutionResolver>,
    branch_reader: Arc<dyn BranchReaderPort>,
    users: Arc<dyn UserRepositoryPort>,
    notifications: Arc<dyn NotificationStorePort>,
    webhooks: Arc<dyn WebhookDispatcherPort>,
}

impl MergeMergeRequestUseCase {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        merge_requests: Arc<dyn MergeRequestStorePort>,
        merge_request_reviews: Arc<dyn MergeRequestReviewPort>,
        merge_executor: Arc<dyn MergeExecutorPort>,
        repositories: Arc<dyn RepositoryStorePort>,
        repository_settings: Arc<dyn RepositorySettingsStorePort>,
        system_settings: Arc<dyn SystemSettingsStorePort>,
        pipelines: Arc<dyn PipelineStorePort>,
        jobs: Arc<dyn JobStorePort>,
        pipeline_file_reader: Arc<dyn PipelineFileReaderPort>,
        pipeline_events: Arc<dyn PipelineEventPublisherPort>,
        job_execution: Arc<JobExecutionResolver>,
        branch_reader: Arc<dyn BranchReaderPort>,
        users: Arc<dyn UserRepositoryPort>,
        notifications: Arc<dyn NotificationStorePort>,
        webhooks: Arc<dyn WebhookDispatcherPort>,
    ) -> Self {
        Self {
            merge_requests,
            merge_request_reviews,
            merge_executor,
            repositories,
            repository_settings,
            system_settings,
            pipelines,
            jobs,
            pipeline_file_reader,
            pipeline_events,
            job_execution,
            branch_reader,
            users,
            notifications,
            webhooks,
        }
    }

    /// Merges a merge request (clean, conflict-free merges only), then triggers a pipeline on the target's new tip. The
    /// commit was written with git plumbing and never went through the push path that normally starts one.
    pub async fn execute(
        &self,
        merge_request_id: Uuid,
        user_id: Uuid,
    ) -> Result<MergeMergeRequestResult, DomainError> {
        let mr = self
            .merge_requests
            .find_by_id(merge_request_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("merge request".to_string()))?;
        if mr.status != MergeRequestStatus::Open {
            return Err(DomainError::Validation(
                "merge request is not open".to_string(),
            ));
        }
        let repo = self
            .repositories
            .find_by_id(mr.repository_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("repository".to_string()))?;

        let settings = self
            .repository_settings
            .get_or_create_default(repo.id)
            .await?;
        if settings.required_approvals > 0 {
            let branches = self.branch_reader.list_branches(&repo.disk_path).await?;
            let tip_sha = branches
                .into_iter()
                .find(|b| b.name == mr.source_branch)
                .map(|b| b.tip_sha)
                .ok_or_else(|| {
                    DomainError::Validation("source branch no longer exists".to_string())
                })?;
            let reviews = self.merge_request_reviews.list_reviews(mr.id).await?;
            let live: Vec<_> = reviews.iter().filter(|r| r.source_sha == tip_sha).collect();

            if let Some(blocker) = live
                .iter()
                .find(|r| r.decision == ReviewDecision::ChangesRequested)
            {
                return Err(DomainError::Validation(format!(
                    "changes requested by {}",
                    blocker.username
                )));
            }
            let approvals = live
                .iter()
                .filter(|r| r.decision == ReviewDecision::Approved)
                .count();
            if approvals < settings.required_approvals as usize {
                return Err(DomainError::Validation(format!(
                    "{approvals}/{} approvals",
                    settings.required_approvals
                )));
            }
        }

        let message = format!(
            "Merge branch '{}' into {}",
            mr.source_branch, mr.target_branch
        );
        let outcome = self
            .merge_executor
            .merge(
                &repo.disk_path,
                &mr.source_branch,
                &mr.target_branch,
                &message,
            )
            .await?;

        match outcome {
            MergeOutcome::Conflicting => Ok(MergeMergeRequestResult::Conflicting),
            MergeOutcome::Merged { commit_sha } => {
                self.merge_requests.mark_merged(mr.id, &commit_sha).await?;

                let create_pipeline = CreatePipelineUseCase::new(
                    self.repository_settings.clone(),
                    self.system_settings.clone(),
                    self.pipelines.clone(),
                    self.jobs.clone(),
                    self.pipeline_file_reader.clone(),
                    self.pipeline_events.clone(),
                    self.job_execution.clone(),
                    self.repositories.clone(),
                    self.users.clone(),
                    self.notifications.clone(),
                    self.webhooks.clone(),
                );
                if let Err(err) = create_pipeline
                    .execute(repo.id, &repo.disk_path, &commit_sha, user_id)
                    .await
                {
                    tracing::error!(error = %err, merge_request_id = %mr.id, "failed to create pipeline after merge");
                }

                let merged = self
                    .merge_requests
                    .find_by_id(mr.id)
                    .await?
                    .ok_or_else(|| DomainError::NotFound("merge request".to_string()))?;

                if let Some(owner) = self.users.find_by_id(repo.owner_id).await.ok().flatten()
                    && let Some(actor) = self.users.find_by_id(user_id).await.ok().flatten()
                {
                    self.webhooks
                        .dispatch(
                            mr.repository_id,
                            WebhookEvent::MergeRequestMerged {
                                repository_owner: owner.username.clone(),
                                repository_name: repo.name.clone(),
                                actor_username: actor.username.clone(),
                                merge_request_id: mr.id,
                                merge_request_title: mr.title.clone(),
                            },
                        )
                        .await
                        .ok();

                    // Nobody to notify once the author's account is gone.
                    if let Some(mr_author_id) = mr.author_id
                        && mr_author_id != user_id
                    {
                        self.notifications
                            .create(NewNotification {
                                recipient_id: mr_author_id,
                                kind: NotificationKind::MergeRequestMerged,
                                repository_owner: owner.username,
                                repository_name: repo.name,
                                actor_username: Some(actor.username),
                                merge_request_id: Some(mr.id),
                                merge_request_title: Some(mr.title),
                                pipeline_id: None,
                                commit_sha: None,
                                role: None,
                                issue_id: None,
                                issue_number: None,
                                issue_title: None,
                            })
                            .await
                            .ok();
                    }
                }

                Ok(MergeMergeRequestResult::Merged(Box::new(merged)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeBranchReader, FakeEvents, FakeFileReader, FakeJobs, FakeMergeRequests,
        FakeNotifications, FakePipelines, FakeRepositories, FakeRepositorySettings,
        FakeSystemSettings, FakeUsers, FakeWebhooks,
    };
    use async_trait::async_trait;
    use chrono::Utc;
    use ferrisgit_domain::branch::BranchInfo;
    use ferrisgit_domain::job::Job;
    use ferrisgit_domain::job_execution::JobExecutionPort;
    use ferrisgit_domain::merge_request::{MergeRequestReview, ReviewDecision};
    use ferrisgit_domain::repository::{Repository, RepositoryVisibility};
    use ferrisgit_domain::settings::RepositorySettings;
    use ferrisgit_domain::user::User;
    use ferrisgit_domain::webhook_event::WebhookEvent;
    use std::sync::Mutex;

    fn merge_request(status: MergeRequestStatus) -> MergeRequest {
        MergeRequest {
            id: Uuid::new_v4(),
            repository_id: Uuid::new_v4(),
            author_id: Some(Uuid::new_v4()),
            source_branch: "feature".to_string(),
            target_branch: "main".to_string(),
            title: "Add feature".to_string(),
            description: String::new(),
            status,
            merge_commit_sha: None,
            milestone_id: None,
            created_at: Utc::now(),
            closed_at: None,
        }
    }

    struct FakeMergeExecutor {
        outcome: MergeOutcome,
        calls: Mutex<Vec<(String, String, String)>>,
    }
    #[async_trait]
    impl MergeExecutorPort for FakeMergeExecutor {
        async fn merge(
            &self,
            repository_disk_path: &str,
            source_branch: &str,
            target_branch: &str,
            _message: &str,
        ) -> Result<MergeOutcome, DomainError> {
            self.calls.lock().unwrap().push((
                repository_disk_path.to_string(),
                source_branch.to_string(),
                target_branch.to_string(),
            ));
            Ok(self.outcome.clone())
        }
    }

    /// CI is disabled in `FakeRepositorySettings`, so this is never called; it only satisfies
    /// `JobExecutionResolver::new`.
    struct NoopExecutor;
    #[async_trait]
    impl JobExecutionPort for NoopExecutor {
        async fn submit(&self, _job: &Job) -> Result<(), DomainError> {
            Ok(())
        }
        async fn cancel(&self, _job: &Job) -> Result<(), DomainError> {
            Ok(())
        }
    }

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

    #[allow(clippy::too_many_arguments)]
    fn use_case(
        mr_store: Arc<FakeMergeRequests>,
        executor: Arc<FakeMergeExecutor>,
        repo: Repository,
        pipelines: Arc<FakePipelines>,
        jobs: Arc<FakeJobs>,
        required_approvals: i32,
        branch_reader: Arc<FakeBranchReader>,
        users: Arc<FakeUsers>,
        notifications: Arc<FakeNotifications>,
        webhooks: Arc<FakeWebhooks>,
    ) -> MergeMergeRequestUseCase {
        let docker: Arc<dyn JobExecutionPort> = Arc::new(NoopExecutor);
        MergeMergeRequestUseCase::new(
            mr_store.clone(),
            mr_store,
            executor,
            Arc::new(FakeRepositories::new(vec![repo])),
            Arc::new(FakeRepositorySettings::new(RepositorySettings {
                repository_id: Uuid::new_v4(),
                pipeline_file_path: ".ferrisgit-ci.yml".to_string(),
                ci_enabled: false,
                required_approvals,
            })),
            Arc::new(FakeSystemSettings::default()),
            pipelines,
            jobs,
            Arc::new(FakeFileReader::none()),
            Arc::new(FakeEvents::new()),
            Arc::new(JobExecutionResolver::new(docker.clone(), docker)),
            branch_reader,
            users,
            notifications,
            webhooks,
        )
    }

    #[tokio::test]
    async fn a_clean_merge_marks_the_request_merged_and_triggers_a_pipeline() {
        let mr = merge_request(MergeRequestStatus::Open);
        let mr_store = Arc::new(FakeMergeRequests::new(vec![mr.clone()]));
        let executor = Arc::new(FakeMergeExecutor {
            outcome: MergeOutcome::Merged {
                commit_sha: "deadbeef".to_string(),
            },
            calls: Mutex::new(vec![]),
        });
        let repo = repository(mr.repository_id);
        let pipelines = Arc::new(FakePipelines::empty());
        let jobs = Arc::new(FakeJobs::empty());
        let use_case = use_case(
            mr_store.clone(),
            executor.clone(),
            repo.clone(),
            pipelines.clone(),
            jobs.clone(),
            0,
            Arc::new(FakeBranchReader::new(vec![])),
            Arc::new(FakeUsers::new(vec![user(Uuid::new_v4())])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(mr.id, mr.author_id.unwrap())
            .await
            .unwrap();

        let MergeMergeRequestResult::Merged(merged) = result else {
            panic!("expected a clean merge")
        };
        assert_eq!(merged.status, MergeRequestStatus::Merged);
        assert_eq!(merged.merge_commit_sha, Some("deadbeef".to_string()));
        assert_eq!(
            executor.calls.lock().unwrap()[0],
            (
                repo.disk_path.clone(),
                "feature".to_string(),
                "main".to_string()
            )
        );
        // CI is disabled in the fake, so no pipeline is created. This only checks that the use case was called with the
        // new commit.
        assert!(pipelines.snapshot().is_empty());
    }

    #[tokio::test]
    async fn a_conflicting_merge_leaves_the_request_open_and_never_touches_the_store_or_a_pipeline()
    {
        let mr = merge_request(MergeRequestStatus::Open);
        let mr_store = Arc::new(FakeMergeRequests::new(vec![mr.clone()]));
        let executor = Arc::new(FakeMergeExecutor {
            outcome: MergeOutcome::Conflicting,
            calls: Mutex::new(vec![]),
        });
        let repo = repository(mr.repository_id);
        let pipelines = Arc::new(FakePipelines::empty());
        let jobs = Arc::new(FakeJobs::empty());
        let use_case = use_case(
            mr_store.clone(),
            executor,
            repo,
            pipelines.clone(),
            jobs,
            0,
            Arc::new(FakeBranchReader::new(vec![])),
            Arc::new(FakeUsers::new(vec![user(Uuid::new_v4())])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(mr.id, mr.author_id.unwrap())
            .await
            .unwrap();

        assert_eq!(result, MergeMergeRequestResult::Conflicting);
        assert_eq!(
            mr_store.get(mr.id).unwrap().status,
            MergeRequestStatus::Open,
            "mark_merged must never be called on a conflicting outcome"
        );
        assert!(pipelines.snapshot().is_empty());
    }

    #[tokio::test]
    async fn merging_an_already_merged_request_is_a_validation_error() {
        let mr = merge_request(MergeRequestStatus::Merged);
        let mr_store = Arc::new(FakeMergeRequests::new(vec![mr.clone()]));
        let executor = Arc::new(FakeMergeExecutor {
            outcome: MergeOutcome::Merged {
                commit_sha: "x".to_string(),
            },
            calls: Mutex::new(vec![]),
        });
        let repo = repository(mr.repository_id);
        let use_case = use_case(
            mr_store,
            executor.clone(),
            repo,
            Arc::new(FakePipelines::empty()),
            Arc::new(FakeJobs::empty()),
            0,
            Arc::new(FakeBranchReader::new(vec![])),
            Arc::new(FakeUsers::new(vec![user(Uuid::new_v4())])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case.execute(mr.id, mr.author_id.unwrap()).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
        assert!(
            executor.calls.lock().unwrap().is_empty(),
            "the executor must never be invoked for an already-terminal merge request"
        );
    }

    #[tokio::test]
    async fn merging_a_non_existent_request_is_a_not_found_error() {
        let use_case = use_case(
            Arc::new(FakeMergeRequests::empty()),
            Arc::new(FakeMergeExecutor {
                outcome: MergeOutcome::Conflicting,
                calls: Mutex::new(vec![]),
            }),
            repository(Uuid::new_v4()),
            Arc::new(FakePipelines::empty()),
            Arc::new(FakeJobs::empty()),
            0,
            Arc::new(FakeBranchReader::new(vec![])),
            Arc::new(FakeUsers::new(vec![user(Uuid::new_v4())])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case.execute(Uuid::new_v4(), Uuid::new_v4()).await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn merging_is_blocked_when_live_approvals_are_below_the_threshold() {
        let mr = merge_request(MergeRequestStatus::Open);
        let mr_store = Arc::new(FakeMergeRequests::new(vec![mr.clone()]));
        let executor = Arc::new(FakeMergeExecutor {
            outcome: MergeOutcome::Merged {
                commit_sha: "x".to_string(),
            },
            calls: Mutex::new(vec![]),
        });
        let repo = repository(mr.repository_id);
        let branch_reader = Arc::new(FakeBranchReader::new(vec![BranchInfo {
            name: "feature".to_string(),
            tip_sha: "sha1".to_string(),
            is_default: false,
        }]));
        let use_case = use_case(
            mr_store,
            executor.clone(),
            repo,
            Arc::new(FakePipelines::empty()),
            Arc::new(FakeJobs::empty()),
            1,
            branch_reader,
            Arc::new(FakeUsers::new(vec![user(Uuid::new_v4())])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case.execute(mr.id, mr.author_id.unwrap()).await;

        assert!(
            matches!(&result, Err(DomainError::Validation(msg)) if msg.contains("0/1")),
            "expected a 0/1 approvals message, got {result:?}"
        );
        assert!(
            executor.calls.lock().unwrap().is_empty(),
            "the executor must never be invoked while blocked"
        );
    }

    #[tokio::test]
    async fn merging_succeeds_once_enough_live_approvals_exist() {
        let mr = merge_request(MergeRequestStatus::Open);
        let review = MergeRequestReview {
            merge_request_id: mr.id,
            user_id: Uuid::new_v4(),
            username: "alice".to_string(),
            decision: ReviewDecision::Approved,
            source_sha: "sha1".to_string(),
            created_at: Utc::now(),
        };
        let mr_store =
            Arc::new(FakeMergeRequests::new(vec![mr.clone()]).with_reviews(vec![review]));
        let executor = Arc::new(FakeMergeExecutor {
            outcome: MergeOutcome::Merged {
                commit_sha: "x".to_string(),
            },
            calls: Mutex::new(vec![]),
        });
        let repo = repository(mr.repository_id);
        let branch_reader = Arc::new(FakeBranchReader::new(vec![BranchInfo {
            name: "feature".to_string(),
            tip_sha: "sha1".to_string(),
            is_default: false,
        }]));
        let use_case = use_case(
            mr_store,
            executor.clone(),
            repo,
            Arc::new(FakePipelines::empty()),
            Arc::new(FakeJobs::empty()),
            1,
            branch_reader,
            Arc::new(FakeUsers::new(vec![user(Uuid::new_v4())])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(mr.id, mr.author_id.unwrap())
            .await
            .unwrap();

        assert!(matches!(result, MergeMergeRequestResult::Merged(_)));
        assert_eq!(executor.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn merging_is_blocked_by_a_live_change_request_even_with_enough_approvals() {
        let mr = merge_request(MergeRequestStatus::Open);
        let approval = MergeRequestReview {
            merge_request_id: mr.id,
            user_id: Uuid::new_v4(),
            username: "alice".to_string(),
            decision: ReviewDecision::Approved,
            source_sha: "sha1".to_string(),
            created_at: Utc::now(),
        };
        let veto = MergeRequestReview {
            merge_request_id: mr.id,
            user_id: Uuid::new_v4(),
            username: "bob".to_string(),
            decision: ReviewDecision::ChangesRequested,
            source_sha: "sha1".to_string(),
            created_at: Utc::now(),
        };
        let mr_store =
            Arc::new(FakeMergeRequests::new(vec![mr.clone()]).with_reviews(vec![approval, veto]));
        let executor = Arc::new(FakeMergeExecutor {
            outcome: MergeOutcome::Merged {
                commit_sha: "x".to_string(),
            },
            calls: Mutex::new(vec![]),
        });
        let repo = repository(mr.repository_id);
        let branch_reader = Arc::new(FakeBranchReader::new(vec![BranchInfo {
            name: "feature".to_string(),
            tip_sha: "sha1".to_string(),
            is_default: false,
        }]));
        let use_case = use_case(
            mr_store,
            executor.clone(),
            repo,
            Arc::new(FakePipelines::empty()),
            Arc::new(FakeJobs::empty()),
            1,
            branch_reader,
            Arc::new(FakeUsers::new(vec![user(Uuid::new_v4())])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case.execute(mr.id, mr.author_id.unwrap()).await;

        assert!(
            matches!(&result, Err(DomainError::Validation(msg)) if msg.contains("bob")),
            "expected a message naming the blocking reviewer, got {result:?}"
        );
        assert!(executor.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_stale_approval_does_not_count_toward_the_threshold() {
        let mr = merge_request(MergeRequestStatus::Open);
        let stale_approval = MergeRequestReview {
            merge_request_id: mr.id,
            user_id: Uuid::new_v4(),
            username: "alice".to_string(),
            decision: ReviewDecision::Approved,
            source_sha: "old-sha".to_string(),
            created_at: Utc::now(),
        };
        let mr_store =
            Arc::new(FakeMergeRequests::new(vec![mr.clone()]).with_reviews(vec![stale_approval]));
        let executor = Arc::new(FakeMergeExecutor {
            outcome: MergeOutcome::Merged {
                commit_sha: "x".to_string(),
            },
            calls: Mutex::new(vec![]),
        });
        let repo = repository(mr.repository_id);
        let branch_reader = Arc::new(FakeBranchReader::new(vec![BranchInfo {
            name: "feature".to_string(),
            tip_sha: "new-sha".to_string(),
            is_default: false,
        }]));
        let use_case = use_case(
            mr_store,
            executor.clone(),
            repo,
            Arc::new(FakePipelines::empty()),
            Arc::new(FakeJobs::empty()),
            1,
            branch_reader,
            Arc::new(FakeUsers::new(vec![user(Uuid::new_v4())])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case.execute(mr.id, mr.author_id.unwrap()).await;

        assert!(
            matches!(&result, Err(DomainError::Validation(msg)) if msg.contains("0/1")),
            "a stale approval (old sha) must not count, got {result:?}"
        );
    }

    #[tokio::test]
    async fn a_stale_change_request_does_not_block() {
        let mr = merge_request(MergeRequestStatus::Open);
        let fresh_approval = MergeRequestReview {
            merge_request_id: mr.id,
            user_id: Uuid::new_v4(),
            username: "alice".to_string(),
            decision: ReviewDecision::Approved,
            source_sha: "new-sha".to_string(),
            created_at: Utc::now(),
        };
        let stale_veto = MergeRequestReview {
            merge_request_id: mr.id,
            user_id: Uuid::new_v4(),
            username: "bob".to_string(),
            decision: ReviewDecision::ChangesRequested,
            source_sha: "old-sha".to_string(),
            created_at: Utc::now(),
        };
        let mr_store = Arc::new(
            FakeMergeRequests::new(vec![mr.clone()]).with_reviews(vec![fresh_approval, stale_veto]),
        );
        let executor = Arc::new(FakeMergeExecutor {
            outcome: MergeOutcome::Merged {
                commit_sha: "x".to_string(),
            },
            calls: Mutex::new(vec![]),
        });
        let repo = repository(mr.repository_id);
        let branch_reader = Arc::new(FakeBranchReader::new(vec![BranchInfo {
            name: "feature".to_string(),
            tip_sha: "new-sha".to_string(),
            is_default: false,
        }]));
        let use_case = use_case(
            mr_store,
            executor.clone(),
            repo,
            Arc::new(FakePipelines::empty()),
            Arc::new(FakeJobs::empty()),
            1,
            branch_reader,
            Arc::new(FakeUsers::new(vec![user(Uuid::new_v4())])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(mr.id, mr.author_id.unwrap())
            .await
            .unwrap();

        assert!(
            matches!(result, MergeMergeRequestResult::Merged(_)),
            "a stale (old sha) change request must not block, got a different result"
        );
    }

    #[tokio::test]
    async fn the_authors_own_approval_counts_toward_the_threshold() {
        let mr = merge_request(MergeRequestStatus::Open);
        let self_approval = MergeRequestReview {
            merge_request_id: mr.id,
            user_id: mr.author_id.unwrap(),
            username: "author".to_string(),
            decision: ReviewDecision::Approved,
            source_sha: "sha1".to_string(),
            created_at: Utc::now(),
        };
        let mr_store =
            Arc::new(FakeMergeRequests::new(vec![mr.clone()]).with_reviews(vec![self_approval]));
        let executor = Arc::new(FakeMergeExecutor {
            outcome: MergeOutcome::Merged {
                commit_sha: "x".to_string(),
            },
            calls: Mutex::new(vec![]),
        });
        let repo = repository(mr.repository_id);
        let branch_reader = Arc::new(FakeBranchReader::new(vec![BranchInfo {
            name: "feature".to_string(),
            tip_sha: "sha1".to_string(),
            is_default: false,
        }]));
        let use_case = use_case(
            mr_store,
            executor.clone(),
            repo,
            Arc::new(FakePipelines::empty()),
            Arc::new(FakeJobs::empty()),
            1,
            branch_reader,
            Arc::new(FakeUsers::new(vec![user(Uuid::new_v4())])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(mr.id, mr.author_id.unwrap())
            .await
            .unwrap();

        assert!(
            matches!(result, MergeMergeRequestResult::Merged(_)),
            "self-approval must count toward the threshold"
        );
    }

    #[tokio::test]
    async fn merging_someone_elses_open_merge_request_notifies_its_author() {
        let mr = merge_request(MergeRequestStatus::Open);
        let merger_id = Uuid::new_v4();
        assert_ne!(Some(merger_id), mr.author_id);
        let mr_store = Arc::new(FakeMergeRequests::new(vec![mr.clone()]));
        let executor = Arc::new(FakeMergeExecutor {
            outcome: MergeOutcome::Merged {
                commit_sha: "deadbeef".to_string(),
            },
            calls: Mutex::new(vec![]),
        });
        let repo = repository(mr.repository_id);
        // Both the owner and the merger must resolve via `find_by_id` for the webhook/notification block.
        let owner_id = repo.owner_id;
        let notifications = Arc::new(FakeNotifications::empty());
        let webhooks = Arc::new(FakeWebhooks::default());
        let use_case = use_case(
            mr_store,
            executor.clone(),
            repo,
            Arc::new(FakePipelines::empty()),
            Arc::new(FakeJobs::empty()),
            0,
            Arc::new(FakeBranchReader::new(vec![])),
            Arc::new(FakeUsers::new(vec![user(owner_id), user(merger_id)])),
            notifications.clone(),
            webhooks.clone(),
        );

        let result = use_case.execute(mr.id, merger_id).await.unwrap();

        assert!(matches!(result, MergeMergeRequestResult::Merged(_)));
        let created = notifications.snapshot();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].recipient_id, mr.author_id.unwrap());
        assert_eq!(created[0].kind, NotificationKind::MergeRequestMerged);
        let dispatched = webhooks.dispatched();
        assert_eq!(dispatched.len(), 1);
        assert_eq!(dispatched[0].0, mr.repository_id);
        assert!(matches!(
            &dispatched[0].1,
            WebhookEvent::MergeRequestMerged { .. }
        ));
    }

    #[tokio::test]
    async fn merging_your_own_merge_request_still_dispatches_a_webhook_with_no_notification() {
        let mr = merge_request(MergeRequestStatus::Open);
        let author_id = mr.author_id.unwrap();
        let mr_store = Arc::new(FakeMergeRequests::new(vec![mr.clone()]));
        let executor = Arc::new(FakeMergeExecutor {
            outcome: MergeOutcome::Merged {
                commit_sha: "deadbeef".to_string(),
            },
            calls: Mutex::new(vec![]),
        });
        let repo = repository(mr.repository_id);
        let owner_id = repo.owner_id;
        let notifications = Arc::new(FakeNotifications::empty());
        let webhooks = Arc::new(FakeWebhooks::default());
        let use_case = use_case(
            mr_store,
            executor.clone(),
            repo,
            Arc::new(FakePipelines::empty()),
            Arc::new(FakeJobs::empty()),
            0,
            Arc::new(FakeBranchReader::new(vec![])),
            Arc::new(FakeUsers::new(vec![user(owner_id), user(author_id)])),
            notifications.clone(),
            webhooks.clone(),
        );

        let result = use_case.execute(mr.id, author_id).await.unwrap();

        assert!(matches!(result, MergeMergeRequestResult::Merged(_)));
        assert!(
            notifications.snapshot().is_empty(),
            "merging your own MR must not notify yourself"
        );
        let dispatched = webhooks.dispatched();
        assert_eq!(
            dispatched.len(),
            1,
            "the webhook must still fire even with no one to notify"
        );
        assert_eq!(dispatched[0].0, mr.repository_id);
        assert!(matches!(
            &dispatched[0].1,
            WebhookEvent::MergeRequestMerged { .. }
        ));
    }
}
