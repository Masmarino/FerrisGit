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

use super::event_context::EventContext;

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

        if let Some(ctx) = EventContext::load(
            self.repositories.as_ref(),
            self.users.as_ref(),
            mr.repository_id,
            author_id,
        )
        .await
        {
            self.webhooks
                .dispatch(
                    mr.repository_id,
                    WebhookEvent::MergeRequestCommented {
                        repository_owner: ctx.owner_username.clone(),
                        repository_name: ctx.repository.name.clone(),
                        actor_username: ctx.actor_username.clone(),
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
                        merge_request_id: Some(mr.id),
                        merge_request_title: Some(mr.title),
                        ..ctx.notification(NotificationKind::MergeRequestCommented, mr_author_id)
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
    use crate::use_cases::fixtures::{
        added_line, merge_request, merge_request_comment, readme_diff, removed_line, repository,
        user,
    };
    use ferrisgit_domain::diff::FileDiff;
    use ferrisgit_domain::merge_request::MergeRequest;
    use ferrisgit_domain::user::User;
    use ferrisgit_domain::webhook_event::WebhookEvent;

    /// A merge request by `author`, in a repository owned by `owner`; `commenter` is a third user. Tests that do not
    /// care about notifications comment as an unknown user, which skips them.
    struct Fixture {
        use_case: AddMergeRequestCommentUseCase,
        store: Arc<FakeMergeRequests>,
        mr: MergeRequest,
        author: User,
        commenter: User,
        notifications: Arc<FakeNotifications>,
        webhooks: Arc<FakeWebhooks>,
    }

    fn fixture(diffs: Vec<FileDiff>) -> Fixture {
        let owner = user("owner");
        let author = user("author");
        let commenter = user("commenter");
        let repo = repository(owner.id);
        let mr = merge_request(repo.id, author.id);
        let store = Arc::new(FakeMergeRequests::new(vec![mr.clone()]));
        let notifications = Arc::new(FakeNotifications::empty());
        let webhooks = Arc::new(FakeWebhooks::default());
        let use_case = AddMergeRequestCommentUseCase::new(
            store.clone(),
            store.clone(),
            Arc::new(FakeRepositories::new(vec![repo])),
            Arc::new(FakeUsers::new(vec![
                owner,
                author.clone(),
                commenter.clone(),
            ])),
            notifications.clone(),
            Arc::new(FakeDiffReader::new(diffs)),
            webhooks.clone(),
        );
        Fixture {
            use_case,
            store,
            mr,
            author,
            commenter,
            notifications,
            webhooks,
        }
    }

    impl Fixture {
        /// Posts `body` as an unknown user, with no reply, anchor or suggestion.
        async fn comment(&self, body: &str) -> Result<MergeRequestComment, DomainError> {
            self.use_case
                .execute(
                    self.mr.id,
                    Uuid::new_v4(),
                    body.to_string(),
                    None,
                    None,
                    None,
                )
                .await
        }

        /// Posts `body` as an unknown user.
        async fn post(
            &self,
            body: &str,
            reply_to_id: Option<Uuid>,
            anchor: Option<PostedAnchor>,
            suggested_content: Option<&str>,
        ) -> Result<MergeRequestComment, DomainError> {
            self.use_case
                .execute(
                    self.mr.id,
                    Uuid::new_v4(),
                    body.to_string(),
                    reply_to_id,
                    anchor,
                    suggested_content.map(str::to_string),
                )
                .await
        }
    }

    fn readme_anchor(line_number: i32, end_line: Option<i32>, side: DiffSide) -> PostedAnchor {
        PostedAnchor {
            file_path: "README.md".to_string(),
            line_number,
            end_line,
            side,
        }
    }

    #[tokio::test]
    async fn adds_a_comment_to_an_existing_merge_request() {
        let f = fixture(vec![]);
        let author_id = Uuid::new_v4();

        let comment = f
            .use_case
            .execute(
                f.mr.id,
                author_id,
                "looks good".to_string(),
                None,
                None,
                None,
            )
            .await
            .unwrap();

        assert_eq!(comment.body, "looks good");
        assert_eq!(comment.author_id, Some(author_id));
        assert_eq!(f.store.list_comments(f.mr.id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn commenting_on_a_non_existent_request_is_a_not_found_error() {
        let f = fixture(vec![]);

        let result = f
            .use_case
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
        let f = fixture(vec![]);

        f.use_case
            .execute(
                f.mr.id,
                f.commenter.id,
                "looks good".to_string(),
                None,
                None,
                None,
            )
            .await
            .unwrap();

        let created = f.notifications.snapshot();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].recipient_id, f.author.id);
        assert_eq!(created[0].kind, NotificationKind::MergeRequestCommented);
    }

    #[tokio::test]
    async fn commenting_on_your_own_merge_request_does_not_notify_yourself() {
        let f = fixture(vec![]);

        f.use_case
            .execute(
                f.mr.id,
                f.author.id,
                "looks good".to_string(),
                None,
                None,
                None,
            )
            .await
            .unwrap();

        assert!(f.notifications.snapshot().is_empty());
    }

    #[tokio::test]
    async fn commenting_on_your_own_merge_request_still_dispatches_a_webhook_with_no_notification()
    {
        let f = fixture(vec![]);

        f.use_case
            .execute(
                f.mr.id,
                f.author.id,
                "looks good".to_string(),
                None,
                None,
                None,
            )
            .await
            .unwrap();

        assert!(
            f.notifications.snapshot().is_empty(),
            "commenting on your own MR must not notify yourself"
        );
        let dispatched = f.webhooks.dispatched();
        assert_eq!(
            dispatched.len(),
            1,
            "the webhook must still fire even with no one to notify"
        );
        assert_eq!(dispatched[0].0, f.mr.repository_id);
        assert!(matches!(
            &dispatched[0].1,
            WebhookEvent::MergeRequestCommented { .. }
        ));
    }

    #[tokio::test]
    async fn posting_a_valid_inline_comment_captures_the_lines_current_content_as_the_anchor() {
        let f = fixture(readme_diff(vec![added_line(2)]));

        let result = f
            .post(
                "why?",
                None,
                Some(readme_anchor(2, None, DiffSide::New)),
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
        let f = fixture(readme_diff(vec![added_line(2)]));

        let result = f
            .post(
                "why?",
                None,
                Some(readme_anchor(999, None, DiffSide::New)),
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
        let f = fixture(readme_diff(vec![added_line(2)]));
        let root = f
            .post(
                "why?",
                None,
                Some(readme_anchor(2, None, DiffSide::New)),
                None,
            )
            .await
            .unwrap();

        let reply = f
            .post(
                "good point",
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
        let f = fixture(readme_diff(vec![added_line(2)]));
        let root = f.comment("root").await.unwrap();
        let reply = f.post("reply", Some(root.id), None, None).await.unwrap();

        let result = f.post("reply to a reply", Some(reply.id), None, None).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn replying_to_a_comment_on_a_different_merge_request_is_rejected() {
        let f = fixture(vec![]);
        let foreign_root = merge_request_comment(Uuid::new_v4());
        f.store.seed_comment(foreign_root.clone());

        let result = f
            .post("sneaky reply", Some(foreign_root.id), None, None)
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn a_general_comment_with_no_anchor_or_reply_still_works_exactly_as_before() {
        let f = fixture(vec![]);

        let result = f.comment("hi").await.unwrap();

        assert!(result.file_path.is_none());
        assert!(result.reply_to_id.is_none());
    }

    #[tokio::test]
    async fn posting_a_multi_line_inline_comment_captures_the_full_ranges_content_as_the_anchor() {
        let f = fixture(readme_diff(vec![added_line(2), added_line(3)]));

        let result = f
            .post(
                "swap this block",
                None,
                Some(readme_anchor(2, Some(3), DiffSide::New)),
                Some("replacement\n"),
            )
            .await
            .unwrap();

        assert_eq!(result.end_line, Some(3));
        assert_eq!(result.anchor_content.as_deref(), Some("line 2\nline 3\n"));
        assert_eq!(result.suggested_content.as_deref(), Some("replacement\n"));
    }

    #[tokio::test]
    async fn posting_a_multi_line_comment_where_any_line_in_the_range_is_missing_is_rejected() {
        let f = fixture(readme_diff(vec![added_line(2)])); // only line 2 exists

        let result = f
            .post(
                "swap",
                None,
                Some(readme_anchor(2, Some(3), DiffSide::New)),
                None,
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn posting_a_suggestion_anchored_to_the_old_side_of_the_diff_is_rejected() {
        let f = fixture(readme_diff(vec![added_line(2)]));

        let result = f
            .post(
                "swap this",
                None,
                Some(readme_anchor(2, None, DiffSide::Old)),
                Some("replacement\n"),
            )
            .await;

        assert!(
            matches!(result, Err(DomainError::Validation(_))),
            "a suggestion can only ever be applied against the source branch's new-side numbering — an old-side suggestion must be rejected, got {result:?}"
        );
    }

    #[tokio::test]
    async fn posting_a_plain_old_side_comment_without_a_suggestion_still_works_exactly_as_before() {
        // A removed line only exists on the old side, so this checks that old-side comments (without a suggestion)
        // still work.
        let f = fixture(readme_diff(vec![removed_line(2)]));

        let result = f
            .post(
                "why was this removed?",
                None,
                Some(readme_anchor(2, None, DiffSide::Old)),
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
        let f = fixture(readme_diff(vec![added_line(2)]));
        let root = f
            .post(
                "why?",
                None,
                Some(readme_anchor(2, None, DiffSide::New)),
                None,
            )
            .await
            .unwrap();

        let reply = f
            .post("counter-suggestion", Some(root.id), None, Some("sneaky\n"))
            .await
            .unwrap();

        assert!(
            reply.suggested_content.is_none(),
            "a reply must never carry its own suggestion, even if the client sends one"
        );
    }
}
