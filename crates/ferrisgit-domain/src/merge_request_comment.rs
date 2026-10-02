use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::diff::DiffSide;
use crate::error::DomainError;

pub struct CommentAnchor {
    pub file_path: String,
    pub line_number: i32,
    /// `None` covers only `line_number`, `Some(n)` the inclusive range up to line `n` on the same side.
    pub end_line: Option<i32>,
    pub side: DiffSide,
    /// The anchored text as it was at creation, captured server-side and never taken from the client. A range is all its
    /// lines concatenated, newlines included. Comparing it with the current diff is how "outdated" is computed.
    pub anchor_content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MergeRequestComment {
    pub id: Uuid,
    pub merge_request_id: Uuid,
    /// `None` once the author's account is deleted: the comment outlives them.
    pub author_id: Option<Uuid>,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub reply_to_id: Option<Uuid>,
    pub file_path: Option<String>,
    pub line_number: Option<i32>,
    pub side: Option<DiffSide>,
    pub anchor_content: Option<String>,
    /// Only read on a thread root. Independent of `outdated`: resolving is a human choice, staleness comes from the diff.
    pub resolved: bool,
    pub end_line: Option<i32>,
    /// `None` unless the comment is a suggestion. Replies never carry one, the use case drops it.
    pub suggested_content: Option<String>,
    /// Set when a suggestion is first applied, never cleared.
    pub applied_at: Option<DateTime<Utc>>,
    pub applied_commit_sha: Option<String>,
}

pub struct NewMergeRequestComment {
    pub merge_request_id: Uuid,
    pub author_id: Uuid,
    pub body: String,
    pub reply_to_id: Option<Uuid>,
    /// `None` for a general comment or a reply, whose anchor is copied from its thread root.
    pub anchor: Option<CommentAnchor>,
    /// Only for a brand-new suggestion, ignored on a reply.
    pub suggested_content: Option<String>,
}

#[async_trait]
pub trait MergeRequestCommentPort: Send + Sync {
    async fn add_comment(
        &self,
        new_comment: NewMergeRequestComment,
    ) -> Result<MergeRequestComment, DomainError>;
    async fn list_comments(
        &self,
        merge_request_id: Uuid,
    ) -> Result<Vec<MergeRequestComment>, DomainError>;
    /// `NotFound` if there is no such comment. Pure persistence: callers check it belongs to the merge request and is a
    /// thread root.
    async fn set_comment_resolved(
        &self,
        comment_id: Uuid,
        resolved: bool,
    ) -> Result<(), DomainError>;
    /// `NotFound` if there is no such comment. Callers check it belongs to the merge request, is a root with a suggestion
    /// and isn't applied yet.
    async fn mark_comment_applied(
        &self,
        comment_id: Uuid,
        commit_sha: &str,
    ) -> Result<MergeRequestComment, DomainError>;
    /// Comment counts (general, inline and replies) for a page of merge requests. Those without comments may be missing,
    /// read them as 0. The default returns nothing so test doubles needn't implement it.
    async fn comment_counts(
        &self,
        _merge_request_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, i64>, DomainError> {
        Ok(HashMap::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_general_comment_has_no_anchor() {
        let new_comment = NewMergeRequestComment {
            merge_request_id: Uuid::new_v4(),
            author_id: Uuid::new_v4(),
            body: "hi".to_string(),
            reply_to_id: None,
            anchor: None,
            suggested_content: None,
        };
        assert!(new_comment.anchor.is_none());
        assert!(new_comment.reply_to_id.is_none());
    }

    #[test]
    fn a_new_inline_comment_carries_its_anchor() {
        let anchor = CommentAnchor {
            file_path: "README.md".to_string(),
            line_number: 2,
            end_line: None,
            side: DiffSide::New,
            anchor_content: "line 2\n".to_string(),
        };
        let new_comment = NewMergeRequestComment {
            merge_request_id: Uuid::new_v4(),
            author_id: Uuid::new_v4(),
            body: "hi".to_string(),
            reply_to_id: None,
            anchor: Some(anchor),
            suggested_content: None,
        };
        let anchor = new_comment.anchor.unwrap();
        assert_eq!(anchor.file_path, "README.md");
        assert_eq!(anchor.line_number, 2);
        assert_eq!(anchor.end_line, None);
        assert_eq!(anchor.side, DiffSide::New);
        assert_eq!(anchor.anchor_content, "line 2\n");
    }

    #[test]
    fn a_new_inline_comment_can_be_a_suggestion_with_a_multi_line_anchor() {
        let anchor = CommentAnchor {
            file_path: "README.md".to_string(),
            line_number: 2,
            end_line: Some(4),
            side: DiffSide::New,
            anchor_content: "a\nb\nc\n".to_string(),
        };
        let new_comment = NewMergeRequestComment {
            merge_request_id: Uuid::new_v4(),
            author_id: Uuid::new_v4(),
            body: "swap this block".to_string(),
            reply_to_id: None,
            anchor: Some(anchor),
            suggested_content: Some("x\ny\n".to_string()),
        };
        let anchor = new_comment.anchor.as_ref().unwrap();
        assert_eq!(anchor.end_line, Some(4));
        assert_eq!(new_comment.suggested_content.as_deref(), Some("x\ny\n"));
    }
}
