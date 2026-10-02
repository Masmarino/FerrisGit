use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::issue::{Issue, IssueStatus, IssueStorePort};
use ferrisgit_domain::notification::{NewNotification, NotificationKind, NotificationStorePort};
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::user::UserRepositoryPort;
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use ferrisgit_domain::webhook_event::WebhookEvent;
use uuid::Uuid;

use super::event_context::EventContext;
use super::find_issue::find_issue;

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
        let existing = find_issue(self.issues.as_ref(), issue_id).await?;
        if existing.status == IssueStatus::Done {
            return Err(DomainError::Validation(
                "issue is already closed".to_string(),
            ));
        }
        self.issues.close(issue_id).await?;
        let issue = find_issue(self.issues.as_ref(), issue_id).await?;

        if let Some(ctx) = EventContext::load(
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
                    WebhookEvent::IssueClosed {
                        repository_owner: ctx.owner_username.clone(),
                        repository_name: ctx.repository.name.clone(),
                        actor_username: ctx.actor_username.clone(),
                        issue_id: issue.id,
                        issue_title: issue.title.clone(),
                    },
                )
                .await
                .ok();

            if issue.author_id != actor_id {
                self.notifications
                    .create(NewNotification {
                        issue_id: Some(issue.id),
                        issue_number: Some(issue.number),
                        issue_title: Some(issue.title.clone()),
                        ..ctx.notification(NotificationKind::IssueClosed, issue.author_id)
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
        let existing = find_issue(self.issues.as_ref(), issue_id).await?;
        if existing.status != IssueStatus::Done {
            return Err(DomainError::Validation("issue is not closed".to_string()));
        }
        self.issues.reopen(issue_id).await?;
        find_issue(self.issues.as_ref(), issue_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeIssues, FakeNotifications, FakeRepositories, FakeUsers, FakeWebhooks,
    };
    use crate::use_cases::fixtures::{issue, repository, user};
    use ferrisgit_domain::user::User;
    use ferrisgit_domain::webhook_event::WebhookEvent;

    struct Fixture {
        use_case: CloseIssueUseCase,
        issue: Issue,
        notifications: Arc<FakeNotifications>,
        webhooks: Arc<FakeWebhooks>,
    }

    /// `issue` lives in a repository owned by `owner`; `users` must hold everybody the test acts as.
    fn fixture(owner: &User, users: Vec<User>, issue: impl FnOnce(Uuid) -> Issue) -> Fixture {
        let repo = repository(owner.id);
        let issue = issue(repo.id);
        let notifications = Arc::new(FakeNotifications::empty());
        let webhooks = Arc::new(FakeWebhooks::default());
        let use_case = CloseIssueUseCase::new(
            Arc::new(FakeIssues::new(vec![issue.clone()])),
            Arc::new(FakeRepositories::new(vec![repo])),
            Arc::new(FakeUsers::new(users)),
            notifications.clone(),
            webhooks.clone(),
        );
        Fixture {
            use_case,
            issue,
            notifications,
            webhooks,
        }
    }

    #[tokio::test]
    async fn closing_someone_elses_issue_notifies_the_author() {
        let owner = user("owner");
        let author = user("author");
        let closer = user("closer");
        let f = fixture(
            &owner,
            vec![owner.clone(), author.clone(), closer.clone()],
            |repo_id| issue(repo_id, author.id),
        );

        f.use_case.execute(f.issue.id, closer.id).await.unwrap();

        let created = f.notifications.snapshot();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].recipient_id, author.id);
        assert_eq!(created[0].kind, NotificationKind::IssueClosed);
    }

    #[tokio::test]
    async fn closing_your_own_issue_does_not_notify_yourself() {
        let owner = user("owner");
        let author = user("author");
        let f = fixture(&owner, vec![owner.clone(), author.clone()], |repo_id| {
            issue(repo_id, author.id)
        });

        f.use_case.execute(f.issue.id, author.id).await.unwrap();

        let created = f.notifications.snapshot();
        assert_eq!(created.len(), 0);
        let dispatched = f.webhooks.dispatched();
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
        let f = fixture(&owner, vec![owner.clone(), author.clone()], |repo_id| {
            Issue {
                status: IssueStatus::Done,
                closed_at: Some(chrono::Utc::now()),
                ..issue(repo_id, author.id)
            }
        });

        let result = f.use_case.execute(f.issue.id, author.id).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
        let created = f.notifications.snapshot();
        assert_eq!(
            created.len(),
            0,
            "an already-closed issue must not re-fire a notification"
        );
    }

    #[tokio::test]
    async fn reopening_a_closed_issue_resets_its_status_and_never_notifies() {
        let closed = Issue {
            status: IssueStatus::Done,
            closed_at: Some(chrono::Utc::now()),
            ..issue(Uuid::new_v4(), Uuid::new_v4())
        };
        // `ReopenIssueUseCase` has no `notifications` field: the type system proves reopening never notifies.
        let use_case = ReopenIssueUseCase::new(Arc::new(FakeIssues::new(vec![closed.clone()])));

        let result = use_case.execute(closed.id).await.unwrap();

        assert_eq!(result.status, IssueStatus::Todo);
    }

    #[tokio::test]
    async fn reopening_an_issue_that_is_not_closed_is_a_validation_error() {
        let open = issue(Uuid::new_v4(), Uuid::new_v4());
        assert_ne!(open.status, IssueStatus::Done);
        let use_case = ReopenIssueUseCase::new(Arc::new(FakeIssues::new(vec![open.clone()])));

        let result = use_case.execute(open.id).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }
}
