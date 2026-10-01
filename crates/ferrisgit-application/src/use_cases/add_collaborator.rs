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

pub struct AddCollaboratorUseCase {
    collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
    users: Arc<dyn UserRepositoryPort>,
    repositories: Arc<dyn RepositoryStorePort>,
    notifications: Arc<dyn NotificationStorePort>,
    webhooks: Arc<dyn WebhookDispatcherPort>,
}

impl AddCollaboratorUseCase {
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
        self.collaborators
            .add(repository_id, target.id, role)
            .await?;

        if let Some(repo) = self
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
                    WebhookEvent::CollaboratorAdded {
                        repository_owner: owner.username.clone(),
                        repository_name: repo.name.clone(),
                        actor_username: actor.username.clone(),
                        target_username: target.username.clone(),
                        role: role.as_str().to_string(),
                    },
                )
                .await
                .ok();

            self.notifications
                .create(NewNotification {
                    recipient_id: target.id,
                    kind: NotificationKind::CollaboratorAdded,
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
    async fn the_owner_can_add_an_existing_user_as_a_collaborator() {
        let owner = user("owner");
        let target = user("alice");
        let collaborators = Arc::new(FakeCollaborators::empty());
        let use_case = AddCollaboratorUseCase::new(
            collaborators.clone(),
            Arc::new(FakeUsers::new(vec![owner.clone(), target.clone()])),
            Arc::new(FakeRepositories::new(vec![repository(owner.id)])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );
        let repo_id = Uuid::new_v4();

        use_case
            .execute(
                repo_id,
                owner.id,
                owner.id,
                "alice",
                CollaboratorRole::Contributor,
            )
            .await
            .unwrap();

        assert_eq!(
            collaborators.snapshot().as_slice(),
            &[(repo_id, target.id, CollaboratorRole::Contributor)]
        );
    }

    #[tokio::test]
    async fn a_non_owner_caller_is_rejected_as_not_found() {
        let owner = user("owner");
        let target = user("alice");
        let use_case = AddCollaboratorUseCase::new(
            Arc::new(FakeCollaborators::empty()),
            Arc::new(FakeUsers::new(vec![owner.clone(), target])),
            Arc::new(FakeRepositories::new(vec![repository(owner.id)])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(
                Uuid::new_v4(),
                Uuid::new_v4(),
                owner.id,
                "alice",
                CollaboratorRole::Contributor,
            )
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn adding_an_unknown_username_is_a_validation_error() {
        let owner = user("owner");
        let use_case = AddCollaboratorUseCase::new(
            Arc::new(FakeCollaborators::empty()),
            Arc::new(FakeUsers::new(vec![owner.clone()])),
            Arc::new(FakeRepositories::new(vec![repository(owner.id)])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(
                Uuid::new_v4(),
                owner.id,
                owner.id,
                "nobody",
                CollaboratorRole::Contributor,
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn adding_the_owner_as_their_own_collaborator_is_a_validation_error() {
        let owner = user("owner");
        let use_case = AddCollaboratorUseCase::new(
            Arc::new(FakeCollaborators::empty()),
            Arc::new(FakeUsers::new(vec![owner.clone()])),
            Arc::new(FakeRepositories::new(vec![repository(owner.id)])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(
                Uuid::new_v4(),
                owner.id,
                owner.id,
                "owner",
                CollaboratorRole::Contributor,
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn a_maintainer_collaborator_can_add_another_collaborator() {
        let owner = user("owner");
        let maintainer = user("maintainer");
        let target = user("alice");
        let repo_id = Uuid::new_v4();
        let collaborators = Arc::new(FakeCollaborators::new(vec![(
            repo_id,
            maintainer.id,
            CollaboratorRole::Maintainer,
        )]));
        let use_case = AddCollaboratorUseCase::new(
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
    async fn a_contributor_collaborator_cannot_add_another_collaborator() {
        let owner = user("owner");
        let contributor = user("contributor");
        let repo_id = Uuid::new_v4();
        let collaborators = Arc::new(FakeCollaborators::new(vec![(
            repo_id,
            contributor.id,
            CollaboratorRole::Contributor,
        )]));
        let use_case = AddCollaboratorUseCase::new(
            collaborators.clone(),
            Arc::new(FakeUsers::new(vec![owner.clone(), contributor.clone()])),
            Arc::new(FakeRepositories::new(vec![repository(owner.id)])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(
                repo_id,
                contributor.id,
                owner.id,
                "owner",
                CollaboratorRole::Reader,
            )
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn adding_a_collaborator_notifies_them() {
        let owner = user("owner");
        let target = user("alice");
        let repo_id = Uuid::new_v4();
        let collaborators = Arc::new(FakeCollaborators::empty());
        let notifications = Arc::new(FakeNotifications::empty());
        let use_case = AddCollaboratorUseCase::new(
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
        assert_eq!(created[0].kind, NotificationKind::CollaboratorAdded);
        assert_eq!(
            created[0].role,
            Some(CollaboratorRole::Maintainer.as_str().to_string())
        );
    }

    #[tokio::test]
    async fn adding_a_collaborator_dispatches_a_webhook() {
        let owner = user("owner");
        let target = user("alice");
        let repo_id = Uuid::new_v4();
        let collaborators = Arc::new(FakeCollaborators::empty());
        let webhooks = Arc::new(FakeWebhooks::default());
        let use_case = AddCollaboratorUseCase::new(
            collaborators.clone(),
            Arc::new(FakeUsers::new(vec![owner.clone(), target.clone()])),
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
                owner.id,
                owner.id,
                "alice",
                CollaboratorRole::Maintainer,
            )
            .await
            .unwrap();

        let dispatched = webhooks.dispatched();
        assert_eq!(dispatched.len(), 1);
        assert_eq!(dispatched[0].0, repo_id);
        match &dispatched[0].1 {
            WebhookEvent::CollaboratorAdded {
                target_username,
                role,
                ..
            } => {
                assert_eq!(target_username, "alice");
                assert_eq!(role, CollaboratorRole::Maintainer.as_str());
            }
            other => panic!("expected CollaboratorAdded, got {other:?}"),
        }
    }
}
