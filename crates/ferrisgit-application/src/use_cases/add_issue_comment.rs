use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::issue::{IssueComment, IssueStorePort, NewIssueComment};
use ferrisgit_domain::issue_comment::IssueCommentPort;
use ferrisgit_domain::notification::{NewNotification, NotificationKind, NotificationStorePort};
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::user::UserRepositoryPort;
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use ferrisgit_domain::webhook_event::WebhookEvent;
use uuid::Uuid;

use super::event_context::EventContext;
use super::find_issue::find_issue;

pub struct AddIssueCommentUseCase {
    issues: Arc<dyn IssueStorePort>,
    issue_comments: Arc<dyn IssueCommentPort>,
    repositories: Arc<dyn RepositoryStorePort>,
    users: Arc<dyn UserRepositoryPort>,
    notifications: Arc<dyn NotificationStorePort>,
    webhooks: Arc<dyn WebhookDispatcherPort>,
}

impl AddIssueCommentUseCase {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        issues: Arc<dyn IssueStorePort>,
        issue_comments: Arc<dyn IssueCommentPort>,
        repositories: Arc<dyn RepositoryStorePort>,
        users: Arc<dyn UserRepositoryPort>,
        notifications: Arc<dyn NotificationStorePort>,
        webhooks: Arc<dyn WebhookDispatcherPort>,
    ) -> Self {
        Self {
            issues,
            issue_comments,
            repositories,
            users,
            notifications,
            webhooks,
        }
    }

    pub async fn execute(
        &self,
        issue_id: Uuid,
        author_id: Uuid,
        body: String,
    ) -> Result<IssueComment, DomainError> {
        let issue = find_issue(self.issues.as_ref(), issue_id).await?;
        let comment = self
            .issue_comments
            .add_comment(NewIssueComment {
                issue_id,
                author_id,
                body,
            })
            .await?;

        if let Some(ctx) = EventContext::load(
            self.repositories.as_ref(),
            self.users.as_ref(),
            issue.repository_id,
            author_id,
        )
        .await
        {
            self.webhooks
                .dispatch(
                    issue.repository_id,
                    WebhookEvent::IssueCommented {
                        repository_owner: ctx.owner_username.clone(),
                        repository_name: ctx.repository.name.clone(),
                        actor_username: ctx.actor_username.clone(),
                        issue_id: issue.id,
                        issue_title: issue.title.clone(),
                    },
                )
                .await
                .ok();

            let mut recipients: Vec<Uuid> = Vec::new();
            if issue.author_id != author_id {
                recipients.push(issue.author_id);
            }
            if let Some(assignee_id) = issue.assignee_id
                && assignee_id != author_id
                && !recipients.contains(&assignee_id)
            {
                recipients.push(assignee_id);
            }

            for recipient_id in recipients {
                self.notifications
                    .create(NewNotification {
                        issue_id: Some(issue.id),
                        issue_number: Some(issue.number),
                        issue_title: Some(issue.title.clone()),
                        ..ctx.notification(NotificationKind::IssueCommented, recipient_id)
                    })
                    .await
                    .ok();
            }
        }

        Ok(comment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeIssues, FakeNotifications, FakeRepositories, FakeUsers, FakeWebhooks,
    };
    use crate::use_cases::fixtures::{issue, repository, user};
    use ferrisgit_domain::issue::Issue;
    use ferrisgit_domain::user::User;
    use ferrisgit_domain::webhook_event::WebhookEvent;

    /// An issue written by `author`, assigned to `assignee`, in a repository owned by somebody else. `commenter` is a
    /// fourth, unrelated user.
    struct Fixture {
        use_case: AddIssueCommentUseCase,
        issue: Issue,
        author: User,
        assignee: User,
        commenter: User,
        notifications: Arc<FakeNotifications>,
        webhooks: Arc<FakeWebhooks>,
    }

    fn fixture() -> Fixture {
        let owner = user("owner");
        let author = user("author");
        let assignee = user("assignee");
        let commenter = user("commenter");
        let repo = repository(owner.id);
        let issue = Issue {
            assignee_id: Some(assignee.id),
            ..issue(repo.id, author.id)
        };
        let issues = Arc::new(FakeIssues::new(vec![issue.clone()]));
        let notifications = Arc::new(FakeNotifications::empty());
        let webhooks = Arc::new(FakeWebhooks::default());
        let use_case = AddIssueCommentUseCase::new(
            issues.clone(),
            issues,
            Arc::new(FakeRepositories::new(vec![repo])),
            Arc::new(FakeUsers::new(vec![
                owner,
                author.clone(),
                assignee.clone(),
                commenter.clone(),
            ])),
            notifications.clone(),
            webhooks.clone(),
        );
        Fixture {
            use_case,
            issue,
            author,
            assignee,
            commenter,
            notifications,
            webhooks,
        }
    }

    #[tokio::test]
    async fn a_third_party_commenting_notifies_both_author_and_assignee() {
        let f = fixture();

        f.use_case
            .execute(f.issue.id, f.commenter.id, "nice bug".to_string())
            .await
            .unwrap();

        let created = f.notifications.snapshot();
        assert_eq!(created.len(), 2);
        let recipient_ids: Vec<Uuid> = created.iter().map(|n| n.recipient_id).collect();
        assert!(recipient_ids.contains(&f.author.id));
        assert!(recipient_ids.contains(&f.assignee.id));
    }

    #[tokio::test]
    async fn the_author_commenting_notifies_only_the_assignee() {
        let f = fixture();

        f.use_case
            .execute(f.issue.id, f.author.id, "update".to_string())
            .await
            .unwrap();

        let created = f.notifications.snapshot();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].recipient_id, f.assignee.id);
    }

    #[tokio::test]
    async fn the_assignee_commenting_notifies_only_the_author() {
        let f = fixture();

        f.use_case
            .execute(f.issue.id, f.assignee.id, "on it".to_string())
            .await
            .unwrap();

        let created = f.notifications.snapshot();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].recipient_id, f.author.id);
    }

    #[tokio::test]
    async fn commenting_dispatches_exactly_one_webhook_even_when_it_notifies_two_people() {
        let f = fixture();

        f.use_case
            .execute(f.issue.id, f.commenter.id, "nice bug".to_string())
            .await
            .unwrap();

        let dispatched = f.webhooks.dispatched();
        assert_eq!(
            dispatched.len(),
            1,
            "one comment must dispatch exactly one webhook, regardless of how many people it notifies"
        );
        assert!(matches!(
            &dispatched[0].1,
            WebhookEvent::IssueCommented { .. }
        ));
    }
}
