use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::issue::{Issue, IssueStatus, IssueStorePort};
use ferrisgit_domain::notification::{NewNotification, NotificationKind, NotificationStorePort};
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::user::UserRepositoryPort;
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use ferrisgit_domain::webhook_event::WebhookEvent;
use uuid::Uuid;

pub struct CloseIssueUseCase {
    issues: Arc<dyn IssueStorePort>,
    repositories: Arc<dyn RepositoryStorePort>,
    users: Arc<dyn UserRepositoryPort>,
    notifications: Arc<dyn NotificationStorePort>,
    webhooks: Arc<dyn WebhookDispatcherPort>,
}

impl CloseIssueUseCase {
    pub fn new(
        issues: Arc<dyn IssueStorePort>,
        repositories: Arc<dyn RepositoryStorePort>,
        users: Arc<dyn UserRepositoryPort>,
        notifications: Arc<dyn NotificationStorePort>,
        webhooks: Arc<dyn WebhookDispatcherPort>,
    ) -> Self {
        Self {
            issues,
            repositories,
            users,
            notifications,
            webhooks,
        }
    }

    pub async fn execute(&self, issue_id: Uuid, actor_id: Uuid) -> Result<Issue, DomainError> {
        let existing = self
            .issues
            .find_by_id(issue_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("issue".to_string()))?;
        if existing.status == IssueStatus::Done {
            return Err(DomainError::Validation(
                "issue is already closed".to_string(),
            ));
        }
        self.issues.close(issue_id).await?;
        let issue = self
            .issues
            .find_by_id(issue_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("issue".to_string()))?;

        if let Some(repo) = self
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
                    WebhookEvent::IssueClosed {
                        repository_owner: owner.username.clone(),
                        repository_name: repo.name.clone(),
                        actor_username: actor.username.clone(),
                        issue_id: issue.id,
                        issue_title: issue.title.clone(),
                    },
                )
                .await
                .ok();

            if issue.author_id != actor_id {
                self.notifications
                    .create(NewNotification {
                        recipient_id: issue.author_id,
                        kind: NotificationKind::IssueClosed,
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

pub struct ReopenIssueUseCase {
    issues: Arc<dyn IssueStorePort>,
}

impl ReopenIssueUseCase {
    pub fn new(issues: Arc<dyn IssueStorePort>) -> Self {
        Self { issues }
    }

    pub async fn execute(&self, issue_id: Uuid) -> Result<Issue, DomainError> {
        let existing = self
            .issues
            .find_by_id(issue_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("issue".to_string()))?;
        if existing.status != IssueStatus::Done {
            return Err(DomainError::Validation("issue is not closed".to_string()));
        }
        self.issues.reopen(issue_id).await?;
        self.issues
            .find_by_id(issue_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("issue".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeIssues, FakeNotifications, FakeRepositories, FakeUsers, FakeWebhooks,
    };
    use ferrisgit_domain::issue::{IssueKind, IssueStatus};
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
            status: IssueStatus::InProgress,
            kind: IssueKind::Bug,
            parent_issue_id: None,
            created_at: chrono::Utc::now(),
            closed_at: None,
        }
    }

    #[tokio::test]
    async fn closing_someone_elses_issue_notifies_the_author() {
        let owner = user("owner");
        let author = user("author");
        let closer = user("closer");
        let repo = repository(owner.id);
        let seed_issue = issue(repo.id, author.id);
        let notifications = Arc::new(FakeNotifications::empty());
        let use_case = CloseIssueUseCase::new(
            Arc::new(FakeIssues::new(vec![seed_issue.clone()])),
            Arc::new(FakeRepositories::new(vec![repo.clone()])),
            Arc::new(FakeUsers::new(vec![
                owner.clone(),
                author.clone(),
                closer.clone(),
            ])),
            notifications.clone(),
            Arc::new(FakeWebhooks::default()),
        );

        use_case.execute(seed_issue.id, closer.id).await.unwrap();

        let created = notifications.snapshot();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].recipient_id, author.id);
        assert_eq!(created[0].kind, NotificationKind::IssueClosed);
    }

    #[tokio::test]
    async fn closing_your_own_issue_does_not_notify_yourself() {
        let owner = user("owner");
        let author = user("author");
        let repo = repository(owner.id);
        let seed_issue = issue(repo.id, author.id);
        let notifications = Arc::new(FakeNotifications::empty());
        let webhooks = Arc::new(FakeWebhooks::default());
        let use_case = CloseIssueUseCase::new(
            Arc::new(FakeIssues::new(vec![seed_issue.clone()])),
            Arc::new(FakeRepositories::new(vec![repo.clone()])),
            Arc::new(FakeUsers::new(vec![owner.clone(), author.clone()])),
            notifications.clone(),
            webhooks.clone(),
        );

        use_case.execute(seed_issue.id, author.id).await.unwrap();

        let created = notifications.snapshot();
        assert_eq!(created.len(), 0);
        let dispatched = webhooks.dispatched();
        assert_eq!(
            dispatched.len(),
            1,
            "the webhook must still fire even with no one to notify"
        );
        assert!(matches!(&dispatched[0].1, WebhookEvent::IssueClosed { .. }));
    }

    #[tokio::test]
    async fn closing_an_already_closed_issue_is_a_validation_error() {
        let owner = user("owner");
        let author = user("author");
        let repo = repository(owner.id);
        let mut seed_issue = issue(repo.id, author.id);
        seed_issue.status = IssueStatus::Done;
        seed_issue.closed_at = Some(chrono::Utc::now());
        let notifications = Arc::new(FakeNotifications::empty());
        let use_case = CloseIssueUseCase::new(
            Arc::new(FakeIssues::new(vec![seed_issue.clone()])),
            Arc::new(FakeRepositories::new(vec![repo.clone()])),
            Arc::new(FakeUsers::new(vec![owner.clone(), author.clone()])),
            notifications.clone(),
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case.execute(seed_issue.id, author.id).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
        let created = notifications.snapshot();
        assert_eq!(
            created.len(),
            0,
            "an already-closed issue must not re-fire a notification"
        );
    }

    #[tokio::test]
    async fn reopening_a_closed_issue_resets_its_status_and_never_notifies() {
        let author = user("author");
        let repo = repository(Uuid::new_v4());
        let mut seed_issue = issue(repo.id, author.id);
        seed_issue.status = IssueStatus::Done;
        seed_issue.closed_at = Some(chrono::Utc::now());
        // `ReopenIssueUseCase` has no `notifications` field: the type system proves reopening never notifies.
        let use_case = ReopenIssueUseCase::new(Arc::new(FakeIssues::new(vec![seed_issue.clone()])));

        let result = use_case.execute(seed_issue.id).await.unwrap();

        assert_eq!(result.status, IssueStatus::Todo);
    }

    #[tokio::test]
    async fn reopening_an_issue_that_is_not_closed_is_a_validation_error() {
        let author = user("author");
        let repo = repository(Uuid::new_v4());
        let seed_issue = issue(repo.id, author.id);
        assert_ne!(seed_issue.status, IssueStatus::Done);
        let use_case = ReopenIssueUseCase::new(Arc::new(FakeIssues::new(vec![seed_issue.clone()])));

        let result = use_case.execute(seed_issue.id).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }
}
