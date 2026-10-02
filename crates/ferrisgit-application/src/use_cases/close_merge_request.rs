use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::merge_request::{MergeRequestStatus, MergeRequestStorePort};
use ferrisgit_domain::notification::{NewNotification, NotificationKind, NotificationStorePort};
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::user::UserRepositoryPort;
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use ferrisgit_domain::webhook_event::WebhookEvent;
use uuid::Uuid;

use super::event_context::EventContext;

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

        if let Some(ctx) = EventContext::load(
            self.repositories.as_ref(),
            self.users.as_ref(),
            mr.repository_id,
            user_id,
        )
        .await
        {
            self.webhooks
                .dispatch(
                    mr.repository_id,
                    WebhookEvent::MergeRequestClosed {
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
                        ..ctx.notification(NotificationKind::MergeRequestClosed, mr_author_id)
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
    use crate::use_cases::fixtures::{merge_request, repository, user};
    use ferrisgit_domain::merge_request::MergeRequest;
    use ferrisgit_domain::user::User;
    use ferrisgit_domain::webhook_event::WebhookEvent;

    struct Fixture {
        use_case: CloseMergeRequestUseCase,
        mr: MergeRequest,
        author: User,
        closer: User,
        store: Arc<FakeMergeRequests>,
        notifications: Arc<FakeNotifications>,
        webhooks: Arc<FakeWebhooks>,
    }

    /// One merge request by `author`, in a repository owned by somebody else; `closer` is a third user. `adjust`
    /// changes the merge request before it is stored.
    fn fixture(adjust: impl FnOnce(MergeRequest) -> MergeRequest) -> Fixture {
        let owner = user("owner");
        let author = user("author");
        let closer = user("closer");
        let repo = repository(owner.id);
        let mr = adjust(merge_request(repo.id, author.id));
        let store = Arc::new(FakeMergeRequests::new(vec![mr.clone()]));
        let notifications = Arc::new(FakeNotifications::empty());
        let webhooks = Arc::new(FakeWebhooks::default());
        let use_case = CloseMergeRequestUseCase::new(
            store.clone(),
            Arc::new(FakeRepositories::new(vec![repo])),
            Arc::new(FakeUsers::new(vec![owner, author.clone(), closer.clone()])),
            notifications.clone(),
            webhooks.clone(),
        );
        Fixture {
            use_case,
            mr,
            author,
            closer,
            store,
            notifications,
            webhooks,
        }
    }

    #[tokio::test]
    async fn closes_an_open_merge_request() {
        let f = fixture(|mr| mr);

        f.use_case.execute(f.mr.id, f.author.id).await.unwrap();

        let updated = f.store.get(f.mr.id).unwrap();
        assert_eq!(updated.status, MergeRequestStatus::Closed);
        assert!(updated.closed_at.is_some());
    }

    #[tokio::test]
    async fn closing_an_already_merged_request_is_a_validation_error() {
        let f = fixture(|mr| MergeRequest {
            status: MergeRequestStatus::Merged,
            ..mr
        });

        let result = f.use_case.execute(f.mr.id, f.author.id).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn closing_a_non_existent_request_is_a_not_found_error() {
        let f = fixture(|mr| mr);

        let result = f.use_case.execute(Uuid::new_v4(), Uuid::new_v4()).await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn closing_someone_elses_open_merge_request_notifies_its_author() {
        let f = fixture(|mr| mr);

        f.use_case.execute(f.mr.id, f.closer.id).await.unwrap();

        let updated = f.store.get(f.mr.id).unwrap();
        assert_eq!(updated.status, MergeRequestStatus::Closed);
        let created = f.notifications.snapshot();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].recipient_id, f.author.id);
        assert_eq!(created[0].kind, NotificationKind::MergeRequestClosed);
    }

    #[tokio::test]
    async fn closing_your_own_merge_request_still_dispatches_a_webhook_with_no_notification() {
        let f = fixture(|mr| mr);

        f.use_case.execute(f.mr.id, f.author.id).await.unwrap();

        assert!(
            f.notifications.snapshot().is_empty(),
            "closing your own MR must not notify yourself"
        );
        let dispatched = f.webhooks.dispatched();
        assert_eq!(
            dispatched.len(),
            1,
            "the webhook must still fire even with no one to notify"
        );
        assert_eq!(dispatched[0].0, f.mr.repository_id);
        assert!(matches!(
            &dispatched[0].1,
            WebhookEvent::MergeRequestClosed { .. }
        ));
    }

    /// The author's account was deleted (`author_id` set to NULL): the merge request still closes and its webhook
    /// still fires, there is just nobody left to notify.
    #[tokio::test]
    async fn closing_a_merge_request_whose_author_was_deleted_notifies_nobody() {
        let f = fixture(|mr| MergeRequest {
            author_id: None,
            ..mr
        });

        f.use_case.execute(f.mr.id, f.closer.id).await.unwrap();

        assert_eq!(
            f.store.get(f.mr.id).unwrap().status,
            MergeRequestStatus::Closed
        );
        assert!(f.notifications.snapshot().is_empty());
        assert_eq!(f.webhooks.dispatched().len(), 1);
    }
}
