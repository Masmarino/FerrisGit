use std::sync::Arc;

use ferrisgit_domain::diff::{DiffReaderPort, DiffSide, find_line, find_lines};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::merge_request::MergeRequestStorePort;
use ferrisgit_domain::merge_request_comment::{
    CommentAnchor, MergeRequestComment, MergeRequestCommentPort, NewMergeRequestComment,
};
use ferrisgit_domain::notification::{NewNotification, NotificationKind, NotificationStorePort};
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::user::UserRepositoryPort;
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use ferrisgit_domain::webhook_event::WebhookEvent;
use uuid::Uuid;

pub struct PostedAnchor {
    pub file_path: String,
    pub line_number: i32,
    pub end_line: Option<i32>,
    pub side: DiffSide,
}

pub struct AddMergeRequestCommentUseCase {
    merge_requests: Arc<dyn MergeRequestStorePort>,
    merge_request_comments: Arc<dyn MergeRequestCommentPort>,
    repositories: Arc<dyn RepositoryStorePort>,
    users: Arc<dyn UserRepositoryPort>,
    notifications: Arc<dyn NotificationStorePort>,
    diff_reader: Arc<dyn DiffReaderPort>,
    webhooks: Arc<dyn WebhookDispatcherPort>,
}

impl AddMergeRequestCommentUseCase {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        merge_requests: Arc<dyn MergeRequestStorePort>,
        merge_request_comments: Arc<dyn MergeRequestCommentPort>,
        repositories: Arc<dyn RepositoryStorePort>,
        users: Arc<dyn UserRepositoryPort>,
        notifications: Arc<dyn NotificationStorePort>,
        diff_reader: Arc<dyn DiffReaderPort>,
        webhooks: Arc<dyn WebhookDispatcherPort>,
    ) -> Self {
        Self {
            merge_requests,
            merge_request_comments,
            repositories,
            users,
            notifications,
            diff_reader,
            webhooks,
        }
    }

    pub async fn execute(
        &self,
        merge_request_id: Uuid,
        author_id: Uuid,
        body: String,
        reply_to_id: Option<Uuid>,
        anchor: Option<PostedAnchor>,
        suggested_content: Option<String>,
    ) -> Result<MergeRequestComment, DomainError> {
        let Some(mr) = self.merge_requests.find_by_id(merge_request_id).await? else {
            return Err(DomainError::NotFound("merge request".to_string()));
        };

        // A reply's own suggested content is never trusted, same as its own anchor below.
        let suggested_content = if reply_to_id.is_some() {
            None
        } else {
            suggested_content
        };

        let final_anchor = match reply_to_id {
            Some(root_id) => {
                let existing = self
                    .merge_request_comments
                    .list_comments(merge_request_id)
                    .await?;
                let root = existing.iter().find(|c| c.id == root_id).ok_or_else(|| {
                    DomainError::Validation(
                        "can only reply to a thread's root comment on this merge request"
                            .to_string(),
                    )
                })?;
                if root.reply_to_id.is_some() {
                    return Err(DomainError::Validation(
                        "can only reply to a thread's root comment on this merge request"
                            .to_string(),
                    ));
                }
                match (
                    &root.file_path,
                    root.line_number,
                    root.side,
                    &root.anchor_content,
                ) {
                    (Some(path), Some(line), Some(side), Some(content)) => Some(CommentAnchor {
                        file_path: path.clone(),
                        line_number: line,
                        end_line: root.end_line,
                        side,
                        anchor_content: content.clone(),
                    }),
                    _ => None,
                }
            }
            None => match anchor {
                Some(posted) => {
                    // A suggestion is spliced into the source tip by new-side line numbers. An old-side number points
                    // into another version of the file and would splice the wrong lines, so reject it outright.
                    if suggested_content.is_some() && posted.side == DiffSide::Old {
                        return Err(DomainError::Validation(
                            "a suggestion can only be anchored to the new side of the diff"
                                .to_string(),
                        ));
                    }
                    let repo = self
                        .repositories
                        .find_by_id(mr.repository_id)
                        .await?
                        .ok_or_else(|| DomainError::NotFound("repository".to_string()))?;
                    let diffs = self
                        .diff_reader
                        .diff_branches(&repo.disk_path, &mr.source_branch, &mr.target_branch)
                        .await?;
                    let content = match posted.end_line {
                        Some(end_line) => find_lines(
                            &diffs,
                            &posted.file_path,
                            posted.line_number,
                            end_line,
                            posted.side,
                        ),
                        None => {
                            find_line(&diffs, &posted.file_path, posted.line_number, posted.side)
                                .map(str::to_string)
                        }
                    }
                    .ok_or_else(|| {
                        DomainError::Validation(
                            "that line is no longer part of the diff — reload and try again"
                                .to_string(),
                        )
                    })?;
                    Some(CommentAnchor {
                        file_path: posted.file_path,
                        line_number: posted.line_number,
                        end_line: posted.end_line,
                        side: posted.side,
                        anchor_content: content,
                    })
                }
                None => None,
            },
        };

        let comment = self
            .merge_request_comments
            .add_comment(NewMergeRequestComment {
                merge_request_id,
                author_id,
                body,
                reply_to_id,
                anchor: final_anchor,
                suggested_content,
            })
            .await?;

        if let Some(repo) = self
            .repositories
            .find_by_id(mr.repository_id)
            .await
            .ok()
            .flatten()
            && let Some(owner) = self.users.find_by_id(repo.owner_id).await.ok().flatten()
            && let Some(actor) = self.users.find_by_id(author_id).await.ok().flatten()
        {
            self.webhooks
                .dispatch(
                    mr.repository_id,
                    WebhookEvent::MergeRequestCommented {
                        repository_owner: owner.username.clone(),
                        repository_name: repo.name.clone(),
                        actor_username: actor.username.clone(),
                        merge_request_id: mr.id,
                        merge_request_title: mr.title.clone(),
                    },
                )
                .await
                .ok();

            // Nobody to notify once the author's account is gone.
            if let Some(mr_author_id) = mr.author_id
                && mr_author_id != author_id
            {
                self.notifications
                    .create(NewNotification {
                        recipient_id: mr_author_id,
                        kind: NotificationKind::MergeRequestCommented,
                        repository_owner: owner.username,
                        repository_name: repo.name,
                        actor_username: Some(actor.username),
                        merge_request_id: Some(mr.id),
                        merge_request_title: Some(mr.title),
                        pipeline_id: None,
                        commit_sha: None,
                        role: None,
                        issue_id: None,
                        issue_number: None,
                        issue_title: None,
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
        FakeDiffReader, FakeMergeRequests, FakeNotifications, FakeRepositories, FakeUsers,
        FakeWebhooks,
    };
    use chrono::Utc;
    use ferrisgit_domain::diff::{DiffLine, DiffLineKind, FileChangeKind, FileDiff, Hunk};
    use ferrisgit_domain::merge_request::{MergeRequest, MergeRequestStatus};
    use ferrisgit_domain::repository::Repository;
    use ferrisgit_domain::user::User;
    use ferrisgit_domain::webhook_event::WebhookEvent;

    fn sample_diffs() -> Vec<FileDiff> {
        vec![FileDiff {
            path: "README.md".to_string(),
            change: FileChangeKind::Modified,
            hunks: vec![Hunk {
                lines: vec![DiffLine {
                    kind: DiffLineKind::Added,
                    content: "line 2\n".to_string(),
                    old_line: None,
                    new_line: Some(2),
                }],
            }],
        }]
    }

    fn sample_diffs_with_two_added_lines() -> Vec<FileDiff> {
        vec![FileDiff {
            path: "README.md".to_string(),
            change: FileChangeKind::Modified,
            hunks: vec![Hunk {
                lines: vec![
                    DiffLine {
                        kind: DiffLineKind::Added,
                        content: "line 2\n".to_string(),
                        old_line: None,
                        new_line: Some(2),
                    },
                    DiffLine {
                        kind: DiffLineKind::Added,
                        content: "line 3\n".to_string(),
                        old_line: None,
                        new_line: Some(3),
                    },
                ],
            }],
        }]
    }

    // Shared placeholder id so anchor resolution's repository lookup succeeds in tests that don't care which
    // repository.
    fn unused_repository_id() -> Uuid {
        Uuid::nil()
    }

    fn seeded_store(mr_id: Uuid) -> Arc<FakeMergeRequests> {
        seeded_store_with_author(mr_id, Uuid::new_v4()).0
    }

    fn seeded_store_with_author(
        mr_id: Uuid,
        mr_author_id: Uuid,
    ) -> (Arc<FakeMergeRequests>, MergeRequest) {
        let mr = MergeRequest {
            id: mr_id,
            repository_id: unused_repository_id(),
            author_id: Some(mr_author_id),
            source_branch: "feature".to_string(),
            target_branch: "main".to_string(),
            title: "t".to_string(),
            description: String::new(),
            status: MergeRequestStatus::Open,
            merge_commit_sha: None,
            milestone_id: None,
            created_at: Utc::now(),
            closed_at: None,
        };
        (Arc::new(FakeMergeRequests::new(vec![mr.clone()])), mr)
    }

    fn unused_dependencies() -> (
        Arc<FakeRepositories>,
        Arc<FakeUsers>,
        Arc<FakeNotifications>,
    ) {
        (
            Arc::new(FakeRepositories::new(vec![Repository {
                id: unused_repository_id(),
                owner_id: Uuid::new_v4(),
                name: "hello".to_string(),
                group_id: None,
                description: String::new(),
                disk_path: "hello.git".to_string(),
                visibility: ferrisgit_domain::repository::RepositoryVisibility::Private,
                created_at: Utc::now(),
            }])),
            Arc::new(FakeUsers::new(vec![])),
            Arc::new(FakeNotifications::empty()),
        )
    }

    #[tokio::test]
    async fn adds_a_comment_to_an_existing_merge_request() {
        let mr_id = Uuid::new_v4();
        let store = seeded_store(mr_id);
        let author_id = Uuid::new_v4();
        let (repositories, users, notifications) = unused_dependencies();
        let diff_reader: Arc<dyn DiffReaderPort> = Arc::new(FakeDiffReader::new(vec![]));
        let use_case = AddMergeRequestCommentUseCase::new(
            store.clone(),
            store.clone(),
            repositories,
            users,
            notifications,
            diff_reader,
            Arc::new(FakeWebhooks::default()),
        );

        let comment = use_case
            .execute(mr_id, author_id, "looks good".to_string(), None, None, None)
            .await
            .unwrap();

        assert_eq!(comment.body, "looks good");
        assert_eq!(comment.author_id, Some(author_id));
        assert_eq!(store.list_comments(mr_id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn commenting_on_a_non_existent_request_is_a_not_found_error() {
        let (repositories, users, notifications) = unused_dependencies();
        let diff_reader: Arc<dyn DiffReaderPort> = Arc::new(FakeDiffReader::new(vec![]));
        let store = Arc::new(FakeMergeRequests::empty());
        let use_case = AddMergeRequestCommentUseCase::new(
            store.clone(),
            store,
            repositories,
            users,
            notifications,
            diff_reader,
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(
                Uuid::new_v4(),
                Uuid::new_v4(),
                "hi".to_string(),
                None,
                None,
                None,
            )
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn commenting_on_someone_elses_merge_request_notifies_its_author() {
        let mr_id = Uuid::new_v4();
        let author_id = Uuid::new_v4();
        let commenter_id = Uuid::new_v4();
        let (store, mr) = seeded_store_with_author(mr_id, author_id);
        let owner_id = Uuid::new_v4();
        let repositories = Arc::new(FakeRepositories::new(vec![Repository {
            id: mr.repository_id,
            owner_id,
            name: "hello".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "hello.git".to_string(),
            visibility: ferrisgit_domain::repository::RepositoryVisibility::Private,
            created_at: Utc::now(),
        }]));
        let users = Arc::new(FakeUsers::new(vec![
            User {
                id: owner_id,
                username: "owner".to_string(),
                email: "o@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
                created_at: Utc::now(),
            },
            User {
                id: commenter_id,
                username: "commenter".to_string(),
                email: "c@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
                created_at: Utc::now(),
            },
        ]));
        let notifications = Arc::new(FakeNotifications::empty());
        let diff_reader: Arc<dyn DiffReaderPort> = Arc::new(FakeDiffReader::new(vec![]));
        let use_case = AddMergeRequestCommentUseCase::new(
            store.clone(),
            store,
            repositories,
            users,
            notifications.clone(),
            diff_reader,
            Arc::new(FakeWebhooks::default()),
        );

        use_case
            .execute(
                mr_id,
                commenter_id,
                "looks good".to_string(),
                None,
                None,
                None,
            )
            .await
            .unwrap();

        let created = notifications.snapshot();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].recipient_id, author_id);
        assert_eq!(created[0].kind, NotificationKind::MergeRequestCommented);
    }

    #[tokio::test]
    async fn commenting_on_your_own_merge_request_does_not_notify_yourself() {
        let mr_id = Uuid::new_v4();
        let author_id = Uuid::new_v4();
        let (store, mr) = seeded_store_with_author(mr_id, author_id);
        let owner_id = Uuid::new_v4();
        let repositories = Arc::new(FakeRepositories::new(vec![Repository {
            id: mr.repository_id,
            owner_id,
            name: "hello".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "hello.git".to_string(),
            visibility: ferrisgit_domain::repository::RepositoryVisibility::Private,
            created_at: Utc::now(),
        }]));
        let users = Arc::new(FakeUsers::new(vec![
            User {
                id: owner_id,
                username: "owner".to_string(),
                email: "o@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
                created_at: Utc::now(),
            },
            User {
                id: author_id,
                username: "author".to_string(),
                email: "a@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
                created_at: Utc::now(),
            },
        ]));
        let notifications = Arc::new(FakeNotifications::empty());
        let diff_reader: Arc<dyn DiffReaderPort> = Arc::new(FakeDiffReader::new(vec![]));
        let use_case = AddMergeRequestCommentUseCase::new(
            store.clone(),
            store,
            repositories,
            users,
            notifications.clone(),
            diff_reader,
            Arc::new(FakeWebhooks::default()),
        );

        use_case
            .execute(mr_id, author_id, "looks good".to_string(), None, None, None)
            .await
            .unwrap();

        assert!(notifications.snapshot().is_empty());
    }

    #[tokio::test]
    async fn commenting_on_your_own_merge_request_still_dispatches_a_webhook_with_no_notification()
    {
        let mr_id = Uuid::new_v4();
        let author_id = Uuid::new_v4();
        let (store, mr) = seeded_store_with_author(mr_id, author_id);
        let owner_id = Uuid::new_v4();
        let repositories = Arc::new(FakeRepositories::new(vec![Repository {
            id: mr.repository_id,
            owner_id,
            name: "hello".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "hello.git".to_string(),
            visibility: ferrisgit_domain::repository::RepositoryVisibility::Private,
            created_at: Utc::now(),
        }]));
        let users = Arc::new(FakeUsers::new(vec![
            User {
                id: owner_id,
                username: "owner".to_string(),
                email: "o@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
                created_at: Utc::now(),
            },
            User {
                id: author_id,
                username: "author".to_string(),
                email: "a@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
                created_at: Utc::now(),
            },
        ]));
        let notifications = Arc::new(FakeNotifications::empty());
        let webhooks = Arc::new(FakeWebhooks::default());
        let diff_reader: Arc<dyn DiffReaderPort> = Arc::new(FakeDiffReader::new(vec![]));
        let use_case = AddMergeRequestCommentUseCase::new(
            store.clone(),
            store,
            repositories,
            users,
            notifications.clone(),
            diff_reader,
            webhooks.clone(),
        );

        use_case
            .execute(mr_id, author_id, "looks good".to_string(), None, None, None)
            .await
            .unwrap();

        assert!(
            notifications.snapshot().is_empty(),
            "commenting on your own MR must not notify yourself"
        );
        let dispatched = webhooks.dispatched();
        assert_eq!(
            dispatched.len(),
            1,
            "the webhook must still fire even with no one to notify"
        );
        assert_eq!(dispatched[0].0, mr.repository_id);
        assert!(matches!(
            &dispatched[0].1,
            WebhookEvent::MergeRequestCommented { .. }
        ));
    }

    #[tokio::test]
    async fn posting_a_valid_inline_comment_captures_the_lines_current_content_as_the_anchor() {
        let mr_id = Uuid::new_v4();
        let store = seeded_store(mr_id);
        let (repositories, users, notifications) = unused_dependencies();
        let diff_reader: Arc<dyn DiffReaderPort> = Arc::new(FakeDiffReader::new(sample_diffs()));
        let use_case = AddMergeRequestCommentUseCase::new(
            store.clone(),
            store,
            repositories,
            users,
            notifications,
            diff_reader,
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(
                mr_id,
                Uuid::new_v4(),
                "why?".to_string(),
                None,
                Some(PostedAnchor {
                    file_path: "README.md".to_string(),
                    line_number: 2,
                    end_line: None,
                    side: DiffSide::New,
                }),
                None,
            )
            .await
            .unwrap();

        assert_eq!(result.file_path.as_deref(), Some("README.md"));
        assert_eq!(
            result.anchor_content.as_deref(),
            Some("line 2\n"),
            "the server must capture the live line content itself, never trust a client-supplied value"
        );
    }

    #[tokio::test]
    async fn posting_an_inline_comment_on_a_line_absent_from_the_current_diff_is_rejected() {
        let mr_id = Uuid::new_v4();
        let store = seeded_store(mr_id);
        let (repositories, users, notifications) = unused_dependencies();
        let diff_reader: Arc<dyn DiffReaderPort> = Arc::new(FakeDiffReader::new(sample_diffs()));
        let use_case = AddMergeRequestCommentUseCase::new(
            store.clone(),
            store,
            repositories,
            users,
            notifications,
            diff_reader,
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(
                mr_id,
                Uuid::new_v4(),
                "why?".to_string(),
                None,
                Some(PostedAnchor {
                    file_path: "README.md".to_string(),
                    line_number: 999,
                    end_line: None,
                    side: DiffSide::New,
                }),
                None,
            )
            .await;

        assert!(
            matches!(result, Err(DomainError::Validation(_))),
            "a client viewing a stale diff must not be able to silently plant a wrong anchor, got {result:?}"
        );
    }

    #[tokio::test]
    async fn a_reply_inherits_its_roots_anchor_even_if_the_client_sends_a_different_one() {
        let mr_id = Uuid::new_v4();
        let store = seeded_store(mr_id);
        let (repositories, users, notifications) = unused_dependencies();
        let diff_reader: Arc<dyn DiffReaderPort> = Arc::new(FakeDiffReader::new(sample_diffs()));
        let use_case = AddMergeRequestCommentUseCase::new(
            store.clone(),
            store.clone(),
            repositories,
            users,
            notifications,
            diff_reader,
            Arc::new(FakeWebhooks::default()),
        );

        let root = use_case
            .execute(
                mr_id,
                Uuid::new_v4(),
                "why?".to_string(),
                None,
                Some(PostedAnchor {
                    file_path: "README.md".to_string(),
                    line_number: 2,
                    end_line: None,
                    side: DiffSide::New,
                }),
                None,
            )
            .await
            .unwrap();

        let reply = use_case
            .execute(
                mr_id,
                Uuid::new_v4(),
                "good point".to_string(),
                Some(root.id),
                Some(PostedAnchor {
                    file_path: "OTHER.md".to_string(),
                    line_number: 1,
                    end_line: None,
                    side: DiffSide::Old,
                }),
                None,
            )
            .await
            .unwrap();

        assert_eq!(reply.reply_to_id, Some(root.id));
        assert_eq!(
            reply.file_path.as_deref(),
            Some("README.md"),
            "a reply must inherit its root's anchor, never the client's own"
        );
        assert_eq!(reply.line_number, Some(2));
    }

    #[tokio::test]
    async fn replying_to_a_reply_instead_of_a_thread_root_is_rejected() {
        let mr_id = Uuid::new_v4();
        let store = seeded_store(mr_id);
        let (repositories, users, notifications) = unused_dependencies();
        let diff_reader: Arc<dyn DiffReaderPort> = Arc::new(FakeDiffReader::new(sample_diffs()));
        let use_case = AddMergeRequestCommentUseCase::new(
            store.clone(),
            store.clone(),
            repositories,
            users,
            notifications,
            diff_reader,
            Arc::new(FakeWebhooks::default()),
        );

        let root = use_case
            .execute(mr_id, Uuid::new_v4(), "root".to_string(), None, None, None)
            .await
            .unwrap();
        let reply = use_case
            .execute(
                mr_id,
                Uuid::new_v4(),
                "reply".to_string(),
                Some(root.id),
                None,
                None,
            )
            .await
            .unwrap();

        let result = use_case
            .execute(
                mr_id,
                Uuid::new_v4(),
                "reply to a reply".to_string(),
                Some(reply.id),
                None,
                None,
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn replying_to_a_comment_on_a_different_merge_request_is_rejected() {
        let mr_id = Uuid::new_v4();
        let other_mr_id = Uuid::new_v4();
        let store = seeded_store(mr_id);
        let foreign_root = MergeRequestComment {
            id: Uuid::new_v4(),
            merge_request_id: other_mr_id,
            author_id: Some(Uuid::new_v4()),
            body: "root on another MR".to_string(),
            created_at: Utc::now(),
            reply_to_id: None,
            file_path: None,
            line_number: None,
            side: None,
            anchor_content: None,
            resolved: false,
            end_line: None,
            suggested_content: None,
            applied_at: None,
            applied_commit_sha: None,
        };
        store.seed_comment(foreign_root.clone());
        let (repositories, users, notifications) = unused_dependencies();
        let diff_reader: Arc<dyn DiffReaderPort> = Arc::new(FakeDiffReader::new(vec![]));
        let use_case = AddMergeRequestCommentUseCase::new(
            store.clone(),
            store,
            repositories,
            users,
            notifications,
            diff_reader,
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(
                mr_id,
                Uuid::new_v4(),
                "sneaky reply".to_string(),
                Some(foreign_root.id),
                None,
                None,
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn a_general_comment_with_no_anchor_or_reply_still_works_exactly_as_before() {
        let mr_id = Uuid::new_v4();
        let store = seeded_store(mr_id);
        let (repositories, users, notifications) = unused_dependencies();
        let diff_reader: Arc<dyn DiffReaderPort> = Arc::new(FakeDiffReader::new(vec![]));
        let use_case = AddMergeRequestCommentUseCase::new(
            store.clone(),
            store,
            repositories,
            users,
            notifications,
            diff_reader,
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(mr_id, Uuid::new_v4(), "hi".to_string(), None, None, None)
            .await
            .unwrap();

        assert!(result.file_path.is_none());
        assert!(result.reply_to_id.is_none());
    }

    #[tokio::test]
    async fn posting_a_multi_line_inline_comment_captures_the_full_ranges_content_as_the_anchor() {
        let mr_id = Uuid::new_v4();
        let store = seeded_store(mr_id);
        let (repositories, users, notifications) = unused_dependencies();
        let diff_reader: Arc<dyn DiffReaderPort> =
            Arc::new(FakeDiffReader::new(sample_diffs_with_two_added_lines()));
        let use_case = AddMergeRequestCommentUseCase::new(
            store.clone(),
            store,
            repositories,
            users,
            notifications,
            diff_reader,
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(
                mr_id,
                Uuid::new_v4(),
                "swap this block".to_string(),
                None,
                Some(PostedAnchor {
                    file_path: "README.md".to_string(),
                    line_number: 2,
                    end_line: Some(3),
                    side: DiffSide::New,
                }),
                Some("replacement\n".to_string()),
            )
            .await
            .unwrap();

        assert_eq!(result.end_line, Some(3));
        assert_eq!(result.anchor_content.as_deref(), Some("line 2\nline 3\n"));
        assert_eq!(result.suggested_content.as_deref(), Some("replacement\n"));
    }

    #[tokio::test]
    async fn posting_a_multi_line_comment_where_any_line_in_the_range_is_missing_is_rejected() {
        let mr_id = Uuid::new_v4();
        let store = seeded_store(mr_id);
        let (repositories, users, notifications) = unused_dependencies();
        let diff_reader: Arc<dyn DiffReaderPort> = Arc::new(FakeDiffReader::new(sample_diffs())); // only line 2 exists
        let use_case = AddMergeRequestCommentUseCase::new(
            store.clone(),
            store,
            repositories,
            users,
            notifications,
            diff_reader,
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(
                mr_id,
                Uuid::new_v4(),
                "swap".to_string(),
                None,
                Some(PostedAnchor {
                    file_path: "README.md".to_string(),
                    line_number: 2,
                    end_line: Some(3),
                    side: DiffSide::New,
                }),
                None,
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn posting_a_suggestion_anchored_to_the_old_side_of_the_diff_is_rejected() {
        let mr_id = Uuid::new_v4();
        let store = seeded_store(mr_id);
        let (repositories, users, notifications) = unused_dependencies();
        let diff_reader: Arc<dyn DiffReaderPort> = Arc::new(FakeDiffReader::new(sample_diffs()));
        let use_case = AddMergeRequestCommentUseCase::new(
            store.clone(),
            store,
            repositories,
            users,
            notifications,
            diff_reader,
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(
                mr_id,
                Uuid::new_v4(),
                "swap this".to_string(),
                None,
                Some(PostedAnchor {
                    file_path: "README.md".to_string(),
                    line_number: 2,
                    end_line: None,
                    side: DiffSide::Old,
                }),
                Some("replacement\n".to_string()),
            )
            .await;

        assert!(
            matches!(result, Err(DomainError::Validation(_))),
            "a suggestion can only ever be applied against the source branch's new-side numbering — an old-side suggestion must be rejected, got {result:?}"
        );
    }

    #[tokio::test]
    async fn posting_a_plain_old_side_comment_without_a_suggestion_still_works_exactly_as_before() {
        let mr_id = Uuid::new_v4();
        let store = seeded_store(mr_id);
        let (repositories, users, notifications) = unused_dependencies();
        // A removed line only exists on the old side, so this checks that old-side comments (without a suggestion)
        // still work.
        let diffs = vec![FileDiff {
            path: "README.md".to_string(),
            change: FileChangeKind::Modified,
            hunks: vec![Hunk {
                lines: vec![DiffLine {
                    kind: DiffLineKind::Removed,
                    content: "line 2\n".to_string(),
                    old_line: Some(2),
                    new_line: None,
                }],
            }],
        }];
        let diff_reader: Arc<dyn DiffReaderPort> = Arc::new(FakeDiffReader::new(diffs));
        let use_case = AddMergeRequestCommentUseCase::new(
            store.clone(),
            store,
            repositories,
            users,
            notifications,
            diff_reader,
            Arc::new(FakeWebhooks::default()),
        );

        let result = use_case
            .execute(
                mr_id,
                Uuid::new_v4(),
                "why was this removed?".to_string(),
                None,
                Some(PostedAnchor {
                    file_path: "README.md".to_string(),
                    line_number: 2,
                    end_line: None,
                    side: DiffSide::Old,
                }),
                None,
            )
            .await
            .unwrap();

        assert_eq!(result.file_path.as_deref(), Some("README.md"));
        assert_eq!(result.side, Some(DiffSide::Old));
        assert!(result.suggested_content.is_none());
    }

    #[tokio::test]
    async fn a_reply_never_carries_its_own_suggested_content_even_if_the_client_sends_one() {
        let mr_id = Uuid::new_v4();
        let store = seeded_store(mr_id);
        let (repositories, users, notifications) = unused_dependencies();
        let diff_reader: Arc<dyn DiffReaderPort> = Arc::new(FakeDiffReader::new(sample_diffs()));
        let use_case = AddMergeRequestCommentUseCase::new(
            store.clone(),
            store.clone(),
            repositories,
            users,
            notifications,
            diff_reader,
            Arc::new(FakeWebhooks::default()),
        );

        let root = use_case
            .execute(
                mr_id,
                Uuid::new_v4(),
                "why?".to_string(),
                None,
                Some(PostedAnchor {
                    file_path: "README.md".to_string(),
                    line_number: 2,
                    end_line: None,
                    side: DiffSide::New,
                }),
                None,
            )
            .await
            .unwrap();

        let reply = use_case
            .execute(
                mr_id,
                Uuid::new_v4(),
                "counter-suggestion".to_string(),
                Some(root.id),
                None,
                Some("sneaky\n".to_string()),
            )
            .await
            .unwrap();

        assert!(
            reply.suggested_content.is_none(),
            "a reply must never carry its own suggestion, even if the client sends one"
        );
    }
}
