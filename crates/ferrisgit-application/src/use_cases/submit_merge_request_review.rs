use std::sync::Arc;

use ferrisgit_domain::branch::BranchReaderPort;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::merge_request::{
    MergeRequestReview, MergeRequestReviewPort, MergeRequestStorePort, ReviewDecision,
};
use ferrisgit_domain::notification::{NewNotification, NotificationKind, NotificationStorePort};
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::user::UserRepositoryPort;
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use ferrisgit_domain::webhook_event::WebhookEvent;
use uuid::Uuid;

use super::event_context::EventContext;
use super::source_branch_tip::source_branch_tip;

pub struct SubmitMergeRequestReviewUseCase {
    merge_requests: Arc<dyn MergeRequestStorePort>,
    merge_request_reviews: Arc<dyn MergeRequestReviewPort>,
    branch_reader: Arc<dyn BranchReaderPort>,
    repositories: Arc<dyn RepositoryStorePort>,
    users: Arc<dyn UserRepositoryPort>,
    notifications: Arc<dyn NotificationStorePort>,
    webhooks: Arc<dyn WebhookDispatcherPort>,
}

impl SubmitMergeRequestReviewUseCase {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        merge_requests: Arc<dyn MergeRequestStorePort>,
        merge_request_reviews: Arc<dyn MergeRequestReviewPort>,
        branch_reader: Arc<dyn BranchReaderPort>,
        repositories: Arc<dyn RepositoryStorePort>,
        users: Arc<dyn UserRepositoryPort>,
        notifications: Arc<dyn NotificationStorePort>,
        webhooks: Arc<dyn WebhookDispatcherPort>,
    ) -> Self {
        Self {
            merge_requests,
            merge_request_reviews,
            branch_reader,
            repositories,
            users,
            notifications,
            webhooks,
        }
    }

    /// Doesn't check access (owner or collaborator), the caller does, as with `AddMergeRequestCommentUseCase`.
    pub async fn execute(
        &self,
        merge_request_id: Uuid,
        repository_disk_path: &str,
        source_branch: &str,
        user_id: Uuid,
        decision: ReviewDecision,
    ) -> Result<MergeRequestReview, DomainError> {
        if decision == ReviewDecision::Approved
            && let Ok(Some(mr)) = self.merge_requests.find_by_id(merge_request_id).await
            && mr.author_id == Some(user_id)
        {
            return Err(DomainError::Validation(
                "a merge request's author cannot approve their own merge request".to_string(),
            ));
        }

        let tip_sha = source_branch_tip(
            self.branch_reader.as_ref(),
            repository_disk_path,
            source_branch,
        )
        .await?;
        let previous_review = self
            .merge_request_reviews
            .list_reviews(merge_request_id)
            .await
            .ok()
            .and_then(|reviews| reviews.into_iter().find(|r| r.user_id == user_id));
        let review = self
            .merge_request_reviews
            .upsert_review(merge_request_id, user_id, decision, &tip_sha)
            .await?;
        let unchanged = previous_review
            .as_ref()
            .is_some_and(|r| r.decision == decision && r.source_sha == tip_sha);

        if !unchanged
            && let Some(mr) = self
                .merge_requests
                .find_by_id(merge_request_id)
                .await
                .ok()
                .flatten()
            && let Some(ctx) = EventContext::load(
                self.repositories.as_ref(),
                self.users.as_ref(),
                mr.repository_id,
                user_id,
            )
            .await
        {
            let webhook_event = if decision == ReviewDecision::Approved {
                WebhookEvent::MergeRequestApproved {
                    repository_owner: ctx.owner_username.clone(),
                    repository_name: ctx.repository.name.clone(),
                    actor_username: ctx.actor_username.clone(),
                    merge_request_id: mr.id,
                    merge_request_title: mr.title.clone(),
                }
            } else {
                WebhookEvent::MergeRequestChangesRequested {
                    repository_owner: ctx.owner_username.clone(),
                    repository_name: ctx.repository.name.clone(),
                    actor_username: ctx.actor_username.clone(),
                    merge_request_id: mr.id,
                    merge_request_title: mr.title.clone(),
                }
            };
            self.webhooks
                .dispatch(mr.repository_id, webhook_event)
                .await
                .ok();

            // The author's account may be gone, then there's nobody to notify.
            if let Some(mr_author_id) = mr.author_id
                && mr_author_id != user_id
            {
                let kind = if decision == ReviewDecision::Approved {
                    NotificationKind::MergeRequestApproved
                } else {
                    NotificationKind::MergeRequestChangesRequested
                };
                self.notifications
                    .create(NewNotification {
                        merge_request_id: Some(mr.id),
                        merge_request_title: Some(mr.title),
                        ..ctx.notification(kind, mr_author_id)
                    })
                    .await
                    .ok();
            }
        }

        Ok(review)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeBranchReader, FakeMergeRequests, FakeNotifications, FakeRepositories, FakeUsers,
        FakeWebhooks,
    };
    use crate::use_cases::fixtures;
    use chrono::Utc;
    use ferrisgit_domain::branch::BranchInfo;
    use ferrisgit_domain::merge_request::{MergeRequest, MergeRequestStatus};

    struct Harness {
        use_case: SubmitMergeRequestReviewUseCase,
        mr: MergeRequest,
        author_id: Uuid,
        reviewer_id: Uuid,
        mr_store: Arc<FakeMergeRequests>,
        notifications: Arc<FakeNotifications>,
        webhooks: Arc<FakeWebhooks>,
    }

    impl Harness {
        /// An open merge request by `author`, its source branch `feature` at `sha1`.
        fn new() -> Self {
            Self::build(true, &[("feature", "sha1")])
        }

        fn build(with_merge_request: bool, branches: &[(&str, &str)]) -> Self {
            let owner = fixtures::user("owner");
            let author = fixtures::user("author");
            let reviewer = fixtures::user("reviewer");
            let repository = fixtures::repository(owner.id);
            let mr = MergeRequest {
                id: Uuid::new_v4(),
                repository_id: repository.id,
                author_id: Some(author.id),
                source_branch: "feature".to_string(),
                target_branch: "main".to_string(),
                title: "Add feature".to_string(),
                description: String::new(),
                status: MergeRequestStatus::Open,
                merge_commit_sha: None,
                milestone_id: None,
                created_at: Utc::now(),
                closed_at: None,
            };
            let mr_store = Arc::new(if with_merge_request {
                FakeMergeRequests::new(vec![mr.clone()])
            } else {
                FakeMergeRequests::empty()
            });
            let branches = branches
                .iter()
                .map(|(name, tip_sha)| BranchInfo {
                    name: name.to_string(),
                    tip_sha: tip_sha.to_string(),
                    is_default: false,
                })
                .collect();
            let notifications = Arc::new(FakeNotifications::empty());
            let webhooks = Arc::new(FakeWebhooks::default());
            Self {
                use_case: SubmitMergeRequestReviewUseCase::new(
                    mr_store.clone(),
                    mr_store.clone(),
                    Arc::new(FakeBranchReader::new(branches)),
                    Arc::new(FakeRepositories::new(vec![repository])),
                    Arc::new(FakeUsers::new(vec![
                        owner.clone(),
                        author.clone(),
                        reviewer.clone(),
                    ])),
                    notifications.clone(),
                    webhooks.clone(),
                ),
                mr,
                author_id: author.id,
                reviewer_id: reviewer.id,
                mr_store,
                notifications,
                webhooks,
            }
        }

        async fn review(
            &self,
            user_id: Uuid,
            decision: ReviewDecision,
        ) -> Result<MergeRequestReview, DomainError> {
            self.use_case
                .execute(self.mr.id, "disk/path", "feature", user_id, decision)
                .await
        }
    }

    #[tokio::test]
    async fn submitting_a_review_records_it_against_the_source_branchs_current_tip() {
        let h = Harness::build(false, &[("feature", "sha1")]);

        let review = h
            .review(h.reviewer_id, ReviewDecision::Approved)
            .await
            .unwrap();

        assert_eq!(review.source_sha, "sha1");
        assert_eq!(review.decision, ReviewDecision::Approved);
        let reviews = h.mr_store.reviews_snapshot();
        assert_eq!(reviews.len(), 1);
        assert_eq!(reviews[0].merge_request_id, h.mr.id);
        assert_eq!(reviews[0].user_id, h.reviewer_id);
        assert_eq!(reviews[0].decision, ReviewDecision::Approved);
        assert_eq!(reviews[0].source_sha, "sha1");
    }

    #[tokio::test]
    async fn reviewing_when_the_source_branch_no_longer_exists_is_a_validation_error() {
        let h = Harness::build(false, &[]);

        let result = h.review(h.reviewer_id, ReviewDecision::Approved).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn approving_someone_elses_merge_request_notifies_its_author() {
        let h = Harness::new();

        h.review(h.reviewer_id, ReviewDecision::Approved)
            .await
            .unwrap();

        let created = h.notifications.snapshot();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].recipient_id, h.author_id);
        assert_eq!(created[0].kind, NotificationKind::MergeRequestApproved);
    }

    #[tokio::test]
    async fn self_approving_your_own_merge_request_is_rejected() {
        let h = Harness::new();

        let result = h.review(h.author_id, ReviewDecision::Approved).await;

        assert!(
            matches!(result, Err(DomainError::Validation(_))),
            "a merge request's author must not be able to approve their own merge request"
        );
        assert!(h.notifications.snapshot().is_empty());
        assert!(
            h.webhooks.dispatched().is_empty(),
            "a rejected self-approval must not dispatch a webhook"
        );
    }

    #[tokio::test]
    async fn requesting_changes_on_your_own_merge_request_is_still_allowed() {
        let h = Harness::new();

        let review = h
            .review(h.author_id, ReviewDecision::ChangesRequested)
            .await
            .unwrap();

        assert_eq!(review.decision, ReviewDecision::ChangesRequested);
        assert!(
            h.webhooks.dispatched().len() == 1,
            "self-review is only blocked for approvals, not for requesting changes on your own work"
        );
    }

    #[tokio::test]
    async fn resubmitting_the_same_decision_on_the_same_commit_does_not_re_notify() {
        let h = Harness::new();

        h.review(h.reviewer_id, ReviewDecision::Approved)
            .await
            .unwrap();
        h.review(h.reviewer_id, ReviewDecision::Approved)
            .await
            .unwrap();

        let created = h.notifications.snapshot();
        assert_eq!(
            created.len(),
            1,
            "resubmitting the identical decision on the same commit must not send a second notification"
        );
        let dispatched = h.webhooks.dispatched();
        assert_eq!(
            dispatched.len(),
            1,
            "resubmitting the identical decision on the same commit must not dispatch a second webhook either"
        );
    }
}
