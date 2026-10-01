use std::sync::Arc;

use ferrisgit_domain::apply_suggestion_executor::ApplySuggestionExecutorPort;
use ferrisgit_domain::branch::BranchReaderPort;
use ferrisgit_domain::diff::{DiffReaderPort, DiffSide, resolve_anchor_content};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::merge_request::{MergeRequestStatus, MergeRequestStorePort};
use ferrisgit_domain::merge_request_comment::{MergeRequestComment, MergeRequestCommentPort};
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::user::UserRepositoryPort;
use uuid::Uuid;

pub struct ApplySuggestionCommentUseCase {
    merge_requests: Arc<dyn MergeRequestStorePort>,
    merge_request_comments: Arc<dyn MergeRequestCommentPort>,
    repositories: Arc<dyn RepositoryStorePort>,
    users: Arc<dyn UserRepositoryPort>,
    diff_reader: Arc<dyn DiffReaderPort>,
    branch_reader: Arc<dyn BranchReaderPort>,
    suggestion_executor: Arc<dyn ApplySuggestionExecutorPort>,
}

impl ApplySuggestionCommentUseCase {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        merge_requests: Arc<dyn MergeRequestStorePort>,
        merge_request_comments: Arc<dyn MergeRequestCommentPort>,
        repositories: Arc<dyn RepositoryStorePort>,
        users: Arc<dyn UserRepositoryPort>,
        diff_reader: Arc<dyn DiffReaderPort>,
        branch_reader: Arc<dyn BranchReaderPort>,
        suggestion_executor: Arc<dyn ApplySuggestionExecutorPort>,
    ) -> Self {
        Self {
            merge_requests,
            merge_request_comments,
            repositories,
            users,
            diff_reader,
            branch_reader,
            suggestion_executor,
        }
    }

    pub async fn execute(
        &self,
        merge_request_id: Uuid,
        comment_id: Uuid,
        applier_id: Uuid,
    ) -> Result<MergeRequestComment, DomainError> {
        let mr = self
            .merge_requests
            .find_by_id(merge_request_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("merge request".to_string()))?;
        if mr.status != MergeRequestStatus::Open {
            return Err(DomainError::Validation(
                "merge request is not open".to_string(),
            ));
        }

        let comments = self
            .merge_request_comments
            .list_comments(merge_request_id)
            .await?;
        let comment = comments
            .iter()
            .find(|c| c.id == comment_id)
            .ok_or_else(|| DomainError::NotFound("comment".to_string()))?;
        if comment.reply_to_id.is_some() {
            return Err(DomainError::Validation(
                "only a thread's root comment can carry a suggestion".to_string(),
            ));
        }
        let Some(suggested_content) = comment.suggested_content.clone() else {
            return Err(DomainError::Validation(
                "this comment has no suggestion to apply".to_string(),
            ));
        };
        if comment.applied_at.is_some() {
            return Err(DomainError::Validation(
                "this suggestion has already been applied".to_string(),
            ));
        }
        let (Some(file_path), Some(start_line), Some(side)) =
            (comment.file_path.clone(), comment.line_number, comment.side)
        else {
            return Err(DomainError::Validation(
                "this comment has no anchor to apply a suggestion against".to_string(),
            ));
        };
        // Last line of defense before a git write: suggestions are spliced with new-side line numbers, so an old-side
        // one would silently corrupt the wrong lines.
        if side == DiffSide::Old {
            return Err(DomainError::Validation(
                "a suggestion can only be anchored to the new side of the diff".to_string(),
            ));
        }
        let end_line = comment.end_line.unwrap_or(start_line);

        let repo = self
            .repositories
            .find_by_id(mr.repository_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("repository".to_string()))?;

        let applier = self
            .users
            .find_by_id(applier_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("user".to_string()))?;

        // Resolve `tip_sha` before the diff re-read: it is `expected_tip` for the git compare-and-swap. If a push
        // lands between the two reads, the diff may be newer while `tip_sha` is older, so the CAS fails cleanly with
        // `Conflict`. The reverse order would validate against stale content while the CAS succeeds on the newer tip,
        // silently writing wrong content. Do not reorder.
        let branches = self.branch_reader.list_branches(&repo.disk_path).await?;
        let tip_sha = branches
            .into_iter()
            .find(|b| b.name == mr.source_branch)
            .map(|b| b.tip_sha)
            .ok_or_else(|| DomainError::Validation("source branch no longer exists".to_string()))?;

        let diffs = self
            .diff_reader
            .diff_branches(&repo.disk_path, &mr.source_branch, &mr.target_branch)
            .await?;
        let current_content =
            resolve_anchor_content(&diffs, &file_path, start_line, comment.end_line, side);
        if current_content.as_deref() != comment.anchor_content.as_deref() {
            return Err(DomainError::Validation(
                "this suggestion is outdated and can no longer be applied — reload and try again"
                    .to_string(),
            ));
        }

        let message = format!("Apply suggestion from @{}", applier.username);
        let commit_sha = self
            .suggestion_executor
            .apply_replacement(
                &repo.disk_path,
                &mr.source_branch,
                &tip_sha,
                &file_path,
                start_line,
                end_line,
                &suggested_content,
                &message,
                &applier.username,
                &applier.email,
            )
            .await?;

        self.merge_request_comments
            .mark_comment_applied(comment_id, &commit_sha)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeBranchReader, FakeDiffReader, FakeMergeRequests, FakeRepositories, FakeUsers,
    };
    use async_trait::async_trait;
    use chrono::Utc;
    use ferrisgit_domain::branch::BranchInfo;
    use ferrisgit_domain::diff::{
        DiffLine, DiffLineKind, DiffSide, FileChangeKind, FileDiff, Hunk,
    };
    use ferrisgit_domain::merge_request::MergeRequest;
    use ferrisgit_domain::repository::{Repository, RepositoryVisibility};
    use ferrisgit_domain::user::User;

    struct FakeSuggestionExecutor {
        result: Result<String, DomainError>,
    }
    #[async_trait]
    impl ApplySuggestionExecutorPort for FakeSuggestionExecutor {
        async fn apply_replacement(
            &self,
            _repository_disk_path: &str,
            _branch: &str,
            _expected_tip: &str,
            _file_path: &str,
            _start_line: i32,
            _end_line: i32,
            _new_content: &str,
            _message: &str,
            _committer_name: &str,
            _committer_email: &str,
        ) -> Result<String, DomainError> {
            match &self.result {
                Ok(sha) => Ok(sha.clone()),
                Err(DomainError::Conflict(msg)) => Err(DomainError::Conflict(msg.clone())),
                Err(_) => unreachable!("tests only construct Ok or Conflict results"),
            }
        }
    }

    fn unused_repository_id() -> Uuid {
        Uuid::nil()
    }

    fn suggestion_comment(
        merge_request_id: Uuid,
        reply_to_id: Option<Uuid>,
        end_line: Option<i32>,
        suggested_content: Option<String>,
        applied: bool,
    ) -> MergeRequestComment {
        MergeRequestComment {
            id: Uuid::new_v4(),
            merge_request_id,
            author_id: Some(Uuid::new_v4()),
            body: "swap this".to_string(),
            created_at: Utc::now(),
            reply_to_id,
            file_path: Some("README.md".to_string()),
            line_number: Some(2),
            side: Some(DiffSide::New),
            anchor_content: Some("line 2\n".to_string()),
            resolved: false,
            end_line,
            suggested_content,
            applied_at: if applied { Some(Utc::now()) } else { None },
            applied_commit_sha: if applied {
                Some("already-applied-sha".to_string())
            } else {
                None
            },
        }
    }

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

    fn florian() -> User {
        User {
            id: Uuid::new_v4(),
            username: "florian".to_string(),
            email: "florian@example.com".to_string(),
            password_hash: "h".to_string(),
            is_admin: false,
            created_at: Utc::now(),
        }
    }

    /// `mr_id` is generated by the caller before building comment fixtures: every comment's `merge_request_id` must
    /// match it.
    fn setup(
        mr_id: Uuid,
        mr_status: MergeRequestStatus,
        comments: Vec<MergeRequestComment>,
        applier: User,
        diffs: Vec<FileDiff>,
        executor_result: Result<String, DomainError>,
    ) -> (ApplySuggestionCommentUseCase, Uuid) {
        let mr = MergeRequest {
            id: mr_id,
            repository_id: unused_repository_id(),
            author_id: Some(Uuid::new_v4()),
            source_branch: "feature".to_string(),
            target_branch: "main".to_string(),
            title: "t".to_string(),
            description: String::new(),
            status: mr_status,
            merge_commit_sha: None,
            milestone_id: None,
            created_at: Utc::now(),
            closed_at: None,
        };
        let store = Arc::new(FakeMergeRequests::new(vec![mr]).with_comments(comments));
        let repositories = Arc::new(FakeRepositories::new(vec![Repository {
            id: unused_repository_id(),
            owner_id: Uuid::new_v4(),
            name: "hello".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "hello.git".to_string(),
            visibility: RepositoryVisibility::Private,
            created_at: Utc::now(),
        }]));
        let applier_id = applier.id;
        let users = Arc::new(FakeUsers::new(vec![applier]));
        let diff_reader: Arc<dyn DiffReaderPort> = Arc::new(FakeDiffReader::new(diffs));
        let branch_reader: Arc<dyn BranchReaderPort> =
            Arc::new(FakeBranchReader::new(vec![BranchInfo {
                name: "feature".to_string(),
                tip_sha: "tip123".to_string(),
                is_default: false,
            }]));
        let suggestion_executor: Arc<dyn ApplySuggestionExecutorPort> =
            Arc::new(FakeSuggestionExecutor {
                result: executor_result,
            });
        (
            ApplySuggestionCommentUseCase::new(
                store.clone(),
                store,
                repositories,
                users,
                diff_reader,
                branch_reader,
                suggestion_executor,
            ),
            applier_id,
        )
    }

    #[tokio::test]
    async fn applying_a_valid_suggestion_marks_it_applied_with_the_new_commit_sha() {
        let mr_id = Uuid::new_v4();
        let comment = suggestion_comment(mr_id, None, None, Some("line TWO\n".to_string()), false);
        let comment_id = comment.id;
        let (use_case, applier_id) = setup(
            mr_id,
            MergeRequestStatus::Open,
            vec![comment],
            florian(),
            sample_diffs(),
            Ok("newsha".to_string()),
        );

        let result = use_case
            .execute(mr_id, comment_id, applier_id)
            .await
            .unwrap();

        assert_eq!(result.applied_commit_sha.as_deref(), Some("newsha"));
        assert!(result.applied_at.is_some());
    }

    #[tokio::test]
    async fn applying_a_reply_is_rejected() {
        let mr_id = Uuid::new_v4();
        let root = suggestion_comment(mr_id, None, None, Some("line TWO\n".to_string()), false);
        let reply = suggestion_comment(mr_id, Some(root.id), None, None, false);
        let reply_id = reply.id;
        let (use_case, applier_id) = setup(
            mr_id,
            MergeRequestStatus::Open,
            vec![root, reply],
            florian(),
            sample_diffs(),
            Ok("newsha".to_string()),
        );

        let result = use_case.execute(mr_id, reply_id, applier_id).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn applying_a_comment_with_no_suggestion_is_rejected() {
        let mr_id = Uuid::new_v4();
        let comment = suggestion_comment(mr_id, None, None, None, false);
        let comment_id = comment.id;
        let (use_case, applier_id) = setup(
            mr_id,
            MergeRequestStatus::Open,
            vec![comment],
            florian(),
            sample_diffs(),
            Ok("newsha".to_string()),
        );

        let result = use_case.execute(mr_id, comment_id, applier_id).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn applying_an_already_applied_suggestion_is_rejected() {
        let mr_id = Uuid::new_v4();
        let comment = suggestion_comment(mr_id, None, None, Some("line TWO\n".to_string()), true);
        let comment_id = comment.id;
        let (use_case, applier_id) = setup(
            mr_id,
            MergeRequestStatus::Open,
            vec![comment],
            florian(),
            sample_diffs(),
            Ok("newsha".to_string()),
        );

        let result = use_case.execute(mr_id, comment_id, applier_id).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn applying_a_suggestion_on_a_non_open_merge_request_is_rejected() {
        let mr_id = Uuid::new_v4();
        let comment = suggestion_comment(mr_id, None, None, Some("line TWO\n".to_string()), false);
        let comment_id = comment.id;
        let (use_case, applier_id) = setup(
            mr_id,
            MergeRequestStatus::Merged,
            vec![comment],
            florian(),
            sample_diffs(),
            Ok("newsha".to_string()),
        );

        let result = use_case.execute(mr_id, comment_id, applier_id).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn applying_an_outdated_suggestion_is_rejected() {
        let mr_id = Uuid::new_v4();
        let mut comment =
            suggestion_comment(mr_id, None, None, Some("line TWO\n".to_string()), false);
        comment.anchor_content = Some("something else entirely\n".to_string());
        let comment_id = comment.id;
        let (use_case, applier_id) = setup(
            mr_id,
            MergeRequestStatus::Open,
            vec![comment],
            florian(),
            sample_diffs(),
            Ok("newsha".to_string()),
        );

        let result = use_case.execute(mr_id, comment_id, applier_id).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn applying_an_outdated_multi_line_suggestion_is_rejected() {
        let mr_id = Uuid::new_v4();
        // `find_lines` needs every line of the range to exist without gaps, so a range ending at line 3 can never
        // match.
        let comment = suggestion_comment(
            mr_id,
            None,
            Some(3),
            Some("replacement\n".to_string()),
            false,
        );
        let comment_id = comment.id;
        let (use_case, applier_id) = setup(
            mr_id,
            MergeRequestStatus::Open,
            vec![comment],
            florian(),
            sample_diffs(),
            Ok("newsha".to_string()),
        );

        let result = use_case.execute(mr_id, comment_id, applier_id).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn applying_a_suggestion_anchored_to_the_old_side_of_the_diff_is_rejected() {
        let mr_id = Uuid::new_v4();
        let mut comment =
            suggestion_comment(mr_id, None, None, Some("line TWO\n".to_string()), false);
        comment.side = Some(DiffSide::Old);
        let comment_id = comment.id;
        let (use_case, applier_id) = setup(
            mr_id,
            MergeRequestStatus::Open,
            vec![comment],
            florian(),
            sample_diffs(),
            Ok("newsha".to_string()),
        );

        let result = use_case.execute(mr_id, comment_id, applier_id).await;

        assert!(
            matches!(result, Err(DomainError::Validation(_))),
            "even if an old-side suggestion somehow reached storage, applying it must be rejected rather than splicing the wrong lines of the source branch, got {result:?}"
        );
    }

    #[tokio::test]
    async fn a_conflict_from_the_executor_propagates_and_the_comment_is_not_marked_applied() {
        let mr_id = Uuid::new_v4();
        let comment = suggestion_comment(mr_id, None, None, Some("line TWO\n".to_string()), false);
        let comment_id = comment.id;
        let (use_case, applier_id) = setup(
            mr_id,
            MergeRequestStatus::Open,
            vec![comment],
            florian(),
            sample_diffs(),
            Err(DomainError::Conflict("branch moved".to_string())),
        );

        let result = use_case.execute(mr_id, comment_id, applier_id).await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
    }
}
