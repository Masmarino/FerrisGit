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

pub struct SetCollaboratorRoleUseCase {
    collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
    users: Arc<dyn UserRepositoryPort>,
    repositories: Arc<dyn RepositoryStorePort>,
    notifications: Arc<dyn NotificationStorePort>,
    webhooks: Arc<dyn WebhookDispatcherPort>,
}

impl SetCollaboratorRoleUseCase {
    pub fn new(
        collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
        users: Arc<dyn UserRepositoryPort>,
        repositories: Arc<dyn RepositoryStorePort>,
        notifications: Arc<dyn NotificationStorePort>,
        webhooks: Arc<dyn WebhookDispatcherPort>,
    ) -> Self {
        Self {
            collaborators,
            users,
            repositories,
            notifications,
            webhooks,
        }
    }

    pub async fn execute(
        &self,
        repository_id: Uuid,
        caller_id: Uuid,
        owner_id: Uuid,
        username: &str,
        role: CollaboratorRole,
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
        let target = self
            .users
            .find_by_username(username)
            .await?
            .ok_or_else(|| DomainError::Validation("no such user".to_string()))?;
        if target.id == owner_id {
            return Err(DomainError::Validation(
                "the owner is already a collaborator".to_string(),
            ));
        }
        let previous_role = self
            .collaborators
            .get_role(repository_id, target.id)
            .await
            .ok()
            .flatten();
        self.collaborators
            .set_role(repository_id, target.id, role)
            .await?;

        if previous_role != Some(role)
            && let Some(repo) = self
                .repositories
                .find_by_id(repository_id)
                .await
                .ok()
                .flatten()
            && let Some(owner) = self.users.find_by_id(repo.owner_id).await.ok().flatten()
            && let Some(actor) = self.users.find_by_id(caller_id).await.ok().flatten()
        {
            self.webhooks
                .dispatch(
                    repository_id,
                    WebhookEvent::CollaboratorRoleChanged {
                        repository_owner: owner.username.clone(),
                        repository_name: repo.name.clone(),
                        actor_username: actor.username.clone(),
                        target_username: target.username.clone(),
                        role: role.as_str().to_string(),
                    },
                )
                .await
                .ok();

            if caller_id != target.id {
                self.notifications
                    .create(NewNotification {
                        recipient_id: target.id,
                        kind: NotificationKind::CollaboratorRoleChanged,
                        repository_owner: owner.username,
                        repository_name: repo.name,
                        actor_username: Some(actor.username),
                        merge_request_id: None,
                        merge_request_title: None,
                        pipeline_id: None,
                        commit_sha: None,
                        role: Some(role.as_str().to_string()),
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
    async fn the_owner_can_promote_a_collaborator_to_maintainer() {
        let owner = user("owner");
        let target = user("alice");
        let repo_id = Uuid::new_v4();
        let collaborators = Arc::new(FakeCollaborators::new(vec![(
            repo_id,
            target.id,
            CollaboratorRole::Reader,
        )]));
        let use_case = SetCollaboratorRoleUseCase::new(
            collaborators.clone(),
            Arc::new(FakeUsers::new(vec![owner.clone(), target.clone()])),
            Arc::new(FakeRepositories::new(vec![repository(owner.id)])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        use_case
            .execute(
                repo_id,
                owner.id,
                owner.id,
                "alice",
                CollaboratorRole::Maintainer,
            )
            .await
            .unwrap();

        assert_eq!(
            collaborators.snapshot().as_slice(),
            &[(repo_id, target.id, CollaboratorRole::Maintainer)]
        );
    }

    #[tokio::test]
    async fn a_maintainer_can_promote_another_collaborator_to_maintainer() {
        let owner = user("owner");
        let maintainer = user("maintainer");
        let target = user("alice");
        let repo_id = Uuid::new_v4();
        let collaborators = Arc::new(FakeCollaborators::new(vec![
            (repo_id, maintainer.id, CollaboratorRole::Maintainer),
            (repo_id, target.id, CollaboratorRole::Reader),
        ]));
        let use_case = SetCollaboratorRoleUseCase::new(
            collaborators.clone(),
            Arc::new(FakeUsers::new(vec![
                owner.clone(),
                maintainer.clone(),
                target.clone(),
            ])),
            Arc::new(FakeRepositories::new(vec![repository(owner.id)])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        use_case
            .execute(
                repo_id,
                maintainer.id,
                owner.id,
                "alice",
                CollaboratorRole::Maintainer,
            )
            .await
            .unwrap();

        assert!(collaborators.snapshot().contains(&(
            repo_id,
            target.id,
            CollaboratorRole::Maintainer
        )));
    }

    #[tokio::test]
    async fn a_contributor_cannot_set_another_collaborators_role() {
        let owner = user("owner");
        let contributor = user("contributor");
        let target = user("alice");
        let repo_id = Uuid::new_v4();
        let collaborators = Arc::new(FakeCollaborators::new(vec![
            (repo_id, contributor.id, CollaboratorRole::Contributor),
            (repo_id, target.id, CollaboratorRole::Reader),
        ]));
        let use_case = SetCollaboratorRoleUseCase::new(
            collaborators.clone(),
            Arc::new(FakeUsers::new(vec![
                owner.clone(),
                contributor.clone(),
                target.clone(),
            ])),
            Arc::new(FakeRepositories::new(vec![repository(owner.id)])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(
                repo_id,
                contributor.id,
                owner.id,
                "alice",
                CollaboratorRole::Maintainer,
            )
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn setting_the_role_of_a_username_that_is_not_currently_a_collaborator_errors() {
        let owner = user("owner");
        let target = user("alice");
        let repo_id = Uuid::new_v4();
        let use_case = SetCollaboratorRoleUseCase::new(
            Arc::new(FakeCollaborators::empty()),
            Arc::new(FakeUsers::new(vec![owner.clone(), target.clone()])),
            Arc::new(FakeRepositories::new(vec![repository(owner.id)])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(
                repo_id,
                owner.id,
                owner.id,
                "alice",
                CollaboratorRole::Maintainer,
            )
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn setting_the_role_of_an_unknown_username_is_a_validation_error() {
        let owner = user("owner");
        let repo_id = Uuid::new_v4();
        let use_case = SetCollaboratorRoleUseCase::new(
            Arc::new(FakeCollaborators::empty()),
            Arc::new(FakeUsers::new(vec![owner.clone()])),
            Arc::new(FakeRepositories::new(vec![repository(owner.id)])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(
                repo_id,
                owner.id,
                owner.id,
                "nobody",
                CollaboratorRole::Maintainer,
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn setting_the_owners_role_is_a_validation_error() {
        let owner = user("owner");
        let repo_id = Uuid::new_v4();
        let use_case = SetCollaboratorRoleUseCase::new(
            Arc::new(FakeCollaborators::empty()),
            Arc::new(FakeUsers::new(vec![owner.clone()])),
            Arc::new(FakeRepositories::new(vec![repository(owner.id)])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(
                repo_id,
                owner.id,
                owner.id,
                "owner",
                CollaboratorRole::Maintainer,
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn changing_another_collaborators_role_notifies_them() {
        let owner = user("owner");
        let target = user("alice");
        let repo_id = Uuid::new_v4();
        let collaborators = Arc::new(FakeCollaborators::new(vec![(
            repo_id,
            target.id,
            CollaboratorRole::Reader,
        )]));
        let notifications = Arc::new(FakeNotifications::empty());
        let use_case = SetCollaboratorRoleUseCase::new(
            collaborators.clone(),
            Arc::new(FakeUsers::new(vec![owner.clone(), target.clone()])),
            Arc::new(FakeRepositories::new(vec![Repository {
                id: repo_id,
                ..repository(owner.id)
            }])),
            notifications.clone(),
            Arc::new(FakeWebhooks::default()),
        );

        use_case
            .execute(
                repo_id,
                owner.id,
                owner.id,
                "alice",
                CollaboratorRole::Maintainer,
            )
            .await
            .unwrap();

        let created = notifications.snapshot();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].recipient_id, target.id);
        assert_eq!(created[0].kind, NotificationKind::CollaboratorRoleChanged);
        assert_eq!(
            created[0].role,
            Some(CollaboratorRole::Maintainer.as_str().to_string())
        );
    }

    #[tokio::test]
    async fn changing_your_own_role_does_not_notify_yourself() {
        let owner = user("owner");
        let maintainer = user("maintainer");
        let repo_id = Uuid::new_v4();
        let collaborators = Arc::new(FakeCollaborators::new(vec![(
            repo_id,
            maintainer.id,
            CollaboratorRole::Maintainer,
        )]));
        let notifications = Arc::new(FakeNotifications::empty());
        let use_case = SetCollaboratorRoleUseCase::new(
            collaborators.clone(),
            Arc::new(FakeUsers::new(vec![owner.clone(), maintainer.clone()])),
            Arc::new(FakeRepositories::new(vec![repository(owner.id)])),
            notifications.clone(),
            Arc::new(FakeWebhooks::default()),
        );

        use_case
            .execute(
                repo_id,
                maintainer.id,
                owner.id,
                "maintainer",
                CollaboratorRole::Maintainer,
            )
            .await
            .unwrap();

        assert!(notifications.snapshot().is_empty());
    }

    #[tokio::test]
    async fn changing_a_collaborators_role_dispatches_a_webhook_even_when_the_caller_changes_their_own_role()
     {
        let owner = user("owner");
        let maintainer = user("maintainer");
        let repo_id = Uuid::new_v4();
        let collaborators = Arc::new(FakeCollaborators::new(vec![(
            repo_id,
            maintainer.id,
            CollaboratorRole::Maintainer,
        )]));
        let webhooks = Arc::new(FakeWebhooks::default());
        let use_case = SetCollaboratorRoleUseCase::new(
            collaborators.clone(),
            Arc::new(FakeUsers::new(vec![owner.clone(), maintainer.clone()])),
            Arc::new(FakeRepositories::new(vec![Repository {
                id: repo_id,
                ..repository(owner.id)
            }])),
            Arc::new(FakeNotifications::empty()),
            webhooks.clone(),
        );

        use_case
            .execute(
                repo_id,
                maintainer.id,
                owner.id,
                "maintainer",
                CollaboratorRole::Contributor,
            )
            .await
            .unwrap();

        let dispatched = webhooks.dispatched();
        assert_eq!(
            dispatched.len(),
            1,
            "self-exclusion must not gate the webhook, only the notification"
        );
    }

    #[tokio::test]
    async fn resetting_a_collaborator_to_the_same_role_dispatches_no_webhook() {
        let owner = user("owner");
        let target = user("alice");
        let repo_id = Uuid::new_v4();
        let collaborators = Arc::new(FakeCollaborators::new(vec![(
            repo_id,
            target.id,
            CollaboratorRole::Maintainer,
        )]));
        let webhooks = Arc::new(FakeWebhooks::default());
        let use_case = SetCollaboratorRoleUseCase::new(
            collaborators.clone(),
            Arc::new(FakeUsers::new(vec![owner.clone(), target.clone()])),
            Arc::new(FakeRepositories::new(vec![repository(owner.id)])),
            Arc::new(FakeNotifications::empty()),
            webhooks.clone(),
        );

        use_case
            .execute(
                repo_id,
                owner.id,
                owner.id,
                "alice",
                CollaboratorRole::Maintainer,
            )
            .await
            .unwrap();

        assert!(
            webhooks.dispatched().is_empty(),
            "the idempotency check must gate the webhook too, unlike self-exclusion"
        );
    }
}
