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
        let issue = self
            .issues
            .find_by_id(issue_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("issue".to_string()))?;
        let comment = self
            .issue_comments
            .add_comment(NewIssueComment {
                issue_id,
                author_id,
                body,
            })
            .await?;

        if let Some(repo) = self
            .repositories
            .find_by_id(issue.repository_id)
            .await
            .ok()
            .flatten()
            && let Some(owner) = self.users.find_by_id(repo.owner_id).await.ok().flatten()
            && let Some(actor) = self.users.find_by_id(author_id).await.ok().flatten()
        {
            self.webhooks
                .dispatch(
                    issue.repository_id,
                    WebhookEvent::IssueCommented {
                        repository_owner: owner.username.clone(),
                        repository_name: repo.name.clone(),
                        actor_username: actor.username.clone(),
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
                        recipient_id,
                        kind: NotificationKind::IssueCommented,
                        repository_owner: owner.username.clone(),
                        repository_name: repo.name.clone(),
                        actor_username: Some(actor.username.clone()),
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

        Ok(comment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeIssues, FakeNotifications, FakeRepositories, FakeUsers, FakeWebhooks,
    };
    use ferrisgit_domain::issue::{Issue, IssueKind, IssueStatus};
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

    fn issue(repository_id: Uuid, author_id: Uuid, assignee_id: Option<Uuid>) -> Issue {
        Issue {
            id: Uuid::new_v4(),
            repository_id,
            number: 1,
            author_id,
            assignee_id,
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
    async fn a_third_party_commenting_notifies_both_author_and_assignee() {
        let owner = user("owner");
        let author = user("author");
        let assignee = user("assignee");
        let commenter = user("commenter");
        let repo = repository(owner.id);
        let seed_issue = issue(repo.id, author.id, Some(assignee.id));
        let notifications = Arc::new(FakeNotifications::empty());
        let issues = Arc::new(FakeIssues::new(vec![seed_issue.clone()]));
        let use_case = AddIssueCommentUseCase::new(
            issues.clone(),
            issues,
            Arc::new(FakeRepositories::new(vec![repo.clone()])),
            Arc::new(FakeUsers::new(vec![
                owner.clone(),
                author.clone(),
                assignee.clone(),
                commenter.clone(),
            ])),
            notifications.clone(),
            Arc::new(FakeWebhooks::default()),
        );

        use_case
            .execute(seed_issue.id, commenter.id, "nice bug".to_string())
            .await
            .unwrap();

        let created = notifications.snapshot();
        assert_eq!(created.len(), 2);
        let recipient_ids: Vec<Uuid> = created.iter().map(|n| n.recipient_id).collect();
        assert!(recipient_ids.contains(&author.id));
        assert!(recipient_ids.contains(&assignee.id));
    }

    #[tokio::test]
    async fn the_author_commenting_notifies_only_the_assignee() {
        let owner = user("owner");
        let author = user("author");
        let assignee = user("assignee");
        let repo = repository(owner.id);
        let seed_issue = issue(repo.id, author.id, Some(assignee.id));
        let notifications = Arc::new(FakeNotifications::empty());
        let issues = Arc::new(FakeIssues::new(vec![seed_issue.clone()]));
        let use_case = AddIssueCommentUseCase::new(
            issues.clone(),
            issues,
            Arc::new(FakeRepositories::new(vec![repo.clone()])),
            Arc::new(FakeUsers::new(vec![
                owner.clone(),
                author.clone(),
                assignee.clone(),
            ])),
            notifications.clone(),
            Arc::new(FakeWebhooks::default()),
        );

        use_case
            .execute(seed_issue.id, author.id, "update".to_string())
            .await
            .unwrap();

        let created = notifications.snapshot();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].recipient_id, assignee.id);
    }

    #[tokio::test]
    async fn the_assignee_commenting_notifies_only_the_author() {
        let owner = user("owner");
        let author = user("author");
        let assignee = user("assignee");
        let repo = repository(owner.id);
        let seed_issue = issue(repo.id, author.id, Some(assignee.id));
        let notifications = Arc::new(FakeNotifications::empty());
        let issues = Arc::new(FakeIssues::new(vec![seed_issue.clone()]));
        let use_case = AddIssueCommentUseCase::new(
            issues.clone(),
            issues,
            Arc::new(FakeRepositories::new(vec![repo.clone()])),
            Arc::new(FakeUsers::new(vec![
                owner.clone(),
                author.clone(),
                assignee.clone(),
            ])),
            notifications.clone(),
            Arc::new(FakeWebhooks::default()),
        );

        use_case
            .execute(seed_issue.id, assignee.id, "on it".to_string())
            .await
            .unwrap();

        let created = notifications.snapshot();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].recipient_id, author.id);
    }

    #[tokio::test]
    async fn commenting_dispatches_exactly_one_webhook_even_when_it_notifies_two_people() {
        let owner = user("owner");
        let author = user("author");
        let assignee = user("assignee");
        let commenter = user("commenter");
        let repo = repository(owner.id);
        let seed_issue = issue(repo.id, author.id, Some(assignee.id));
        let webhooks = Arc::new(FakeWebhooks::default());
        let issues = Arc::new(FakeIssues::new(vec![seed_issue.clone()]));
        let use_case = AddIssueCommentUseCase::new(
            issues.clone(),
            issues,
            Arc::new(FakeRepositories::new(vec![repo.clone()])),
            Arc::new(FakeUsers::new(vec![
                owner.clone(),
                author.clone(),
                assignee.clone(),
                commenter.clone(),
            ])),
            Arc::new(FakeNotifications::empty()),
            webhooks.clone(),
        );

        use_case
            .execute(seed_issue.id, commenter.id, "nice bug".to_string())
            .await
            .unwrap();

        let dispatched = webhooks.dispatched();
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
