use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::issue::{Issue, IssueStorePort};
use ferrisgit_domain::notification::{NewNotification, NotificationKind, NotificationStorePort};
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::repository_collaborator::RepositoryCollaboratorStorePort;
use ferrisgit_domain::user::UserRepositoryPort;
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use ferrisgit_domain::webhook_event::WebhookEvent;
use uuid::Uuid;

pub struct AssignIssueUseCase {
    issues: Arc<dyn IssueStorePort>,
    repositories: Arc<dyn RepositoryStorePort>,
    collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
    users: Arc<dyn UserRepositoryPort>,
    notifications: Arc<dyn NotificationStorePort>,
    webhooks: Arc<dyn WebhookDispatcherPort>,
}

impl AssignIssueUseCase {
    pub fn new(
        issues: Arc<dyn IssueStorePort>,
        repositories: Arc<dyn RepositoryStorePort>,
        collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
        users: Arc<dyn UserRepositoryPort>,
        notifications: Arc<dyn NotificationStorePort>,
        webhooks: Arc<dyn WebhookDispatcherPort>,
    ) -> Self {
        Self {
            issues,
            repositories,
            collaborators,
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
        let existing = self
            .issues
            .find_by_id(issue_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("issue".to_string()))?;

        if let Some(new_assignee) = assignee_id {
            let repo = self
                .repositories
                .find_by_id(existing.repository_id)
                .await?
                .ok_or_else(|| DomainError::NotFound("repository".to_string()))?;
            let has_access = repo.owner_id == new_assignee
                || self
                    .collaborators
                    .get_role(existing.repository_id, new_assignee)
                    .await?
                    .is_some();
            if !has_access {
                return Err(DomainError::Validation(
                    "assignee is not a collaborator on this repository".to_string(),
                ));
            }
        }

        self.issues.assign(issue_id, assignee_id).await?;
        let issue = self
            .issues
            .find_by_id(issue_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("issue".to_string()))?;

        if let Some(new_assignee) = assignee_id
            && let Some(repo) = self
                .repositories
                .find_by_id(issue.repository_id)
                .await
                .ok()
                .flatten()
            && let Some(owner) = self.users.find_by_id(repo.owner_id).await.ok().flatten()
            && let Some(actor) = self.users.find_by_id(actor_id).await.ok().flatten()
        {
            self.webhooks
                .dispatch(
                    issue.repository_id,
                    WebhookEvent::IssueAssigned {
                        repository_owner: owner.username.clone(),
                        repository_name: repo.name.clone(),
                        actor_username: actor.username.clone(),
                        issue_id: issue.id,
                        issue_title: issue.title.clone(),
                    },
                )
                .await
                .ok();

            if new_assignee != actor_id {
                self.notifications
                    .create(NewNotification {
                        recipient_id: new_assignee,
                        kind: NotificationKind::IssueAssigned,
                        repository_owner: owner.username,
                        repository_name: repo.name,
                        actor_username: Some(actor.username),
                        merge_request_id: None,
                        merge_request_title: None,
                        pipeline_id: None,
                        commit_sha: None,
                        role: None,
                        issue_id: Some(issue.id),
                        issue_number: Some(issue.number),
                        issue_title: Some(issue.title.clone()),
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
        FakeCollaborators, FakeIssues, FakeNotifications, FakeRepositories, FakeUsers, FakeWebhooks,
    };
    use ferrisgit_domain::issue::{IssueKind, IssueStatus};
    use ferrisgit_domain::repository::{Repository, RepositoryVisibility};
    use ferrisgit_domain::repository_collaborator::CollaboratorRole;
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

    fn issue(repository_id: Uuid, author_id: Uuid) -> Issue {
        Issue {
            id: Uuid::new_v4(),
            repository_id,
            number: 1,
            author_id,
            assignee_id: None,
            milestone_id: None,
            title: "bug".to_string(),
            description: "desc".to_string(),
            status: IssueStatus::Todo,
            kind: IssueKind::Bug,
            parent_issue_id: None,
            created_at: chrono::Utc::now(),
            closed_at: None,
        }
    }

    #[tokio::test]
    async fn assigning_an_issue_to_someone_else_notifies_them() {
        let owner = user("owner");
        let assignee = user("alice");
        let repo = repository(owner.id);
        let seed_issue = issue(repo.id, owner.id);
        let notifications = Arc::new(FakeNotifications::empty());
        let use_case = AssignIssueUseCase::new(
            Arc::new(FakeIssues::new(vec![seed_issue.clone()])),
            Arc::new(FakeRepositories::new(vec![repo.clone()])),
            Arc::new(FakeCollaborators::new(vec![(
                repo.id,
                assignee.id,
                CollaboratorRole::Contributor,
            )])),
            Arc::new(FakeUsers::new(vec![owner.clone(), assignee.clone()])),
            notifications.clone(),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(seed_issue.id, owner.id, Some(assignee.id))
            .await
            .unwrap();

        assert_eq!(result.assignee_id, Some(assignee.id));
        let created = notifications.snapshot();
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
        let notifications = Arc::new(FakeNotifications::empty());
        let use_case = AssignIssueUseCase::new(
            Arc::new(FakeIssues::new(vec![seed_issue.clone()])),
            Arc::new(FakeRepositories::new(vec![repo.clone()])),
            Arc::new(FakeCollaborators::empty()),
            Arc::new(FakeUsers::new(vec![owner.clone(), stranger.clone()])),
            notifications.clone(),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(seed_issue.id, owner.id, Some(stranger.id))
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
        assert!(notifications.snapshot().is_empty());
    }

    #[tokio::test]
    async fn self_assigning_an_issue_does_not_notify_yourself() {
        let owner = user("owner");
        let repo = repository(owner.id);
        let seed_issue = issue(repo.id, owner.id);
        let notifications = Arc::new(FakeNotifications::empty());
        let webhooks = Arc::new(FakeWebhooks::default());
        let use_case = AssignIssueUseCase::new(
            Arc::new(FakeIssues::new(vec![seed_issue.clone()])),
            Arc::new(FakeRepositories::new(vec![repo.clone()])),
            Arc::new(FakeCollaborators::empty()),
            Arc::new(FakeUsers::new(vec![owner.clone()])),
            notifications.clone(),
            webhooks.clone(),
        );

        let result = use_case
            .execute(seed_issue.id, owner.id, Some(owner.id))
            .await
            .unwrap();

        assert_eq!(result.assignee_id, Some(owner.id));
        assert_eq!(notifications.snapshot().len(), 0);
        let dispatched = webhooks.dispatched();
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
}
