use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::group_membership::GroupMembershipPort;
use ferrisgit_domain::notification::{NotificationKind, NotificationStorePort};
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::repository_collaborator::RepositoryCollaboratorStorePort;
use ferrisgit_domain::user::UserRepositoryPort;
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use ferrisgit_domain::webhook_event::WebhookEvent;
use uuid::Uuid;

use super::collaborator_guard::require_collaborator_manager;
use super::event_context::EventContext;

pub struct RemoveCollaboratorUseCase {
    collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
    repositories: Arc<dyn RepositoryStorePort>,
    users: Arc<dyn UserRepositoryPort>,
    notifications: Arc<dyn NotificationStorePort>,
    webhooks: Arc<dyn WebhookDispatcherPort>,
    groups: Arc<dyn GroupStorePort>,
    group_membership: Arc<dyn GroupMembershipPort>,
}

impl RemoveCollaboratorUseCase {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
        repositories: Arc<dyn RepositoryStorePort>,
        users: Arc<dyn UserRepositoryPort>,
        notifications: Arc<dyn NotificationStorePort>,
        webhooks: Arc<dyn WebhookDispatcherPort>,
        groups: Arc<dyn GroupStorePort>,
        group_membership: Arc<dyn GroupMembershipPort>,
    ) -> Self {
        Self {
            collaborators,
            repositories,
            users,
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
        target_user_id: Uuid,
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
            && let Some(ctx) =
                EventContext::for_repository(self.users.as_ref(), repo, caller_id).await
            && let Some(target) = self.users.find_by_id(target_user_id).await.ok().flatten()
        {
            self.webhooks
                .dispatch(
                    repository_id,
                    WebhookEvent::CollaboratorRemoved {
                        repository_owner: ctx.owner_username.clone(),
                        repository_name: ctx.repository.name.clone(),
                        actor_username: ctx.actor_username.clone(),
                        target_username: target.username,
                    },
                )
                .await
                .ok();

            if caller_id != target_user_id {
                self.notifications
                    .create(ctx.notification(NotificationKind::CollaboratorRemoved, target_user_id))
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
    use ferrisgit_domain::repository_collaborator::CollaboratorRole;
    use ferrisgit_domain::user::User;
    use ferrisgit_domain::webhook_event::WebhookEvent;

    struct Harness {
        use_case: RemoveCollaboratorUseCase,
        repo_id: Uuid,
        collaborators: Arc<FakeCollaborators>,
        notifications: Arc<FakeNotifications>,
        webhooks: Arc<FakeWebhooks>,
    }

    /// A personal repository owned by `owner`. `others` exist as accounts, and `roles` are the collaborators they hold
    /// on it.
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
            use_case: RemoveCollaboratorUseCase::new(
                collaborators.clone(),
                Arc::new(FakeRepositories::new(vec![repo.clone()])),
                Arc::new(FakeUsers::new(users)),
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
    async fn the_owner_can_remove_a_collaborator() {
        let owner = user("owner");
        let target = user("alice");
        let h = harness(
            &owner,
            &[&target],
            &[(&target, CollaboratorRole::Contributor)],
        );

        h.use_case
            .execute(h.repo_id, owner.id, target.id)
            .await
            .unwrap();

        assert!(h.collaborators.snapshot().is_empty());
    }

    #[tokio::test]
    async fn a_non_owner_caller_is_rejected_as_not_found() {
        let h = harness(&user("owner"), &[], &[]);

        let result = h
            .use_case
            .execute(h.repo_id, Uuid::new_v4(), Uuid::new_v4())
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn a_maintainer_collaborator_can_remove_another_collaborator() {
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
            .execute(h.repo_id, maintainer.id, target.id)
            .await
            .unwrap();

        assert_eq!(
            h.collaborators.snapshot().as_slice(),
            &[(h.repo_id, maintainer.id, CollaboratorRole::Maintainer)]
        );
    }

    #[tokio::test]
    async fn a_maintainer_can_remove_themselves() {
        let owner = user("owner");
        let maintainer = user("maintainer");
        let h = harness(
            &owner,
            &[&maintainer],
            &[(&maintainer, CollaboratorRole::Maintainer)],
        );

        h.use_case
            .execute(h.repo_id, maintainer.id, maintainer.id)
            .await
            .unwrap();

        assert!(h.collaborators.snapshot().is_empty());
    }

    #[tokio::test]
    async fn a_contributor_collaborator_cannot_remove_another_collaborator() {
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
            .execute(h.repo_id, contributor.id, target.id)
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn removing_another_collaborator_notifies_them() {
        let owner = user("owner");
        let target = user("alice");
        let h = harness(
            &owner,
            &[&target],
            &[(&target, CollaboratorRole::Contributor)],
        );

        h.use_case
            .execute(h.repo_id, owner.id, target.id)
            .await
            .unwrap();

        let created = h.notifications.snapshot();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].recipient_id, target.id);
        assert_eq!(created[0].kind, NotificationKind::CollaboratorRemoved);
    }

    #[tokio::test]
    async fn removing_yourself_does_not_notify_yourself() {
        let owner = user("owner");
        let maintainer = user("maintainer");
        let h = harness(
            &owner,
            &[&maintainer],
            &[(&maintainer, CollaboratorRole::Maintainer)],
        );

        h.use_case
            .execute(h.repo_id, maintainer.id, maintainer.id)
            .await
            .unwrap();

        assert!(h.notifications.snapshot().is_empty());
    }

    #[tokio::test]
    async fn removing_a_target_who_was_never_a_collaborator_does_not_notify_them() {
        let owner = user("owner");
        let target = user("alice");
        // `target` holds no role on purpose. The store's `remove` is a no-op for a pair that never existed, so `execute`
        // must detect that beforehand and skip the notification.
        let h = harness(&owner, &[&target], &[]);

        let result = h.use_case.execute(h.repo_id, owner.id, target.id).await;

        assert!(
            result.is_ok(),
            "removing a non-collaborator is still a harmless success"
        );
        assert!(
            h.notifications.snapshot().is_empty(),
            "no notification should be sent for a removal that never actually happened"
        );
    }

    #[tokio::test]
    async fn a_self_removal_dispatches_a_webhook_with_zero_notifications() {
        let owner = user("owner");
        let maintainer = user("maintainer");
        let h = harness(
            &owner,
            &[&maintainer],
            &[(&maintainer, CollaboratorRole::Maintainer)],
        );

        h.use_case
            .execute(h.repo_id, maintainer.id, maintainer.id)
            .await
            .unwrap();

        assert!(
            h.notifications.snapshot().is_empty(),
            "self-exclusion must still gate the notification"
        );
        let dispatched = h.webhooks.dispatched();
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

    #[tokio::test]
    async fn a_maintainer_of_the_repositorys_group_can_remove_a_collaborator() {
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
            .execute(h.repo_id, maintainer.id, target.id)
            .await
            .unwrap();

        assert!(h.collaborators.snapshot().is_empty());
    }
}
