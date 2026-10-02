use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::group_membership::GroupMembershipPort;
use ferrisgit_domain::issue::{Issue, IssueStorePort};
use ferrisgit_domain::notification::{NewNotification, NotificationKind, NotificationStorePort};
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::repository_authz::effective_repository_role;
use ferrisgit_domain::repository_collaborator::{
    CollaboratorRole, RepositoryCollaboratorStorePort,
};
use ferrisgit_domain::user::UserRepositoryPort;
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use ferrisgit_domain::webhook_event::WebhookEvent;
use uuid::Uuid;

use super::event_context::EventContext;
use super::find_issue::find_issue;

pub struct AssignIssueUseCase {
    issues: Arc<dyn IssueStorePort>,
    repositories: Arc<dyn RepositoryStorePort>,
    collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
    groups: Arc<dyn GroupStorePort>,
    group_membership: Arc<dyn GroupMembershipPort>,
    users: Arc<dyn UserRepositoryPort>,
    notifications: Arc<dyn NotificationStorePort>,
    webhooks: Arc<dyn WebhookDispatcherPort>,
}

impl AssignIssueUseCase {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        issues: Arc<dyn IssueStorePort>,
        repositories: Arc<dyn RepositoryStorePort>,
        collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
        groups: Arc<dyn GroupStorePort>,
        group_membership: Arc<dyn GroupMembershipPort>,
        users: Arc<dyn UserRepositoryPort>,
        notifications: Arc<dyn NotificationStorePort>,
        webhooks: Arc<dyn WebhookDispatcherPort>,
    ) -> Self {
        Self {
            issues,
            repositories,
            collaborators,
            groups,
            group_membership,
            users,
            notifications,
            webhooks,
        }
    }

    pub async fn execute(
        &self,
        issue_id: Uuid,
        actor_id: Uuid,
        assignee_id: Option<Uuid>,
    ) -> Result<Issue, DomainError> {
        let existing = find_issue(self.issues.as_ref(), issue_id).await?;

        if let Some(new_assignee) = assignee_id {
            let repo = self
                .repositories
                .find_by_id(existing.repository_id)
                .await?
                .ok_or_else(|| DomainError::NotFound("repository".to_string()))?;
            // The assignee must be able to work on the issue: Contributor or more, whether the role is direct or
            // inherited from a group.
            let can_contribute = effective_repository_role(
                self.collaborators.as_ref(),
                self.groups.as_ref(),
                self.group_membership.as_ref(),
                &repo,
                new_assignee,
            )
            .await?
            .is_some_and(|role| role >= CollaboratorRole::Contributor);
            if !can_contribute {
                return Err(DomainError::Validation(
                    "assignee must be at least a contributor on this repository".to_string(),
                ));
            }
        }

        self.issues.assign(issue_id, assignee_id).await?;
        let issue = find_issue(self.issues.as_ref(), issue_id).await?;

        if let Some(new_assignee) = assignee_id
            && let Some(ctx) = EventContext::load(
                self.repositories.as_ref(),
                self.users.as_ref(),
                issue.repository_id,
                actor_id,
            )
            .await
        {
            self.webhooks
                .dispatch(
                    issue.repository_id,
                    WebhookEvent::IssueAssigned {
                        repository_owner: ctx.owner_username.clone(),
                        repository_name: ctx.repository.name.clone(),
                        actor_username: ctx.actor_username.clone(),
                        issue_id: issue.id,
                        issue_title: issue.title.clone(),
                    },
                )
                .await
                .ok();

            if new_assignee != actor_id {
                self.notifications
                    .create(NewNotification {
                        issue_id: Some(issue.id),
                        issue_number: Some(issue.number),
                        issue_title: Some(issue.title.clone()),
                        ..ctx.notification(NotificationKind::IssueAssigned, new_assignee)
                    })
                    .await
                    .ok();
            }
        }

        Ok(issue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeCollaborators, FakeGroups, FakeIssues, FakeNotifications, FakeRepositories, FakeUsers,
        FakeWebhooks,
    };
    use crate::use_cases::fixtures::{group, issue, repository, user};
    use ferrisgit_domain::repository::Repository;
    use ferrisgit_domain::user::User;
    use ferrisgit_domain::webhook_event::WebhookEvent;

    struct Fixture {
        use_case: AssignIssueUseCase,
        notifications: Arc<FakeNotifications>,
        webhooks: Arc<FakeWebhooks>,
    }

    fn fixture(users: Vec<User>, repository: Repository, issue: Issue) -> Fixture {
        fixture_with(
            users,
            repository,
            issue,
            FakeCollaborators::empty(),
            FakeGroups::empty(),
        )
    }

    fn fixture_with(
        users: Vec<User>,
        repository: Repository,
        issue: Issue,
        collaborators: FakeCollaborators,
        groups: FakeGroups,
    ) -> Fixture {
        let notifications = Arc::new(FakeNotifications::empty());
        let webhooks = Arc::new(FakeWebhooks::default());
        let groups = Arc::new(groups);
        let use_case = AssignIssueUseCase::new(
            Arc::new(FakeIssues::new(vec![issue])),
            Arc::new(FakeRepositories::new(vec![repository])),
            Arc::new(collaborators),
            groups.clone(),
            groups,
            Arc::new(FakeUsers::new(users)),
            notifications.clone(),
            webhooks.clone(),
        );
        Fixture {
            use_case,
            notifications,
            webhooks,
        }
    }

    #[tokio::test]
    async fn assigning_an_issue_to_someone_else_notifies_them() {
        let owner = user("owner");
        let assignee = user("alice");
        let repo = repository(owner.id);
        let seed_issue = issue(repo.id, owner.id);
        let f = fixture_with(
            vec![owner.clone(), assignee.clone()],
            repo.clone(),
            seed_issue.clone(),
            FakeCollaborators::new(vec![(repo.id, assignee.id, CollaboratorRole::Contributor)]),
            FakeGroups::empty(),
        );

        let result = f
            .use_case
            .execute(seed_issue.id, owner.id, Some(assignee.id))
            .await
            .unwrap();

        assert_eq!(result.assignee_id, Some(assignee.id));
        let created = f.notifications.snapshot();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].recipient_id, assignee.id);
        assert_eq!(created[0].kind, NotificationKind::IssueAssigned);
    }

    #[tokio::test]
    async fn assigning_an_issue_to_someone_who_is_not_a_collaborator_is_a_validation_error() {
        let owner = user("owner");
        let stranger = user("stranger");
        let repo = repository(owner.id);
        let seed_issue = issue(repo.id, owner.id);
        let f = fixture(
            vec![owner.clone(), stranger.clone()],
            repo,
            seed_issue.clone(),
        );

        let result = f
            .use_case
            .execute(seed_issue.id, owner.id, Some(stranger.id))
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
        assert!(f.notifications.snapshot().is_empty());
    }

    #[tokio::test]
    async fn self_assigning_an_issue_does_not_notify_yourself() {
        let owner = user("owner");
        let repo = repository(owner.id);
        let seed_issue = issue(repo.id, owner.id);
        let f = fixture(vec![owner.clone()], repo, seed_issue.clone());

        let result = f
            .use_case
            .execute(seed_issue.id, owner.id, Some(owner.id))
            .await
            .unwrap();

        assert_eq!(result.assignee_id, Some(owner.id));
        assert_eq!(f.notifications.snapshot().len(), 0);
        let dispatched = f.webhooks.dispatched();
        assert_eq!(
            dispatched.len(),
            1,
            "the webhook must still fire even when self-assigning notifies no one"
        );
        assert!(matches!(
            &dispatched[0].1,
            WebhookEvent::IssueAssigned { .. }
        ));
    }

    #[tokio::test]
    async fn a_reader_collaborator_cannot_be_assigned() {
        let owner = user("owner");
        let reader = user("reader");
        let repo = repository(owner.id);
        let seed_issue = issue(repo.id, owner.id);
        let f = fixture_with(
            vec![owner.clone(), reader.clone()],
            repo.clone(),
            seed_issue.clone(),
            FakeCollaborators::new(vec![(repo.id, reader.id, CollaboratorRole::Reader)]),
            FakeGroups::empty(),
        );

        let result = f
            .use_case
            .execute(seed_issue.id, owner.id, Some(reader.id))
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    /// A repository in `child`, below `root`; `member` holds `role` on `root` only.
    async fn assign_in_group_repository(role: CollaboratorRole) -> Result<Issue, DomainError> {
        let creator = user("creator");
        let member = user("member");
        let root = group(None, "root");
        let child = group(Some(root.id), "child");
        let repo = Repository {
            group_id: Some(child.id),
            ..repository(creator.id)
        };
        let seed_issue = issue(repo.id, creator.id);
        let groups = FakeGroups::new(vec![root.clone(), child]);
        groups.add_member(root.id, member.id, role).await.unwrap();
        let f = fixture_with(
            vec![creator, member.clone()],
            repo,
            seed_issue.clone(),
            FakeCollaborators::empty(),
            groups,
        );
        f.use_case
            .execute(seed_issue.id, member.id, Some(member.id))
            .await
    }

    #[tokio::test]
    async fn a_contributor_through_a_group_can_be_assigned() {
        let assigned = assign_in_group_repository(CollaboratorRole::Contributor)
            .await
            .unwrap();

        assert!(assigned.assignee_id.is_some());
    }

    #[tokio::test]
    async fn a_reader_through_a_group_cannot_be_assigned() {
        let result = assign_in_group_repository(CollaboratorRole::Reader).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }
}
