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

    /// Access control (owner or collaborator) is up to the caller. Like `AddMergeRequestCommentUseCase`, this use case
    /// does not check it again.
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

        let branches = self
            .branch_reader
            .list_branches(repository_disk_path)
            .await?;
        let tip_sha = branches
            .into_iter()
            .find(|b| b.name == source_branch)
            .map(|b| b.tip_sha)
            .ok_or_else(|| DomainError::Validation("source branch no longer exists".to_string()))?;
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
            && let Some(repo) = self
                .repositories
                .find_by_id(mr.repository_id)
                .await
                .ok()
                .flatten()
            && let Some(owner) = self.users.find_by_id(repo.owner_id).await.ok().flatten()
            && let Some(actor) = self.users.find_by_id(user_id).await.ok().flatten()
        {
            let webhook_event = if decision == ReviewDecision::Approved {
                WebhookEvent::MergeRequestApproved {
                    repository_owner: owner.username.clone(),
                    repository_name: repo.name.clone(),
                    actor_username: actor.username.clone(),
                    merge_request_id: mr.id,
                    merge_request_title: mr.title.clone(),
                }
            } else {
                WebhookEvent::MergeRequestChangesRequested {
                    repository_owner: owner.username.clone(),
                    repository_name: repo.name.clone(),
                    actor_username: actor.username.clone(),
                    merge_request_id: mr.id,
                    merge_request_title: mr.title.clone(),
                }
            };
            self.webhooks
                .dispatch(mr.repository_id, webhook_event)
                .await
                .ok();

            // Nobody to notify once the author's account is gone.
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
                        recipient_id: mr_author_id,
                        kind,
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
    use chrono::Utc;
    use ferrisgit_domain::branch::BranchInfo;
    use ferrisgit_domain::merge_request::{MergeRequest, MergeRequestStatus};
    use ferrisgit_domain::repository::Repository;
    use ferrisgit_domain::user::User;

    #[tokio::test]
    async fn submitting_a_review_records_it_against_the_source_branchs_current_tip() {
        let mr_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let mr_store = Arc::new(FakeMergeRequests::empty());
        let branch_reader = Arc::new(FakeBranchReader::new(vec![BranchInfo {
            name: "feature".to_string(),
            tip_sha: "sha1".to_string(),
            is_default: false,
        }]));
        let repositories = Arc::new(FakeRepositories::new(vec![Repository {
            id: Uuid::new_v4(),
            owner_id: Uuid::new_v4(),
            name: "hello".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "hello.git".to_string(),
            visibility: ferrisgit_domain::repository::RepositoryVisibility::Private,
            created_at: Utc::now(),
        }]));
        let users = Arc::new(FakeUsers::new(vec![]));
        let notifications = Arc::new(FakeNotifications::empty());
        let use_case = SubmitMergeRequestReviewUseCase::new(
            mr_store.clone(),
            mr_store.clone(),
            branch_reader,
            repositories,
            users,
            notifications,
            Arc::new(FakeWebhooks::default()),
        );

        let review = use_case
            .execute(
                mr_id,
                "disk/path",
                "feature",
                user_id,
                ReviewDecision::Approved,
            )
            .await
            .unwrap();

        assert_eq!(review.source_sha, "sha1");
        assert_eq!(review.decision, ReviewDecision::Approved);
        let reviews = mr_store.reviews_snapshot();
        assert_eq!(reviews.len(), 1);
        assert_eq!(reviews[0].merge_request_id, mr_id);
        assert_eq!(reviews[0].user_id, user_id);
        assert_eq!(reviews[0].decision, ReviewDecision::Approved);
        assert_eq!(reviews[0].source_sha, "sha1");
    }

    #[tokio::test]
    async fn reviewing_when_the_source_branch_no_longer_exists_is_a_validation_error() {
        let mr_store = Arc::new(FakeMergeRequests::empty());
        let branch_reader = Arc::new(FakeBranchReader::new(vec![]));
        let repositories = Arc::new(FakeRepositories::new(vec![Repository {
            id: Uuid::new_v4(),
            owner_id: Uuid::new_v4(),
            name: "hello".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "hello.git".to_string(),
            visibility: ferrisgit_domain::repository::RepositoryVisibility::Private,
            created_at: Utc::now(),
        }]));
        let users = Arc::new(FakeUsers::new(vec![]));
        let notifications = Arc::new(FakeNotifications::empty());
        let use_case = SubmitMergeRequestReviewUseCase::new(
            mr_store.clone(),
            mr_store,
            branch_reader,
            repositories,
            users,
            notifications,
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(
                Uuid::new_v4(),
                "disk/path",
                "feature",
                Uuid::new_v4(),
                ReviewDecision::Approved,
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn approving_someone_elses_merge_request_notifies_its_author() {
        let mr_id = Uuid::new_v4();
        let author_id = Uuid::new_v4();
        let reviewer_id = Uuid::new_v4();
        let repository_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let mr = MergeRequest {
            id: mr_id,
            repository_id,
            author_id: Some(author_id),
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
        let mr_store = Arc::new(FakeMergeRequests::new(vec![mr]));
        let branch_reader = Arc::new(FakeBranchReader::new(vec![BranchInfo {
            name: "feature".to_string(),
            tip_sha: "sha1".to_string(),
            is_default: false,
        }]));
        let repositories = Arc::new(FakeRepositories::new(vec![Repository {
            id: repository_id,
            owner_id,
            name: "hello".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "hello.git".to_string(),
            visibility: ferrisgit_domain::repository::RepositoryVisibility::Private,
            created_at: Utc::now(),
        }]));
        let users = Arc::new(FakeUsers::new(vec![
            User {
                id: owner_id,
                username: "owner".to_string(),
                email: "o@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
                created_at: Utc::now(),
            },
            User {
                id: reviewer_id,
                username: "reviewer".to_string(),
                email: "r@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
                created_at: Utc::now(),
            },
        ]));
        let notifications = Arc::new(FakeNotifications::empty());
        let use_case = SubmitMergeRequestReviewUseCase::new(
            mr_store.clone(),
            mr_store,
            branch_reader,
            repositories,
            users,
            notifications.clone(),
            Arc::new(FakeWebhooks::default()),
        );

        use_case
            .execute(
                mr_id,
                "disk/path",
                "feature",
                reviewer_id,
                ReviewDecision::Approved,
            )
            .await
            .unwrap();

        let created = notifications.snapshot();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].recipient_id, author_id);
        assert_eq!(created[0].kind, NotificationKind::MergeRequestApproved);
    }

    #[tokio::test]
    async fn self_approving_your_own_merge_request_is_rejected() {
        let mr_id = Uuid::new_v4();
        let author_id = Uuid::new_v4();
        let repository_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let mr = MergeRequest {
            id: mr_id,
            repository_id,
            author_id: Some(author_id),
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
        let mr_store = Arc::new(FakeMergeRequests::new(vec![mr]));
        let branch_reader = Arc::new(FakeBranchReader::new(vec![BranchInfo {
            name: "feature".to_string(),
            tip_sha: "sha1".to_string(),
            is_default: false,
        }]));
        let repositories = Arc::new(FakeRepositories::new(vec![Repository {
            id: repository_id,
            owner_id,
            name: "hello".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "hello.git".to_string(),
            visibility: ferrisgit_domain::repository::RepositoryVisibility::Private,
            created_at: Utc::now(),
        }]));
        let users = Arc::new(FakeUsers::new(vec![
            User {
                id: owner_id,
                username: "owner".to_string(),
                email: "o@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
                created_at: Utc::now(),
            },
            User {
                id: author_id,
                username: "author".to_string(),
                email: "a@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
                created_at: Utc::now(),
            },
        ]));
        let notifications = Arc::new(FakeNotifications::empty());
        let webhooks = Arc::new(FakeWebhooks::default());
        let use_case = SubmitMergeRequestReviewUseCase::new(
            mr_store.clone(),
            mr_store,
            branch_reader,
            repositories,
            users,
            notifications.clone(),
            webhooks.clone(),
        );

        let result = use_case
            .execute(
                mr_id,
                "disk/path",
                "feature",
                author_id,
                ReviewDecision::Approved,
            )
            .await;

        assert!(
            matches!(result, Err(DomainError::Validation(_))),
            "a merge request's author must not be able to approve their own merge request"
        );
        assert!(notifications.snapshot().is_empty());
        assert!(
            webhooks.dispatched().is_empty(),
            "a rejected self-approval must not dispatch a webhook"
        );
    }

    #[tokio::test]
    async fn requesting_changes_on_your_own_merge_request_is_still_allowed() {
        let mr_id = Uuid::new_v4();
        let author_id = Uuid::new_v4();
        let repository_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let mr = MergeRequest {
            id: mr_id,
            repository_id,
            author_id: Some(author_id),
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
        let mr_store = Arc::new(FakeMergeRequests::new(vec![mr]));
        let branch_reader = Arc::new(FakeBranchReader::new(vec![BranchInfo {
            name: "feature".to_string(),
            tip_sha: "sha1".to_string(),
            is_default: false,
        }]));
        let repositories = Arc::new(FakeRepositories::new(vec![Repository {
            id: repository_id,
            owner_id,
            name: "hello".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "hello.git".to_string(),
            visibility: ferrisgit_domain::repository::RepositoryVisibility::Private,
            created_at: Utc::now(),
        }]));
        let users = Arc::new(FakeUsers::new(vec![
            User {
                id: owner_id,
                username: "owner".to_string(),
                email: "o@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
                created_at: Utc::now(),
            },
            User {
                id: author_id,
                username: "author".to_string(),
                email: "a@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
                created_at: Utc::now(),
            },
        ]));
        let notifications = Arc::new(FakeNotifications::empty());
        let webhooks = Arc::new(FakeWebhooks::default());
        let use_case = SubmitMergeRequestReviewUseCase::new(
            mr_store.clone(),
            mr_store,
            branch_reader,
            repositories,
            users,
            notifications.clone(),
            webhooks.clone(),
        );

        let review = use_case
            .execute(
                mr_id,
                "disk/path",
                "feature",
                author_id,
                ReviewDecision::ChangesRequested,
            )
            .await
            .unwrap();

        assert_eq!(review.decision, ReviewDecision::ChangesRequested);
        assert!(
            webhooks.dispatched().len() == 1,
            "self-review is only blocked for approvals, not for requesting changes on your own work"
        );
    }

    #[tokio::test]
    async fn resubmitting_the_same_decision_on_the_same_commit_does_not_re_notify() {
        let mr_id = Uuid::new_v4();
        let author_id = Uuid::new_v4();
        let reviewer_id = Uuid::new_v4();
        let repository_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let mr = MergeRequest {
            id: mr_id,
            repository_id,
            author_id: Some(author_id),
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
        let mr_store = Arc::new(FakeMergeRequests::new(vec![mr]));
        let branch_reader = Arc::new(FakeBranchReader::new(vec![BranchInfo {
            name: "feature".to_string(),
            tip_sha: "sha1".to_string(),
            is_default: false,
        }]));
        let repositories = Arc::new(FakeRepositories::new(vec![Repository {
            id: repository_id,
            owner_id,
            name: "hello".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "hello.git".to_string(),
            visibility: ferrisgit_domain::repository::RepositoryVisibility::Private,
            created_at: Utc::now(),
        }]));
        let users = Arc::new(FakeUsers::new(vec![
            User {
                id: owner_id,
                username: "owner".to_string(),
                email: "o@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
                created_at: Utc::now(),
            },
            User {
                id: reviewer_id,
                username: "reviewer".to_string(),
                email: "r@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
                created_at: Utc::now(),
            },
        ]));
        let notifications = Arc::new(FakeNotifications::empty());
        let webhooks = Arc::new(FakeWebhooks::default());
        let use_case = SubmitMergeRequestReviewUseCase::new(
            mr_store.clone(),
            mr_store,
            branch_reader,
            repositories,
            users,
            notifications.clone(),
            webhooks.clone(),
        );

        use_case
            .execute(
                mr_id,
                "disk/path",
                "feature",
                reviewer_id,
                ReviewDecision::Approved,
            )
            .await
            .unwrap();
        use_case
            .execute(
                mr_id,
                "disk/path",
                "feature",
                reviewer_id,
                ReviewDecision::Approved,
            )
            .await
            .unwrap();

        let created = notifications.snapshot();
        assert_eq!(
            created.len(),
            1,
            "resubmitting the identical decision on the same commit must not send a second notification"
        );
        let dispatched = webhooks.dispatched();
        assert_eq!(
            dispatched.len(),
            1,
            "resubmitting the identical decision on the same commit must not dispatch a second webhook either"
        );
    }
}
