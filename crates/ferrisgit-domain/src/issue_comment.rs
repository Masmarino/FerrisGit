use std::collections::HashMap;

use async_trait::async_trait;
use uuid::Uuid;

use crate::error::DomainError;
use crate::issue::{IssueComment, NewIssueComment};

#[async_trait]
pub trait IssueCommentPort: Send + Sync {
    async fn add_comment(&self, new_comment: NewIssueComment) -> Result<IssueComment, DomainError>;
    async fn list_comments(&self, issue_id: Uuid) -> Result<Vec<IssueComment>, DomainError>;
    /// Number of comments per issue, for a whole page of issues in one call. Issues without comments may be absent from
    /// the map (read them as 0). The default reports no comments so test doubles need not implement it; the real
    /// store overrides it.
    async fn comment_counts(&self, _issue_ids: &[Uuid]) -> Result<HashMap<Uuid, i64>, DomainError> {
        Ok(HashMap::new())
    }
}
