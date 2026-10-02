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

use super::source_branch_tip::source_branch_tip;

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
        // Last check before the git write: an old-side suggestion would silently splice the wrong lines.
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

        // Read the tip before re-reading the diff, don't swap them. It's the expected tip of the git compare-and-swap,
        // so a push landing in between makes the swap fail with Conflict. The other way round we'd validate stale
        // content and the swap would succeed on the newer tip, writing the wrong thing.
        let tip_sha = source_branch_tip(
            self.branch_reader.as_ref(),
            &repo.disk_path,
            &mr.source_branch,
        )
        .await?;

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
    use crate::use_cases::fixtures::{
        added_line, merge_request, merge_request_comment, readme_diff, repository, user,
    };
    use async_trait::async_trait;
    use ferrisgit_domain::branch::BranchInfo;
    use ferrisgit_domain::merge_request::MergeRequest;

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

    /// A merge request from `feature` (at `tip123`) whose diff adds line 2 of `README.md`, and an applier whose git
    /// write ends in `executor_result`.
    struct Setup {
        use_case: ApplySuggestionCommentUseCase,
        store: Arc<FakeMergeRequests>,
        mr_id: Uuid,
        applier_id: Uuid,
    }

    fn setup(mr_status: MergeRequestStatus, executor_result: Result<String, DomainError>) -> Setup {
        let repo = repository(Uuid::new_v4());
        let mr = MergeRequest {
            status: mr_status,
            ..merge_request(repo.id, Uuid::new_v4())
        };
        let mr_id = mr.id;
        let applier = user("florian");
        let applier_id = applier.id;
        let store = Arc::new(FakeMergeRequests::new(vec![mr]));
        let use_case = ApplySuggestionCommentUseCase::new(
            store.clone(),
            store.clone(),
            Arc::new(FakeRepositories::new(vec![repo])),
            Arc::new(FakeUsers::new(vec![applier])),
            Arc::new(FakeDiffReader::new(readme_diff(vec![added_line(2)]))),
            Arc::new(FakeBranchReader::new(vec![BranchInfo {
                name: "feature".to_string(),
                tip_sha: "tip123".to_string(),
                is_default: false,
            }])),
            Arc::new(FakeSuggestionExecutor {
                result: executor_result,
            }),
        );
        Setup {
            use_case,
            store,
            mr_id,
            applier_id,
        }
    }

    fn open_setup() -> Setup {
        setup(MergeRequestStatus::Open, Ok("newsha".to_string()))
    }

    impl Setup {
        fn seed(&self, comment: MergeRequestComment) -> Uuid {
            let id = comment.id;
            self.store.seed_comment(comment);
            id
        }

        async fn apply(
            &self,
            comment: MergeRequestComment,
        ) -> Result<MergeRequestComment, DomainError> {
            let comment_id = self.seed(comment);
            self.use_case
                .execute(self.mr_id, comment_id, self.applier_id)
                .await
        }
    }

    /// A suggestion on line 2 of `README.md` replacing `line 2` with `line TWO`.
    fn suggestion_comment(merge_request_id: Uuid) -> MergeRequestComment {
        MergeRequestComment {
            file_path: Some("README.md".to_string()),
            line_number: Some(2),
            side: Some(DiffSide::New),
            anchor_content: Some("line 2\n".to_string()),
            suggested_content: Some("line TWO\n".to_string()),
            ..merge_request_comment(merge_request_id)
        }
    }

    #[tokio::test]
    async fn applying_a_valid_suggestion_marks_it_applied_with_the_new_commit_sha() {
        let s = open_setup();

        let result = s.apply(suggestion_comment(s.mr_id)).await.unwrap();

        assert_eq!(result.applied_commit_sha.as_deref(), Some("newsha"));
        assert!(result.applied_at.is_some());
    }

    #[tokio::test]
    async fn applying_a_reply_is_rejected() {
        let s = open_setup();
        let root = suggestion_comment(s.mr_id);
        let reply = MergeRequestComment {
            reply_to_id: Some(root.id),
            suggested_content: None,
            ..suggestion_comment(s.mr_id)
        };
        s.seed(root);

        let result = s.apply(reply).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn applying_a_comment_with_no_suggestion_is_rejected() {
        let s = open_setup();

        let result = s
            .apply(MergeRequestComment {
                suggested_content: None,
                ..suggestion_comment(s.mr_id)
            })
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn applying_an_already_applied_suggestion_is_rejected() {
        let s = open_setup();

        let result = s
            .apply(MergeRequestComment {
                applied_at: Some(chrono::Utc::now()),
                applied_commit_sha: Some("already-applied-sha".to_string()),
                ..suggestion_comment(s.mr_id)
            })
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn applying_a_suggestion_on_a_non_open_merge_request_is_rejected() {
        let s = setup(MergeRequestStatus::Merged, Ok("newsha".to_string()));

        let result = s.apply(suggestion_comment(s.mr_id)).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn applying_an_outdated_suggestion_is_rejected() {
        let s = open_setup();

        let result = s
            .apply(MergeRequestComment {
                anchor_content: Some("something else entirely\n".to_string()),
                ..suggestion_comment(s.mr_id)
            })
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn applying_an_outdated_multi_line_suggestion_is_rejected() {
        let s = open_setup();

        // The diff has no line 3, so the range can't match.
        let result = s
            .apply(MergeRequestComment {
                end_line: Some(3),
                suggested_content: Some("replacement\n".to_string()),
                ..suggestion_comment(s.mr_id)
            })
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn applying_a_suggestion_anchored_to_the_old_side_of_the_diff_is_rejected() {
        let s = open_setup();

        let result = s
            .apply(MergeRequestComment {
                side: Some(DiffSide::Old),
                ..suggestion_comment(s.mr_id)
            })
            .await;

        assert!(
            matches!(result, Err(DomainError::Validation(_))),
            "even if an old-side suggestion somehow reached storage, applying it must be rejected rather than splicing the wrong lines of the source branch, got {result:?}"
        );
    }

    #[tokio::test]
    async fn a_conflict_from_the_executor_propagates_and_the_comment_is_not_marked_applied() {
        let s = setup(
            MergeRequestStatus::Open,
            Err(DomainError::Conflict("branch moved".to_string())),
        );

        let result = s.apply(suggestion_comment(s.mr_id)).await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
    }
}
