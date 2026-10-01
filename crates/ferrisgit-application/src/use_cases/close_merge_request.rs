use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::merge_request::{MergeRequestStatus, MergeRequestStorePort};
use ferrisgit_domain::notification::{NewNotification, NotificationKind, NotificationStorePort};
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::user::UserRepositoryPort;
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use ferrisgit_domain::webhook_event::WebhookEvent;
use uuid::Uuid;

pub struct CloseMergeRequestUseCase {
    merge_requests: Arc<dyn MergeRequestStorePort>,
    repositories: Arc<dyn RepositoryStorePort>,
    users: Arc<dyn UserRepositoryPort>,
    notifications: Arc<dyn NotificationStorePort>,
    webhooks: Arc<dyn WebhookDispatcherPort>,
}

impl CloseMergeRequestUseCase {
    pub fn new(
        merge_requests: Arc<dyn MergeRequestStorePort>,
        repositories: Arc<dyn RepositoryStorePort>,
        users: Arc<dyn UserRepositoryPort>,
        notifications: Arc<dyn NotificationStorePort>,
        webhooks: Arc<dyn WebhookDispatcherPort>,
    ) -> Self {
        Self {
            merge_requests,
            repositories,
            users,
            notifications,
            webhooks,
        }
    }

    pub async fn execute(&self, merge_request_id: Uuid, user_id: Uuid) -> Result<(), DomainError> {
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
        self.merge_requests.mark_closed(merge_request_id).await?;

        if let Some(repo) = self
            .repositories
            .find_by_id(mr.repository_id)
            .await
            .ok()
            .flatten()
            && let Some(owner) = self.users.find_by_id(repo.owner_id).await.ok().flatten()
            && let Some(actor) = self.users.find_by_id(user_id).await.ok().flatten()
        {
            self.webhooks
                .dispatch(
                    mr.repository_id,
                    WebhookEvent::MergeRequestClosed {
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
                        kind: NotificationKind::MergeRequestClosed,
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

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeMergeRequests, FakeNotifications, FakeRepositories, FakeUsers, FakeWebhooks,
    };
    use chrono::Utc;
    use ferrisgit_domain::merge_request::MergeRequest;
    use ferrisgit_domain::repository::{Repository, RepositoryVisibility};
    use ferrisgit_domain::user::User;
    use ferrisgit_domain::webhook_event::WebhookEvent;

    fn merge_request(status: MergeRequestStatus) -> MergeRequest {
        MergeRequest {
            id: Uuid::new_v4(),
            repository_id: Uuid::new_v4(),
            author_id: Some(Uuid::new_v4()),
            source_branch: "feature".to_string(),
            target_branch: "main".to_string(),
            title: "t".to_string(),
            description: String::new(),
            status,
            merge_commit_sha: None,
            milestone_id: None,
            created_at: Utc::now(),
            closed_at: None,
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

    #[tokio::test]
    async fn closes_an_open_merge_request() {
        let mr = merge_request(MergeRequestStatus::Open);
        let store = Arc::new(FakeMergeRequests::new(vec![mr.clone()]));
        let repo = repository(mr.repository_id);
        let use_case = CloseMergeRequestUseCase::new(
            store.clone(),
            Arc::new(FakeRepositories::new(vec![repo])),
            Arc::new(FakeUsers::new(vec![user(Uuid::new_v4())])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        use_case
            .execute(mr.id, mr.author_id.unwrap())
            .await
            .unwrap();

        let updated = store.get(mr.id).unwrap();
        assert_eq!(updated.status, MergeRequestStatus::Closed);
        assert!(updated.closed_at.is_some());
    }

    #[tokio::test]
    async fn closing_an_already_merged_request_is_a_validation_error() {
        let mr = merge_request(MergeRequestStatus::Merged);
        let store = Arc::new(FakeMergeRequests::new(vec![mr.clone()]));
        let repo = repository(mr.repository_id);
        let use_case = CloseMergeRequestUseCase::new(
            store,
            Arc::new(FakeRepositories::new(vec![repo])),
            Arc::new(FakeUsers::new(vec![user(Uuid::new_v4())])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case.execute(mr.id, mr.author_id.unwrap()).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn closing_a_non_existent_request_is_a_not_found_error() {
        let use_case = CloseMergeRequestUseCase::new(
            Arc::new(FakeMergeRequests::empty()),
            Arc::new(FakeRepositories::new(vec![repository(Uuid::new_v4())])),
            Arc::new(FakeUsers::new(vec![user(Uuid::new_v4())])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case.execute(Uuid::new_v4(), Uuid::new_v4()).await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn closing_someone_elses_open_merge_request_notifies_its_author() {
        let mr = merge_request(MergeRequestStatus::Open);
        let closer_id = Uuid::new_v4();
        assert_ne!(Some(closer_id), mr.author_id);
        let store = Arc::new(FakeMergeRequests::new(vec![mr.clone()]));
        let repo = repository(mr.repository_id);
        let owner_id = repo.owner_id;
        let notifications = Arc::new(FakeNotifications::empty());
        // The webhook/notification block needs both the owner and the closer to resolve via `find_by_id`.
        let use_case = CloseMergeRequestUseCase::new(
            store.clone(),
            Arc::new(FakeRepositories::new(vec![repo])),
            Arc::new(FakeUsers::new(vec![user(owner_id), user(closer_id)])),
            notifications.clone(),
            Arc::new(FakeWebhooks::default()),
        );

        use_case.execute(mr.id, closer_id).await.unwrap();

        let updated = store.get(mr.id).unwrap();
        assert_eq!(updated.status, MergeRequestStatus::Closed);
        let created = notifications.snapshot();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].recipient_id, mr.author_id.unwrap());
        assert_eq!(created[0].kind, NotificationKind::MergeRequestClosed);
    }

    #[tokio::test]
    async fn closing_your_own_merge_request_still_dispatches_a_webhook_with_no_notification() {
        let repo_id = Uuid::new_v4();
        let mr = merge_request(MergeRequestStatus::Open);
        let mr = MergeRequest {
            repository_id: repo_id,
            ..mr
        };
        let author_id = mr.author_id.unwrap();
        let repo = Repository {
            id: repo_id,
            ..repository(repo_id)
        };
        let owner_id = repo.owner_id;
        let notifications = Arc::new(FakeNotifications::empty());
        let webhooks = Arc::new(FakeWebhooks::default());
        let use_case = CloseMergeRequestUseCase::new(
            Arc::new(FakeMergeRequests::new(vec![mr.clone()])),
            Arc::new(FakeRepositories::new(vec![repo])),
            Arc::new(FakeUsers::new(vec![user(owner_id), user(author_id)])),
            notifications.clone(),
            webhooks.clone(),
        );

        use_case.execute(mr.id, author_id).await.unwrap();

        assert!(
            notifications.snapshot().is_empty(),
            "closing your own MR must not notify yourself"
        );
        let dispatched = webhooks.dispatched();
        assert_eq!(
            dispatched.len(),
            1,
            "the webhook must still fire even with no one to notify"
        );
        assert_eq!(dispatched[0].0, repo_id);
        assert!(matches!(
            &dispatched[0].1,
            WebhookEvent::MergeRequestClosed { .. }
        ));
    }

    /// The author's account was deleted (`author_id` set to NULL): the merge request still closes and its webhook
    /// still fires, there is just nobody left to notify.
    #[tokio::test]
    async fn closing_a_merge_request_whose_author_was_deleted_notifies_nobody() {
        let mr = MergeRequest {
            author_id: None,
            ..merge_request(MergeRequestStatus::Open)
        };
        let closer_id = Uuid::new_v4();
        let store = Arc::new(FakeMergeRequests::new(vec![mr.clone()]));
        let repo = repository(mr.repository_id);
        let owner_id = repo.owner_id;
        let notifications = Arc::new(FakeNotifications::empty());
        let webhooks = Arc::new(FakeWebhooks::default());
        let use_case = CloseMergeRequestUseCase::new(
            store.clone(),
            Arc::new(FakeRepositories::new(vec![repo])),
            Arc::new(FakeUsers::new(vec![user(owner_id), user(closer_id)])),
            notifications.clone(),
            webhooks.clone(),
        );

        use_case.execute(mr.id, closer_id).await.unwrap();

        assert_eq!(store.get(mr.id).unwrap().status, MergeRequestStatus::Closed);
        assert!(notifications.snapshot().is_empty());
        assert_eq!(webhooks.dispatched().len(), 1);
    }
}
