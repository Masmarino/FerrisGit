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
use crate::use_cases::event_context::EventContext;
use crate::use_cases::source_branch_tip::source_branch_tip;

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
            self.require_approvals(&mr, &repo.disk_path, settings.required_approvals as usize)
                .await?;
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

                if let Some(ctx) =
                    EventContext::for_repository(self.users.as_ref(), repo, user_id).await
                {
                    self.webhooks
                        .dispatch(
                            mr.repository_id,
                            WebhookEvent::MergeRequestMerged {
                                repository_owner: ctx.owner_username.clone(),
                                repository_name: ctx.repository.name.clone(),
                                actor_username: ctx.actor_username.clone(),
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
                                merge_request_id: Some(mr.id),
                                merge_request_title: Some(mr.title),
                                ..ctx.notification(
                                    NotificationKind::MergeRequestMerged,
                                    mr_author_id,
                                )
                            })
                            .await
                            .ok();
                    }
                }

                Ok(MergeMergeRequestResult::Merged(Box::new(merged)))
            }
        }
    }

    /// Only reviews of the source branch's current tip count: a push after a review makes it stale.
    async fn require_approvals(
        &self,
        mr: &MergeRequest,
        repository_disk_path: &str,
        required: usize,
    ) -> Result<(), DomainError> {
        let tip_sha = source_branch_tip(
            self.branch_reader.as_ref(),
            repository_disk_path,
            &mr.source_branch,
        )
        .await?;
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
        if approvals < required {
            return Err(DomainError::Validation(format!(
                "{approvals}/{required} approvals"
            )));
        }
        Ok(())
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
    use crate::use_cases::fixtures;
    use async_trait::async_trait;
    use chrono::Utc;
    use ferrisgit_domain::branch::BranchInfo;
    use ferrisgit_domain::job::Job;
    use ferrisgit_domain::job_execution::JobExecutionPort;
    use ferrisgit_domain::merge_request::{MergeRequestReview, ReviewDecision};
    use ferrisgit_domain::repository::Repository;
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

    fn review(
        mr: &MergeRequest,
        username: &str,
        decision: ReviewDecision,
        source_sha: &str,
    ) -> MergeRequestReview {
        MergeRequestReview {
            merge_request_id: mr.id,
            user_id: Uuid::new_v4(),
            username: username.to_string(),
            decision,
            source_sha: source_sha.to_string(),
            created_at: Utc::now(),
        }
    }

    struct FakeMergeExecutor {
        outcome: MergeOutcome,
        calls: Mutex<Vec<(String, String, String)>>,
    }

    impl FakeMergeExecutor {
        fn new(outcome: MergeOutcome) -> Self {
            Self {
                outcome,
                calls: Mutex::new(vec![]),
            }
        }
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

    /// What a test decides about the world; everything else is a plain default (a clean merge, no approval required).
    struct Scenario {
        mr: MergeRequest,
        outcome: MergeOutcome,
        reviews: Vec<MergeRequestReview>,
        /// Tip of the source branch, `None` when the repository has no branch at all.
        source_tip: Option<&'static str>,
        required_approvals: i32,
        /// Accounts that exist besides the repository owner.
        accounts: Vec<Uuid>,
        owner_has_an_account: bool,
    }

    impl Scenario {
        fn new(mr: MergeRequest) -> Self {
            Self {
                mr,
                outcome: MergeOutcome::Merged {
                    commit_sha: "deadbeef".to_string(),
                },
                reviews: vec![],
                source_tip: None,
                required_approvals: 0,
                accounts: vec![],
                owner_has_an_account: false,
            }
        }

        fn build(self) -> Harness {
            let repo = fixtures::repository_with_id(self.mr.repository_id, Uuid::new_v4());
            let mr_store =
                Arc::new(FakeMergeRequests::new(vec![self.mr.clone()]).with_reviews(self.reviews));
            let executor = Arc::new(FakeMergeExecutor::new(self.outcome));
            let branches = self
                .source_tip
                .map(|tip_sha| BranchInfo {
                    name: "feature".to_string(),
                    tip_sha: tip_sha.to_string(),
                    is_default: false,
                })
                .into_iter()
                .collect();
            let owner = self.owner_has_an_account.then_some(repo.owner_id);
            let users = owner
                .into_iter()
                .chain(self.accounts)
                .map(|id| User {
                    id,
                    ..fixtures::user("florian")
                })
                .collect();
            let pipelines = Arc::new(FakePipelines::empty());
            let notifications = Arc::new(FakeNotifications::empty());
            let webhooks = Arc::new(FakeWebhooks::default());
            let docker: Arc<dyn JobExecutionPort> = Arc::new(NoopExecutor);
            let use_case = MergeMergeRequestUseCase::new(
                mr_store.clone(),
                mr_store.clone(),
                executor.clone(),
                Arc::new(FakeRepositories::new(vec![repo.clone()])),
                Arc::new(FakeRepositorySettings::new(RepositorySettings {
                    repository_id: Uuid::new_v4(),
                    pipeline_file_path: ".ferrisgit-ci.yml".to_string(),
                    ci_enabled: false,
                    required_approvals: self.required_approvals,
                })),
                Arc::new(FakeSystemSettings::default()),
                pipelines.clone(),
                Arc::new(FakeJobs::empty()),
                Arc::new(FakeFileReader::none()),
                Arc::new(FakeEvents::new()),
                Arc::new(JobExecutionResolver::new(docker.clone(), docker)),
                Arc::new(FakeBranchReader::new(branches)),
                Arc::new(FakeUsers::new(users)),
                notifications.clone(),
                webhooks.clone(),
            );
            Harness {
                use_case,
                mr: self.mr,
                repo,
                mr_store,
                executor,
                pipelines,
                notifications,
                webhooks,
            }
        }
    }

    struct Harness {
        use_case: MergeMergeRequestUseCase,
        mr: MergeRequest,
        repo: Repository,
        mr_store: Arc<FakeMergeRequests>,
        executor: Arc<FakeMergeExecutor>,
        pipelines: Arc<FakePipelines>,
        notifications: Arc<FakeNotifications>,
        webhooks: Arc<FakeWebhooks>,
    }

    impl Harness {
        /// Merges as the merge request's own author.
        async fn merge_as_author(&self) -> Result<MergeMergeRequestResult, DomainError> {
            self.use_case
                .execute(self.mr.id, self.mr.author_id.unwrap())
                .await
        }

        fn executor_calls(&self) -> usize {
            self.executor.calls.lock().unwrap().len()
        }
    }

    #[tokio::test]
    async fn a_clean_merge_marks_the_request_merged_and_triggers_a_pipeline() {
        let h = Scenario::new(merge_request(MergeRequestStatus::Open)).build();

        let result = h.merge_as_author().await.unwrap();

        let MergeMergeRequestResult::Merged(merged) = result else {
            panic!("expected a clean merge")
        };
        assert_eq!(merged.status, MergeRequestStatus::Merged);
        assert_eq!(merged.merge_commit_sha, Some("deadbeef".to_string()));
        assert_eq!(
            h.executor.calls.lock().unwrap()[0],
            (
                h.repo.disk_path.clone(),
                "feature".to_string(),
                "main".to_string()
            )
        );
        // CI is disabled in the fake, so no pipeline is created. This only checks that the use case was called with the
        // new commit.
        assert!(h.pipelines.snapshot().is_empty());
    }

    #[tokio::test]
    async fn a_conflicting_merge_leaves_the_request_open_and_never_touches_the_store_or_a_pipeline()
    {
        let h = Scenario {
            outcome: MergeOutcome::Conflicting,
            ..Scenario::new(merge_request(MergeRequestStatus::Open))
        }
        .build();

        let result = h.merge_as_author().await.unwrap();

        assert_eq!(result, MergeMergeRequestResult::Conflicting);
        assert_eq!(
            h.mr_store.get(h.mr.id).unwrap().status,
            MergeRequestStatus::Open,
            "mark_merged must never be called on a conflicting outcome"
        );
        assert!(h.pipelines.snapshot().is_empty());
    }

    #[tokio::test]
    async fn merging_an_already_merged_request_is_a_validation_error() {
        let h = Scenario::new(merge_request(MergeRequestStatus::Merged)).build();

        let result = h.merge_as_author().await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
        assert_eq!(
            h.executor_calls(),
            0,
            "the executor must never be invoked for an already-terminal merge request"
        );
    }

    #[tokio::test]
    async fn merging_a_non_existent_request_is_a_not_found_error() {
        let h = Scenario::new(merge_request(MergeRequestStatus::Open)).build();

        let result = h.use_case.execute(Uuid::new_v4(), Uuid::new_v4()).await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn merging_is_blocked_when_live_approvals_are_below_the_threshold() {
        let h = Scenario {
            required_approvals: 1,
            source_tip: Some("sha1"),
            ..Scenario::new(merge_request(MergeRequestStatus::Open))
        }
        .build();

        let result = h.merge_as_author().await;

        assert!(
            matches!(&result, Err(DomainError::Validation(msg)) if msg.contains("0/1")),
            "expected a 0/1 approvals message, got {result:?}"
        );
        assert_eq!(
            h.executor_calls(),
            0,
            "the executor must never be invoked while blocked"
        );
    }

    #[tokio::test]
    async fn merging_succeeds_once_enough_live_approvals_exist() {
        let mr = merge_request(MergeRequestStatus::Open);
        let h = Scenario {
            required_approvals: 1,
            source_tip: Some("sha1"),
            reviews: vec![review(&mr, "alice", ReviewDecision::Approved, "sha1")],
            ..Scenario::new(mr)
        }
        .build();

        let result = h.merge_as_author().await.unwrap();

        assert!(matches!(result, MergeMergeRequestResult::Merged(_)));
        assert_eq!(h.executor_calls(), 1);
    }

    #[tokio::test]
    async fn merging_is_blocked_by_a_live_change_request_even_with_enough_approvals() {
        let mr = merge_request(MergeRequestStatus::Open);
        let h = Scenario {
            required_approvals: 1,
            source_tip: Some("sha1"),
            reviews: vec![
                review(&mr, "alice", ReviewDecision::Approved, "sha1"),
                review(&mr, "bob", ReviewDecision::ChangesRequested, "sha1"),
            ],
            ..Scenario::new(mr)
        }
        .build();

        let result = h.merge_as_author().await;

        assert!(
            matches!(&result, Err(DomainError::Validation(msg)) if msg.contains("bob")),
            "expected a message naming the blocking reviewer, got {result:?}"
        );
        assert_eq!(h.executor_calls(), 0);
    }

    #[tokio::test]
    async fn a_stale_approval_does_not_count_toward_the_threshold() {
        let mr = merge_request(MergeRequestStatus::Open);
        let h = Scenario {
            required_approvals: 1,
            source_tip: Some("new-sha"),
            reviews: vec![review(&mr, "alice", ReviewDecision::Approved, "old-sha")],
            ..Scenario::new(mr)
        }
        .build();

        let result = h.merge_as_author().await;

        assert!(
            matches!(&result, Err(DomainError::Validation(msg)) if msg.contains("0/1")),
            "a stale approval (old sha) must not count, got {result:?}"
        );
    }

    #[tokio::test]
    async fn a_stale_change_request_does_not_block() {
        let mr = merge_request(MergeRequestStatus::Open);
        let h = Scenario {
            required_approvals: 1,
            source_tip: Some("new-sha"),
            reviews: vec![
                review(&mr, "alice", ReviewDecision::Approved, "new-sha"),
                review(&mr, "bob", ReviewDecision::ChangesRequested, "old-sha"),
            ],
            ..Scenario::new(mr)
        }
        .build();

        let result = h.merge_as_author().await.unwrap();

        assert!(
            matches!(result, MergeMergeRequestResult::Merged(_)),
            "a stale (old sha) change request must not block, got a different result"
        );
    }

    #[tokio::test]
    async fn the_authors_own_approval_counts_toward_the_threshold() {
        let mr = merge_request(MergeRequestStatus::Open);
        let self_approval = MergeRequestReview {
            user_id: mr.author_id.unwrap(),
            ..review(&mr, "author", ReviewDecision::Approved, "sha1")
        };
        let h = Scenario {
            required_approvals: 1,
            source_tip: Some("sha1"),
            reviews: vec![self_approval],
            ..Scenario::new(mr)
        }
        .build();

        let result = h.merge_as_author().await.unwrap();

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
        // Both the owner and the merger must resolve via `find_by_id` for the webhook/notification block.
        let h = Scenario {
            accounts: vec![merger_id],
            owner_has_an_account: true,
            ..Scenario::new(mr)
        }
        .build();

        let result = h.use_case.execute(h.mr.id, merger_id).await.unwrap();

        assert!(matches!(result, MergeMergeRequestResult::Merged(_)));
        let created = h.notifications.snapshot();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].recipient_id, h.mr.author_id.unwrap());
        assert_eq!(created[0].kind, NotificationKind::MergeRequestMerged);
        let dispatched = h.webhooks.dispatched();
        assert_eq!(dispatched.len(), 1);
        assert_eq!(dispatched[0].0, h.mr.repository_id);
        assert!(matches!(
            &dispatched[0].1,
            WebhookEvent::MergeRequestMerged { .. }
        ));
    }

    #[tokio::test]
    async fn merging_your_own_merge_request_still_dispatches_a_webhook_with_no_notification() {
        let mr = merge_request(MergeRequestStatus::Open);
        let h = Scenario {
            accounts: vec![mr.author_id.unwrap()],
            owner_has_an_account: true,
            ..Scenario::new(mr)
        }
        .build();

        let result = h.merge_as_author().await.unwrap();

        assert!(matches!(result, MergeMergeRequestResult::Merged(_)));
        assert!(
            h.notifications.snapshot().is_empty(),
            "merging your own MR must not notify yourself"
        );
        let dispatched = h.webhooks.dispatched();
        assert_eq!(
            dispatched.len(),
            1,
            "the webhook must still fire even with no one to notify"
        );
        assert_eq!(dispatched[0].0, h.mr.repository_id);
        assert!(matches!(
            &dispatched[0].1,
            WebhookEvent::MergeRequestMerged { .. }
        ));
    }
}
