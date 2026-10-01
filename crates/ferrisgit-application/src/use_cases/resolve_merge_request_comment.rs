use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::merge_request_comment::MergeRequestCommentPort;
use uuid::Uuid;

pub struct ResolveMergeRequestCommentUseCase {
    merge_requests: Arc<dyn MergeRequestCommentPort>,
}

impl ResolveMergeRequestCommentUseCase {
    pub fn new(merge_requests: Arc<dyn MergeRequestCommentPort>) -> Self {
        Self { merge_requests }
    }

    pub async fn execute(
        &self,
        merge_request_id: Uuid,
        comment_id: Uuid,
        resolved: bool,
    ) -> Result<(), DomainError> {
        let comments = self.merge_requests.list_comments(merge_request_id).await?;
        let target = comments
            .iter()
            .find(|c| c.id == comment_id)
            .ok_or_else(|| DomainError::NotFound("comment".to_string()))?;
        if target.reply_to_id.is_some() {
            return Err(DomainError::Validation(
                "only a thread's root comment can be resolved".to_string(),
            ));
        }
        self.merge_requests
            .set_comment_resolved(comment_id, resolved)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeMergeRequests;
    use chrono::Utc;
    use ferrisgit_domain::merge_request_comment::MergeRequestComment;

    fn root_comment(merge_request_id: Uuid, reply_to_id: Option<Uuid>) -> MergeRequestComment {
        MergeRequestComment {
            id: Uuid::new_v4(),
            merge_request_id,
            author_id: Some(Uuid::new_v4()),
            body: "hi".to_string(),
            created_at: Utc::now(),
            reply_to_id,
            file_path: None,
            line_number: None,
            side: None,
            anchor_content: None,
            resolved: false,
            end_line: None,
            suggested_content: None,
            applied_at: None,
            applied_commit_sha: None,
        }
    }

    fn store_with(comments: Vec<MergeRequestComment>) -> Arc<FakeMergeRequests> {
        Arc::new(FakeMergeRequests::new(vec![]).with_comments(comments))
    }

    #[tokio::test]
    async fn resolves_a_root_comment() {
        let mr_id = Uuid::new_v4();
        let root = root_comment(mr_id, None);
        let root_id = root.id;
        let store = store_with(vec![root]);
        let use_case = ResolveMergeRequestCommentUseCase::new(store.clone());

        use_case.execute(mr_id, root_id, true).await.unwrap();

        assert!(store.list_comments(mr_id).await.unwrap()[0].resolved);
    }

    #[tokio::test]
    async fn unresolving_a_resolved_root_comment_works() {
        let mr_id = Uuid::new_v4();
        let mut root = root_comment(mr_id, None);
        root.resolved = true;
        let root_id = root.id;
        let store = store_with(vec![root]);
        let use_case = ResolveMergeRequestCommentUseCase::new(store.clone());

        use_case.execute(mr_id, root_id, false).await.unwrap();

        assert!(!store.list_comments(mr_id).await.unwrap()[0].resolved);
    }

    #[tokio::test]
    async fn resolving_a_reply_is_rejected() {
        let mr_id = Uuid::new_v4();
        let root = root_comment(mr_id, None);
        let reply = root_comment(mr_id, Some(root.id));
        let reply_id = reply.id;
        let store = store_with(vec![root, reply]);
        let use_case = ResolveMergeRequestCommentUseCase::new(store);

        let result = use_case.execute(mr_id, reply_id, true).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn resolving_a_comment_that_does_not_exist_on_this_merge_request_is_not_found() {
        let mr_id = Uuid::new_v4();
        let store = store_with(vec![]);
        let use_case = ResolveMergeRequestCommentUseCase::new(store);

        let result = use_case.execute(mr_id, Uuid::new_v4(), true).await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn resolving_a_comment_that_belongs_to_a_different_merge_request_is_not_found() {
        let mr_id = Uuid::new_v4();
        let other_mr_id = Uuid::new_v4();
        let foreign_root = root_comment(other_mr_id, None);
        let foreign_root_id = foreign_root.id;
        let store = store_with(vec![foreign_root]);
        let use_case = ResolveMergeRequestCommentUseCase::new(store);

        let result = use_case.execute(mr_id, foreign_root_id, true).await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }
}
