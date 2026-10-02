use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::group_membership::GroupMembershipPort;
use ferrisgit_domain::notification::{NewNotification, NotificationKind, NotificationStorePort};
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::repository_collaborator::{
    CollaboratorRole, RepositoryCollaboratorStorePort,
};
use ferrisgit_domain::user::UserRepositoryPort;
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use ferrisgit_domain::webhook_event::WebhookEvent;
use uuid::Uuid;

use super::collaborator_guard::require_collaborator_manager;
use super::event_context::EventContext;

pub struct AddCollaboratorUseCase {
    collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
    users: Arc<dyn UserRepositoryPort>,
    repositories: Arc<dyn RepositoryStorePort>,
    notifications: Arc<dyn NotificationStorePort>,
    webhooks: Arc<dyn WebhookDispatcherPort>,
    groups: Arc<dyn GroupStorePort>,
    group_membership: Arc<dyn GroupMembershipPort>,
}

impl AddCollaboratorUseCase {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
        users: Arc<dyn UserRepositoryPort>,
        repositories: Arc<dyn RepositoryStorePort>,
        notifications: Arc<dyn NotificationStorePort>,
        webhooks: Arc<dyn WebhookDispatcherPort>,
        groups: Arc<dyn GroupStorePort>,
        group_membership: Arc<dyn GroupMembershipPort>,
    ) -> Self {
        Self {
            collaborators,
            users,
            repositories,
            notifications,
            webhooks,
            groups,
            group_membership,
        }
    }

    pub async fn execute(
        &self,
        repository_id: Uuid,
        caller_id: Uuid,
        username: &str,
        role: CollaboratorRole,
    ) -> Result<(), DomainError> {
        let repo = require_collaborator_manager(
            self.repositories.as_ref(),
            self.collaborators.as_ref(),
            self.groups.as_ref(),
            self.group_membership.as_ref(),
            repository_id,
            caller_id,
        )
        .await?;
        let target = self
            .users
            .find_by_username(username)
            .await?
            .ok_or_else(|| DomainError::Validation("no such user".to_string()))?;
        // A group repository's `owner_id` only records who created it: it carries no implicit role.
        if repo.group_id.is_none() && target.id == repo.owner_id {
            return Err(DomainError::Validation(
                "the owner is already a collaborator".to_string(),
            ));
        }
        self.collaborators
            .add(repository_id, target.id, role)
            .await?;

        if let Some(ctx) = EventContext::for_repository(self.users.as_ref(), repo, caller_id).await
        {
            self.webhooks
                .dispatch(
                    repository_id,
                    WebhookEvent::CollaboratorAdded {
                        repository_owner: ctx.owner_username.clone(),
                        repository_name: ctx.repository.name.clone(),
                        actor_username: ctx.actor_username.clone(),
                        target_username: target.username.clone(),
                        role: role.as_str().to_string(),
                    },
                )
                .await
                .ok();

            self.notifications
                .create(NewNotification {
                    role: Some(role.as_str().to_string()),
                    ..ctx.notification(NotificationKind::CollaboratorAdded, target.id)
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
        FakeCollaborators, FakeGroups, FakeNotifications, FakeRepositories, FakeUsers, FakeWebhooks,
    };
    use crate::use_cases::fixtures::{group, repository, user};
    use ferrisgit_domain::repository::Repository;
    use ferrisgit_domain::user::User;

    struct Fixture {
        use_case: AddCollaboratorUseCase,
        collaborators: Arc<FakeCollaborators>,
        notifications: Arc<FakeNotifications>,
        webhooks: Arc<FakeWebhooks>,
    }

    fn fixture(users: Vec<User>, repository: Repository) -> Fixture {
        fixture_with(
            users,
            repository,
            FakeCollaborators::empty(),
            FakeGroups::empty(),
        )
    }

    fn fixture_with(
        users: Vec<User>,
        repository: Repository,
        collaborators: FakeCollaborators,
        groups: FakeGroups,
    ) -> Fixture {
        let collaborators = Arc::new(collaborators);
        let notifications = Arc::new(FakeNotifications::empty());
        let webhooks = Arc::new(FakeWebhooks::default());
        let groups = Arc::new(groups);
        let use_case = AddCollaboratorUseCase::new(
            collaborators.clone(),
            Arc::new(FakeUsers::new(users)),
            Arc::new(FakeRepositories::new(vec![repository])),
            notifications.clone(),
            webhooks.clone(),
            groups.clone(),
            groups,
        );
        Fixture {
            use_case,
            collaborators,
            notifications,
            webhooks,
        }
    }

    #[tokio::test]
    async fn the_owner_can_add_an_existing_user_as_a_collaborator() {
        let owner = user("owner");
        let target = user("alice");
        let repo = repository(owner.id);
        let f = fixture(vec![owner.clone(), target.clone()], repo.clone());

        f.use_case
            .execute(repo.id, owner.id, "alice", CollaboratorRole::Contributor)
            .await
            .unwrap();

        assert_eq!(
            f.collaborators.snapshot().as_slice(),
            &[(repo.id, target.id, CollaboratorRole::Contributor)]
        );
    }

    #[tokio::test]
    async fn a_non_owner_caller_is_rejected_as_not_found() {
        let owner = user("owner");
        let repo = repository(owner.id);
        let f = fixture(vec![owner, user("alice")], repo.clone());

        let result = f
            .use_case
            .execute(
                repo.id,
                Uuid::new_v4(),
                "alice",
                CollaboratorRole::Contributor,
            )
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn adding_an_unknown_username_is_a_validation_error() {
        let owner = user("owner");
        let repo = repository(owner.id);
        let f = fixture(vec![owner.clone()], repo.clone());

        let result = f
            .use_case
            .execute(repo.id, owner.id, "nobody", CollaboratorRole::Contributor)
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn adding_the_owner_as_their_own_collaborator_is_a_validation_error() {
        let owner = user("owner");
        let repo = repository(owner.id);
        let f = fixture(vec![owner.clone()], repo.clone());

        let result = f
            .use_case
            .execute(repo.id, owner.id, "owner", CollaboratorRole::Contributor)
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn a_maintainer_collaborator_can_add_another_collaborator() {
        let owner = user("owner");
        let maintainer = user("maintainer");
        let target = user("alice");
        let repo = repository(owner.id);
        let f = fixture_with(
            vec![owner, maintainer.clone(), target.clone()],
            repo.clone(),
            FakeCollaborators::new(vec![(repo.id, maintainer.id, CollaboratorRole::Maintainer)]),
            FakeGroups::empty(),
        );

        f.use_case
            .execute(
                repo.id,
                maintainer.id,
                "alice",
                CollaboratorRole::Maintainer,
            )
            .await
            .unwrap();

        assert!(f.collaborators.snapshot().contains(&(
            repo.id,
            target.id,
            CollaboratorRole::Maintainer
        )));
    }

    #[tokio::test]
    async fn a_contributor_collaborator_cannot_add_another_collaborator() {
        let owner = user("owner");
        let contributor = user("contributor");
        let repo = repository(owner.id);
        let f = fixture_with(
            vec![owner, contributor.clone()],
            repo.clone(),
            FakeCollaborators::new(vec![(
                repo.id,
                contributor.id,
                CollaboratorRole::Contributor,
            )]),
            FakeGroups::empty(),
        );

        let result = f
            .use_case
            .execute(repo.id, contributor.id, "owner", CollaboratorRole::Reader)
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn adding_a_collaborator_notifies_them() {
        let owner = user("owner");
        let target = user("alice");
        let repo = repository(owner.id);
        let f = fixture(vec![owner.clone(), target.clone()], repo.clone());

        f.use_case
            .execute(repo.id, owner.id, "alice", CollaboratorRole::Maintainer)
            .await
            .unwrap();

        let created = f.notifications.snapshot();
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
        let repo = repository(owner.id);
        let f = fixture(vec![owner.clone(), user("alice")], repo.clone());

        f.use_case
            .execute(repo.id, owner.id, "alice", CollaboratorRole::Maintainer)
            .await
            .unwrap();

        let dispatched = f.webhooks.dispatched();
        assert_eq!(dispatched.len(), 1);
        assert_eq!(dispatched[0].0, repo.id);
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

    /// A repository in `child`, itself below `root`, whose creator holds no role anywhere. `member` holds
    /// `member_role` on `root`.
    async fn group_repository_fixture(
        member: &User,
        member_role: CollaboratorRole,
        target: &User,
    ) -> (Uuid, Fixture) {
        let owner = user("owner");
        let root = group(None, "root");
        let child = group(Some(root.id), "child");
        let repo = Repository {
            group_id: Some(child.id),
            ..repository(owner.id)
        };
        let groups = FakeGroups::new(vec![root.clone(), child]);
        groups
            .add_member(root.id, member.id, member_role)
            .await
            .unwrap();
        let f = fixture_with(
            vec![owner, member.clone(), target.clone()],
            repo.clone(),
            FakeCollaborators::empty(),
            groups,
        );
        (repo.id, f)
    }

    #[tokio::test]
    async fn a_maintainer_of_an_ancestor_group_can_add_a_collaborator_to_a_group_repository() {
        let maintainer = user("maintainer");
        let target = user("alice");
        let (repo_id, f) =
            group_repository_fixture(&maintainer, CollaboratorRole::Maintainer, &target).await;

        f.use_case
            .execute(repo_id, maintainer.id, "alice", CollaboratorRole::Reader)
            .await
            .unwrap();

        assert_eq!(
            f.collaborators.snapshot().as_slice(),
            &[(repo_id, target.id, CollaboratorRole::Reader)]
        );
    }

    #[tokio::test]
    async fn a_contributor_of_an_ancestor_group_cannot_add_a_collaborator_to_a_group_repository() {
        let contributor = user("contributor");
        let target = user("alice");
        let (repo_id, f) =
            group_repository_fixture(&contributor, CollaboratorRole::Contributor, &target).await;

        let result = f
            .use_case
            .execute(repo_id, contributor.id, "alice", CollaboratorRole::Reader)
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
        assert!(f.collaborators.snapshot().is_empty());
    }
}
