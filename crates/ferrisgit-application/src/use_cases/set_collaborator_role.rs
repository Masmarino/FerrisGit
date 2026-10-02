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

pub struct SetCollaboratorRoleUseCase {
    collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
    users: Arc<dyn UserRepositoryPort>,
    repositories: Arc<dyn RepositoryStorePort>,
    notifications: Arc<dyn NotificationStorePort>,
    webhooks: Arc<dyn WebhookDispatcherPort>,
    groups: Arc<dyn GroupStorePort>,
    group_membership: Arc<dyn GroupMembershipPort>,
}

impl SetCollaboratorRoleUseCase {
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
        // On a group repo `owner_id` is just the creator, not a role.
        if repo.group_id.is_none() && target.id == repo.owner_id {
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
            && let Some(ctx) =
                EventContext::for_repository(self.users.as_ref(), repo, caller_id).await
        {
            self.webhooks
                .dispatch(
                    repository_id,
                    WebhookEvent::CollaboratorRoleChanged {
                        repository_owner: ctx.owner_username.clone(),
                        repository_name: ctx.repository.name.clone(),
                        actor_username: ctx.actor_username.clone(),
                        target_username: target.username.clone(),
                        role: role.as_str().to_string(),
                    },
                )
                .await
                .ok();

            if caller_id != target.id {
                self.notifications
                    .create(NewNotification {
                        role: Some(role.as_str().to_string()),
                        ..ctx.notification(NotificationKind::CollaboratorRoleChanged, target.id)
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
        FakeCollaborators, FakeGroups, FakeNotifications, FakeRepositories, FakeUsers, FakeWebhooks,
    };
    use crate::use_cases::fixtures::{group, repository, user};
    use ferrisgit_domain::repository::Repository;
    use ferrisgit_domain::user::User;

    struct Harness {
        use_case: SetCollaboratorRoleUseCase,
        repo_id: Uuid,
        collaborators: Arc<FakeCollaborators>,
        notifications: Arc<FakeNotifications>,
        webhooks: Arc<FakeWebhooks>,
    }

    /// A personal repo of `owner`; `others` are extra accounts and `roles` the collaborator roles on that repo.
    fn harness(owner: &User, others: &[&User], roles: &[(&User, CollaboratorRole)]) -> Harness {
        let users = [&[owner], others].concat().into_iter().cloned().collect();
        harness_over(
            repository(owner.id),
            users,
            roles,
            Arc::new(FakeGroups::empty()),
        )
    }

    fn harness_over(
        repo: Repository,
        users: Vec<User>,
        roles: &[(&User, CollaboratorRole)],
        groups: Arc<FakeGroups>,
    ) -> Harness {
        let rows = roles
            .iter()
            .map(|(user, role)| (repo.id, user.id, *role))
            .collect();
        let collaborators = Arc::new(FakeCollaborators::new(rows));
        let notifications = Arc::new(FakeNotifications::empty());
        let webhooks = Arc::new(FakeWebhooks::default());
        Harness {
            use_case: SetCollaboratorRoleUseCase::new(
                collaborators.clone(),
                Arc::new(FakeUsers::new(users)),
                Arc::new(FakeRepositories::new(vec![repo.clone()])),
                notifications.clone(),
                webhooks.clone(),
                groups.clone(),
                groups,
            ),
            repo_id: repo.id,
            collaborators,
            notifications,
            webhooks,
        }
    }

    #[tokio::test]
    async fn the_owner_can_promote_a_collaborator_to_maintainer() {
        let owner = user("owner");
        let target = user("alice");
        let h = harness(&owner, &[&target], &[(&target, CollaboratorRole::Reader)]);

        h.use_case
            .execute(h.repo_id, owner.id, "alice", CollaboratorRole::Maintainer)
            .await
            .unwrap();

        assert_eq!(
            h.collaborators.snapshot().as_slice(),
            &[(h.repo_id, target.id, CollaboratorRole::Maintainer)]
        );
    }

    #[tokio::test]
    async fn a_maintainer_can_promote_another_collaborator_to_maintainer() {
        let owner = user("owner");
        let maintainer = user("maintainer");
        let target = user("alice");
        let h = harness(
            &owner,
            &[&maintainer, &target],
            &[
                (&maintainer, CollaboratorRole::Maintainer),
                (&target, CollaboratorRole::Reader),
            ],
        );

        h.use_case
            .execute(
                h.repo_id,
                maintainer.id,
                "alice",
                CollaboratorRole::Maintainer,
            )
            .await
            .unwrap();

        assert!(h.collaborators.snapshot().contains(&(
            h.repo_id,
            target.id,
            CollaboratorRole::Maintainer
        )));
    }

    #[tokio::test]
    async fn a_contributor_cannot_set_another_collaborators_role() {
        let owner = user("owner");
        let contributor = user("contributor");
        let target = user("alice");
        let h = harness(
            &owner,
            &[&contributor, &target],
            &[
                (&contributor, CollaboratorRole::Contributor),
                (&target, CollaboratorRole::Reader),
            ],
        );

        let result = h
            .use_case
            .execute(
                h.repo_id,
                contributor.id,
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
        let h = harness(&owner, &[&target], &[]);

        let result = h
            .use_case
            .execute(h.repo_id, owner.id, "alice", CollaboratorRole::Maintainer)
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn setting_the_role_of_an_unknown_username_is_a_validation_error() {
        let owner = user("owner");
        let h = harness(&owner, &[], &[]);

        let result = h
            .use_case
            .execute(h.repo_id, owner.id, "nobody", CollaboratorRole::Maintainer)
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn setting_the_owners_role_is_a_validation_error() {
        let owner = user("owner");
        let h = harness(&owner, &[], &[]);

        let result = h
            .use_case
            .execute(h.repo_id, owner.id, "owner", CollaboratorRole::Maintainer)
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn changing_another_collaborators_role_notifies_them() {
        let owner = user("owner");
        let target = user("alice");
        let h = harness(&owner, &[&target], &[(&target, CollaboratorRole::Reader)]);

        h.use_case
            .execute(h.repo_id, owner.id, "alice", CollaboratorRole::Maintainer)
            .await
            .unwrap();

        let created = h.notifications.snapshot();
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
        let h = harness(
            &owner,
            &[&maintainer],
            &[(&maintainer, CollaboratorRole::Maintainer)],
        );

        h.use_case
            .execute(
                h.repo_id,
                maintainer.id,
                "maintainer",
                CollaboratorRole::Maintainer,
            )
            .await
            .unwrap();

        assert!(h.notifications.snapshot().is_empty());
    }

    #[tokio::test]
    async fn changing_a_collaborators_role_dispatches_a_webhook_even_when_the_caller_changes_their_own_role()
     {
        let owner = user("owner");
        let maintainer = user("maintainer");
        let h = harness(
            &owner,
            &[&maintainer],
            &[(&maintainer, CollaboratorRole::Maintainer)],
        );

        h.use_case
            .execute(
                h.repo_id,
                maintainer.id,
                "maintainer",
                CollaboratorRole::Contributor,
            )
            .await
            .unwrap();

        let dispatched = h.webhooks.dispatched();
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
        let h = harness(
            &owner,
            &[&target],
            &[(&target, CollaboratorRole::Maintainer)],
        );

        h.use_case
            .execute(h.repo_id, owner.id, "alice", CollaboratorRole::Maintainer)
            .await
            .unwrap();

        assert!(
            h.webhooks.dispatched().is_empty(),
            "the idempotency check must gate the webhook too, unlike self-exclusion"
        );
    }

    #[tokio::test]
    async fn a_maintainer_of_the_repositorys_group_can_set_a_collaborators_role() {
        let creator = user("creator");
        let maintainer = user("maintainer");
        let target = user("alice");
        let team = group(None, "team");
        let repo = Repository {
            group_id: Some(team.id),
            ..repository(creator.id)
        };
        let groups = Arc::new(FakeGroups::new(vec![team.clone()]));
        groups
            .add_member(team.id, maintainer.id, CollaboratorRole::Maintainer)
            .await
            .unwrap();
        let h = harness_over(
            repo,
            vec![creator, maintainer.clone(), target.clone()],
            &[(&target, CollaboratorRole::Reader)],
            groups,
        );

        h.use_case
            .execute(
                h.repo_id,
                maintainer.id,
                "alice",
                CollaboratorRole::Contributor,
            )
            .await
            .unwrap();

        assert_eq!(
            h.collaborators.snapshot().as_slice(),
            &[(h.repo_id, target.id, CollaboratorRole::Contributor)]
        );
    }
}
