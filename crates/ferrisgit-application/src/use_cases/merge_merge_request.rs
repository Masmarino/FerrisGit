use std::sync::Arc;

use ferrisgit_domain::branch::BranchReaderPort;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::JobStorePort;
use ferrisgit_domain::merge_executor::{
    CommitAuthor, MergeExecutorPort, MergeMethod, MergeOutcome, MergeRequestMerge,
};
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
use crate::use_cases::merge_request_pipeline::pipeline_requirement_problem;
use crate::use_cases::source_branch_tip::source_branch_tip;

#[derive(Debug)]
pub enum MergeMergeRequestResult {
    /// The merge request as merged, and the method it was merged with.
    Merged(Box<MergeRequest>, MergeMethod),
    Conflicting,
}

impl PartialEq for MergeMergeRequestResult {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (MergeMergeRequestResult::Conflicting, MergeMergeRequestResult::Conflicting) => true,
            (
                MergeMergeRequestResult::Merged(a, a_method),
                MergeMergeRequestResult::Merged(b, b_method),
            ) => {
                a.id == b.id
                    && a.status == b.status
                    && a.merge_commit_sha == b.merge_commit_sha
                    && a_method == b_method
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

    /// Clean merges only, with `method` or else the repository's default one. The pipeline on the new tip is started
    /// here, since the merge never goes through the push path that normally triggers one.
    pub async fn execute(
        &self,
        merge_request_id: Uuid,
        user_id: Uuid,
        method: Option<MergeMethod>,
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
        if settings.pipeline_must_succeed {
            let tip_sha = source_branch_tip(
                self.branch_reader.as_ref(),
                &repo.disk_path,
                &mr.source_branch,
            )
            .await?;
            let pipelines = self.pipelines.list_for_merge_request(mr.id).await?;
            if let Some(problem) = pipeline_requirement_problem(&pipelines, &tip_sha) {
                return Err(DomainError::Validation(problem));
            }
        }

        let method = method.unwrap_or(settings.default_merge_method);
        let (message, squash_author) = match method {
            MergeMethod::Squash => (squash_message(&mr), self.squash_author(&mr).await?),
            MergeMethod::Merge | MergeMethod::Rebase => (
                format!(
                    "Merge branch '{}' into {}",
                    mr.source_branch, mr.target_branch
                ),
                None,
            ),
        };
        let outcome = self
            .merge_executor
            .merge(
                &repo.disk_path,
                &MergeRequestMerge {
                    source_branch: mr.source_branch.clone(),
                    target_branch: mr.target_branch.clone(),
                    method,
                    message,
                    squash_author,
                },
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

                    // The author's account may be gone, then there's nobody to notify.
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

                Ok(MergeMergeRequestResult::Merged(Box::new(merged), method))
            }
        }
    }

    /// The author of the merge request, whose change the squash commit is. `None` once their account is gone.
    async fn squash_author(&self, mr: &MergeRequest) -> Result<Option<CommitAuthor>, DomainError> {
        let Some(author_id) = mr.author_id else {
            return Ok(None);
        };
        Ok(self
            .users
            .find_by_id(author_id)
            .await?
            .map(|user| CommitAuthor {
                name: user.username,
                email: user.email,
            }))
    }

    /// Only reviews of the current tip count, a push after a review makes it stale.
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

/// A squash commit says what the merge request did: its title, then its description when it has one.
fn squash_message(mr: &MergeRequest) -> String {
    let description = mr.description.trim();
    if description.is_empty() {
        mr.title.clone()
    } else {
        format!("{}\n\n{description}", mr.title)
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
    use ferrisgit_domain::pipeline::{MergeRequestPipeline, Pipeline, PipelineStatus};
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
        requests: Mutex<Vec<MergeRequestMerge>>,
    }

    impl FakeMergeExecutor {
        fn new(outcome: MergeOutcome) -> Self {
            Self {
                outcome,
                calls: Mutex::new(vec![]),
                requests: Mutex::new(vec![]),
            }
        }
    }

    #[async_trait]
    impl MergeExecutorPort for FakeMergeExecutor {
        async fn merge(
            &self,
            repository_disk_path: &str,
            merge: &MergeRequestMerge,
        ) -> Result<MergeOutcome, DomainError> {
            self.calls.lock().unwrap().push((
                repository_disk_path.to_string(),
                merge.source_branch.clone(),
                merge.target_branch.clone(),
            ));
            self.requests.lock().unwrap().push(merge.clone());
            Ok(self.outcome.clone())
        }
    }

    /// Never called (the fake settings have CI off), it just fills the resolver's constructor.
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

    /// Defaults to a clean merge with no approval required.
    struct Scenario {
        mr: MergeRequest,
        outcome: MergeOutcome,
        reviews: Vec<MergeRequestReview>,
        /// `None` means the repository has no branch at all.
        source_tip: Option<&'static str>,
        required_approvals: i32,
        pipeline_must_succeed: bool,
        /// Merge request pipelines already run.
        pipelines: Vec<Pipeline>,
        default_merge_method: MergeMethod,
        /// Accounts besides the repository owner.
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
                pipeline_must_succeed: false,
                pipelines: vec![],
                default_merge_method: MergeMethod::Merge,
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
            let pipelines = Arc::new(FakePipelines::new(self.pipelines));
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
                    pipeline_must_succeed: self.pipeline_must_succeed,
                    default_merge_method: self.default_merge_method,
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
        async fn merge_as_author(&self) -> Result<MergeMergeRequestResult, DomainError> {
            self.use_case
                .execute(self.mr.id, self.mr.author_id.unwrap(), None)
                .await
        }

        fn executor_calls(&self) -> usize {
            self.executor.calls.lock().unwrap().len()
        }
    }

    #[tokio::test]
    async fn a_clean_merge_marks_the_request_merged_on_the_target() {
        let h = Scenario::new(merge_request(MergeRequestStatus::Open)).build();

        let result = h.merge_as_author().await.unwrap();

        let MergeMergeRequestResult::Merged(merged, _) = result else {
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
        // CI is off in the fake settings, so the pipeline step runs but creates nothing.
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

        let result = h
            .use_case
            .execute(Uuid::new_v4(), Uuid::new_v4(), None)
            .await;

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

        assert!(matches!(result, MergeMergeRequestResult::Merged(..)));
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
            matches!(result, MergeMergeRequestResult::Merged(..)),
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
            matches!(result, MergeMergeRequestResult::Merged(..)),
            "self-approval must count toward the threshold"
        );
    }

    #[tokio::test]
    async fn merging_someone_elses_open_merge_request_notifies_its_author() {
        let mr = merge_request(MergeRequestStatus::Open);
        let merger_id = Uuid::new_v4();
        assert_ne!(Some(merger_id), mr.author_id);
        // The webhook/notification block needs both the owner and the merger to exist as users.
        let h = Scenario {
            accounts: vec![merger_id],
            owner_has_an_account: true,
            ..Scenario::new(mr)
        }
        .build();

        let result = h.use_case.execute(h.mr.id, merger_id, None).await.unwrap();

        assert!(matches!(result, MergeMergeRequestResult::Merged(..)));
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

        assert!(matches!(result, MergeMergeRequestResult::Merged(..)));
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
            ..fixtures::pipeline(Uuid::new_v4(), status)
        }
    }

    #[tokio::test]
    async fn with_pipelines_required_only_a_successful_pipeline_of_the_current_source_commit_lets_the_merge_through()
     {
        let mr = merge_request(MergeRequestStatus::Open);
        for (pipelines, merges) in [
            (vec![], false),
            (
                vec![merge_request_pipeline(&mr, "tip1", PipelineStatus::Running)],
                false,
            ),
            (
                vec![merge_request_pipeline(&mr, "tip1", PipelineStatus::Failed)],
                false,
            ),
            (
                vec![merge_request_pipeline(&mr, "old", PipelineStatus::Success)],
                false,
            ),
            (
                vec![merge_request_pipeline(&mr, "tip1", PipelineStatus::Success)],
                true,
            ),
        ] {
            let h = Scenario {
                source_tip: Some("tip1"),
                pipeline_must_succeed: true,
                pipelines,
                ..Scenario::new(mr.clone())
            }
            .build();

            let result = h.merge_as_author().await;

            if merges {
                assert!(
                    matches!(result, Ok(MergeMergeRequestResult::Merged(..))),
                    "{result:?}"
                );
            } else {
                assert!(
                    matches!(&result, Err(DomainError::Validation(m)) if m.contains("pipeline")),
                    "{result:?}"
                );
                assert_eq!(h.executor_calls(), 0);
            }
        }
    }

    #[tokio::test]
    async fn the_repository_default_method_applies_unless_the_merge_names_another() {
        let mr = merge_request(MergeRequestStatus::Open);
        for (requested, expected) in [
            (None, MergeMethod::Rebase),
            (Some(MergeMethod::Merge), MergeMethod::Merge),
            (Some(MergeMethod::Squash), MergeMethod::Squash),
        ] {
            let h = Scenario {
                default_merge_method: MergeMethod::Rebase,
                ..Scenario::new(mr.clone())
            }
            .build();

            h.use_case
                .execute(h.mr.id, h.mr.author_id.unwrap(), requested)
                .await
                .unwrap();

            assert_eq!(h.executor.requests.lock().unwrap()[0].method, expected);
        }
    }

    #[tokio::test]
    async fn a_squash_commit_is_titled_after_the_merge_request_and_attributed_to_its_author() {
        let mr = MergeRequest {
            description: "Adds the feature.\n".to_string(),
            ..merge_request(MergeRequestStatus::Open)
        };
        let author_id = mr.author_id.unwrap();
        let h = Scenario {
            accounts: vec![author_id],
            ..Scenario::new(mr)
        }
        .build();

        h.use_case
            .execute(h.mr.id, author_id, Some(MergeMethod::Squash))
            .await
            .unwrap();

        let request = h.executor.requests.lock().unwrap()[0].clone();
        assert_eq!(request.message, "Add feature\n\nAdds the feature.");
        assert_eq!(
            request.squash_author,
            Some(CommitAuthor {
                name: "florian".to_string(),
                email: "florian@example.com".to_string(),
            })
        );
    }

    #[test]
    fn a_squash_message_without_description_is_the_title_alone() {
        let mr = merge_request(MergeRequestStatus::Open);
        assert_eq!(squash_message(&mr), "Add feature");
    }
}
