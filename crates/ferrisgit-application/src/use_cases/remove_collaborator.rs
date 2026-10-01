use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::notification::{NewNotification, NotificationKind, NotificationStorePort};
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::repository_collaborator::{
    CollaboratorRole, RepositoryCollaboratorStorePort,
};
use ferrisgit_domain::user::UserRepositoryPort;
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use ferrisgit_domain::webhook_event::WebhookEvent;
use uuid::Uuid;

pub struct RemoveCollaboratorUseCase {
    collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
    repositories: Arc<dyn RepositoryStorePort>,
    users: Arc<dyn UserRepositoryPort>,
    notifications: Arc<dyn NotificationStorePort>,
    webhooks: Arc<dyn WebhookDispatcherPort>,
}

impl RemoveCollaboratorUseCase {
    pub fn new(
        collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
        repositories: Arc<dyn RepositoryStorePort>,
        users: Arc<dyn UserRepositoryPort>,
        notifications: Arc<dyn NotificationStorePort>,
        webhooks: Arc<dyn WebhookDispatcherPort>,
    ) -> Self {
        Self {
            collaborators,
            repositories,
            users,
            notifications,
            webhooks,
        }
    }

    pub async fn execute(
        &self,
        repository_id: Uuid,
        caller_id: Uuid,
        owner_id: Uuid,
        target_user_id: Uuid,
    ) -> Result<(), DomainError> {
        let caller_role = self
            .collaborators
            .get_role(repository_id, caller_id)
            .await?;
        let authorized =
            caller_id == owner_id || caller_role.is_some_and(|r| r >= CollaboratorRole::Maintainer);
        if !authorized {
            return Err(DomainError::NotFound("repository".to_string()));
        }
        let target_was_collaborator = self
            .collaborators
            .get_role(repository_id, target_user_id)
            .await
            .ok()
            .flatten()
            .is_some();
        self.collaborators
            .remove(repository_id, target_user_id)
            .await?;

        if target_was_collaborator
            && let Some(repo) = self
                .repositories
                .find_by_id(repository_id)
                .await
                .ok()
                .flatten()
            && let Some(owner) = self.users.find_by_id(repo.owner_id).await.ok().flatten()
            && let Some(actor) = self.users.find_by_id(caller_id).await.ok().flatten()
            && let Some(target) = self.users.find_by_id(target_user_id).await.ok().flatten()
        {
            self.webhooks
                .dispatch(
                    repository_id,
                    WebhookEvent::CollaboratorRemoved {
                        repository_owner: owner.username.clone(),
                        repository_name: repo.name.clone(),
                        actor_username: actor.username.clone(),
                        target_username: target.username,
                    },
                )
                .await
                .ok();

            if caller_id != target_user_id {
                self.notifications
                    .create(NewNotification {
                        recipient_id: target_user_id,
                        kind: NotificationKind::CollaboratorRemoved,
                        repository_owner: owner.username,
                        repository_name: repo.name,
                        actor_username: Some(actor.username),
                        merge_request_id: None,
                        merge_request_title: None,
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
        FakeCollaborators, FakeNotifications, FakeRepositories, FakeUsers, FakeWebhooks,
    };
    use ferrisgit_domain::repository::{Repository, RepositoryVisibility};
    use ferrisgit_domain::user::User;
    use ferrisgit_domain::webhook_event::WebhookEvent;

    fn user(username: &str) -> User {
        User {
            id: Uuid::new_v4(),
            username: username.to_string(),
            email: format!("{username}@example.com"),
            password_hash: "h".to_string(),
            is_admin: false,
            created_at: chrono::Utc::now(),
        }
    }

    fn repository(owner_id: Uuid) -> Repository {
        Repository {
            id: Uuid::new_v4(),
            owner_id,
            name: "hello".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "hello.git".to_string(),
            visibility: RepositoryVisibility::Private,
            created_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn the_owner_can_remove_a_collaborator() {
        let owner = user("owner");
        let target = user("alice");
        let repo_id = Uuid::new_v4();
        let collaborators = Arc::new(FakeCollaborators::new(vec![(
            repo_id,
            target.id,
            CollaboratorRole::Contributor,
        )]));
        let use_case = RemoveCollaboratorUseCase::new(
            collaborators.clone(),
            Arc::new(FakeRepositories::new(vec![repository(owner.id)])),
            Arc::new(FakeUsers::new(vec![owner.clone(), target.clone()])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        use_case
            .execute(repo_id, owner.id, owner.id, target.id)
            .await
            .unwrap();

        assert!(collaborators.snapshot().is_empty());
    }

    #[tokio::test]
    async fn a_non_owner_caller_is_rejected_as_not_found() {
        let owner_id = Uuid::new_v4();
        let use_case = RemoveCollaboratorUseCase::new(
            Arc::new(FakeCollaborators::empty()),
            Arc::new(FakeRepositories::new(vec![repository(owner_id)])),
            Arc::new(FakeUsers::new(vec![])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(Uuid::new_v4(), Uuid::new_v4(), owner_id, Uuid::new_v4())
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn a_maintainer_collaborator_can_remove_another_collaborator() {
        let owner = user("owner");
        let maintainer = user("maintainer");
        let target = user("alice");
        let repo_id = Uuid::new_v4();
        let collaborators = Arc::new(FakeCollaborators::new(vec![
            (repo_id, maintainer.id, CollaboratorRole::Maintainer),
            (repo_id, target.id, CollaboratorRole::Reader),
        ]));
        let use_case = RemoveCollaboratorUseCase::new(
            collaborators.clone(),
            Arc::new(FakeRepositories::new(vec![repository(owner.id)])),
            Arc::new(FakeUsers::new(vec![
                owner.clone(),
                maintainer.clone(),
                target.clone(),
            ])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        use_case
            .execute(repo_id, maintainer.id, owner.id, target.id)
            .await
            .unwrap();

        assert_eq!(
            collaborators.snapshot().as_slice(),
            &[(repo_id, maintainer.id, CollaboratorRole::Maintainer)]
        );
    }

    #[tokio::test]
    async fn a_maintainer_can_remove_themselves() {
        let owner = user("owner");
        let maintainer = user("maintainer");
        let repo_id = Uuid::new_v4();
        let collaborators = Arc::new(FakeCollaborators::new(vec![(
            repo_id,
            maintainer.id,
            CollaboratorRole::Maintainer,
        )]));
        let use_case = RemoveCollaboratorUseCase::new(
            collaborators.clone(),
            Arc::new(FakeRepositories::new(vec![repository(owner.id)])),
            Arc::new(FakeUsers::new(vec![owner.clone(), maintainer.clone()])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        use_case
            .execute(repo_id, maintainer.id, owner.id, maintainer.id)
            .await
            .unwrap();

        assert!(collaborators.snapshot().is_empty());
    }

    #[tokio::test]
    async fn a_contributor_collaborator_cannot_remove_another_collaborator() {
        let owner = user("owner");
        let contributor = user("contributor");
        let target = user("alice");
        let repo_id = Uuid::new_v4();
        let collaborators = Arc::new(FakeCollaborators::new(vec![
            (repo_id, contributor.id, CollaboratorRole::Contributor),
            (repo_id, target.id, CollaboratorRole::Reader),
        ]));
        let use_case = RemoveCollaboratorUseCase::new(
            collaborators.clone(),
            Arc::new(FakeRepositories::new(vec![repository(owner.id)])),
            Arc::new(FakeUsers::new(vec![
                owner.clone(),
                contributor.clone(),
                target.clone(),
            ])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(repo_id, contributor.id, owner.id, target.id)
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn removing_another_collaborator_notifies_them() {
        let owner = user("owner");
        let target = user("alice");
        let repo_id = Uuid::new_v4();
        let collaborators = Arc::new(FakeCollaborators::new(vec![(
            repo_id,
            target.id,
            CollaboratorRole::Contributor,
        )]));
        let notifications = Arc::new(FakeNotifications::empty());
        let use_case = RemoveCollaboratorUseCase::new(
            collaborators.clone(),
            Arc::new(FakeRepositories::new(vec![Repository {
                id: repo_id,
                ..repository(owner.id)
            }])),
            Arc::new(FakeUsers::new(vec![owner.clone(), target.clone()])),
            notifications.clone(),
            Arc::new(FakeWebhooks::default()),
        );

        use_case
            .execute(repo_id, owner.id, owner.id, target.id)
            .await
            .unwrap();

        let created = notifications.snapshot();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].recipient_id, target.id);
        assert_eq!(created[0].kind, NotificationKind::CollaboratorRemoved);
    }

    #[tokio::test]
    async fn removing_yourself_does_not_notify_yourself() {
        let owner = user("owner");
        let maintainer = user("maintainer");
        let repo_id = Uuid::new_v4();
        let collaborators = Arc::new(FakeCollaborators::new(vec![(
            repo_id,
            maintainer.id,
            CollaboratorRole::Maintainer,
        )]));
        let notifications = Arc::new(FakeNotifications::empty());
        let use_case = RemoveCollaboratorUseCase::new(
            collaborators.clone(),
            Arc::new(FakeRepositories::new(vec![repository(owner.id)])),
            Arc::new(FakeUsers::new(vec![owner.clone(), maintainer.clone()])),
            notifications.clone(),
            Arc::new(FakeWebhooks::default()),
        );

        use_case
            .execute(repo_id, maintainer.id, owner.id, maintainer.id)
            .await
            .unwrap();

        assert!(notifications.snapshot().is_empty());
    }

    #[tokio::test]
    async fn removing_a_target_who_was_never_a_collaborator_does_not_notify_them() {
        let owner = user("owner");
        let target = user("alice");
        let repo_id = Uuid::new_v4();
        // `target` is left out of the collaborators fixture on purpose. The store's `remove` is a no-op for a pair that
        // never existed, so `execute` must detect that beforehand and skip the notification.
        let collaborators = Arc::new(FakeCollaborators::empty());
        let notifications = Arc::new(FakeNotifications::empty());
        let use_case = RemoveCollaboratorUseCase::new(
            collaborators.clone(),
            Arc::new(FakeRepositories::new(vec![repository(owner.id)])),
            Arc::new(FakeUsers::new(vec![owner.clone(), target.clone()])),
            notifications.clone(),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(repo_id, owner.id, owner.id, target.id)
            .await;

        assert!(
            result.is_ok(),
            "removing a non-collaborator is still a harmless success"
        );
        assert!(
            notifications.snapshot().is_empty(),
            "no notification should be sent for a removal that never actually happened"
        );
    }

    #[tokio::test]
    async fn a_self_removal_dispatches_a_webhook_with_zero_notifications() {
        let owner = user("owner");
        let maintainer = user("maintainer");
        let repo_id = Uuid::new_v4();
        let collaborators = Arc::new(FakeCollaborators::new(vec![(
            repo_id,
            maintainer.id,
            CollaboratorRole::Maintainer,
        )]));
        let notifications = Arc::new(FakeNotifications::empty());
        let webhooks = Arc::new(FakeWebhooks::default());
        let use_case = RemoveCollaboratorUseCase::new(
            collaborators.clone(),
            Arc::new(FakeRepositories::new(vec![Repository {
                id: repo_id,
                ..repository(owner.id)
            }])),
            Arc::new(FakeUsers::new(vec![owner.clone(), maintainer.clone()])),
            notifications.clone(),
            webhooks.clone(),
        );

        use_case
            .execute(repo_id, maintainer.id, owner.id, maintainer.id)
            .await
            .unwrap();

        assert!(
            notifications.snapshot().is_empty(),
            "self-exclusion must still gate the notification"
        );
        let dispatched = webhooks.dispatched();
        assert_eq!(
            dispatched.len(),
            1,
            "self-exclusion must not gate the webhook"
        );
        match &dispatched[0].1 {
            WebhookEvent::CollaboratorRemoved {
                target_username,
                actor_username,
                ..
            } => {
                assert_eq!(target_username, "maintainer");
                assert_eq!(actor_username, "maintainer");
            }
            other => panic!("expected CollaboratorRemoved, got {other:?}"),
        }
    }
}
