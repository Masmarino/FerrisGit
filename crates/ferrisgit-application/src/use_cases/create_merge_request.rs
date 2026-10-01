use std::sync::Arc;

use ferrisgit_domain::branch::BranchReaderPort;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::merge_request::{MergeRequest, MergeRequestStorePort, NewMergeRequest};
use uuid::Uuid;

pub struct CreateMergeRequestUseCase {
    merge_requests: Arc<dyn MergeRequestStorePort>,
    branch_reader: Arc<dyn BranchReaderPort>,
}

impl CreateMergeRequestUseCase {
    pub fn new(
        merge_requests: Arc<dyn MergeRequestStorePort>,
        branch_reader: Arc<dyn BranchReaderPort>,
    ) -> Self {
        Self {
            merge_requests,
            branch_reader,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        repository_id: Uuid,
        repository_disk_path: &str,
        author_id: Uuid,
        source_branch: String,
        target_branch: String,
        title: String,
        description: String,
    ) -> Result<MergeRequest, DomainError> {
        if source_branch == target_branch {
            return Err(DomainError::Validation(
                "source and target branch must be different".to_string(),
            ));
        }
        let branches = self
            .branch_reader
            .list_branches(repository_disk_path)
            .await?;
        if !branches.iter().any(|b| b.name == source_branch) {
            return Err(DomainError::Validation(format!(
                "branch not found: {source_branch}"
            )));
        }
        if !branches.iter().any(|b| b.name == target_branch) {
            return Err(DomainError::Validation(format!(
                "branch not found: {target_branch}"
            )));
        }
        self.merge_requests
            .create(NewMergeRequest {
                repository_id,
                author_id,
                source_branch,
                target_branch,
                title,
                description,
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeBranchReader, FakeMergeRequests};
    use ferrisgit_domain::branch::BranchInfo;
    use ferrisgit_domain::merge_request::MergeRequestStatus;

    /// Builds a `FakeBranchReader` from bare branch names, `"main"` being the default.
    fn branch_reader(names: &[&str]) -> Arc<FakeBranchReader> {
        Arc::new(FakeBranchReader::new(
            names
                .iter()
                .map(|name| BranchInfo {
                    name: name.to_string(),
                    tip_sha: "sha".to_string(),
                    is_default: *name == "main",
                })
                .collect(),
        ))
    }

    #[tokio::test]
    async fn creates_a_merge_request_between_two_existing_branches() {
        let merge_requests = Arc::new(FakeMergeRequests::empty());
        let use_case = CreateMergeRequestUseCase::new(
            merge_requests.clone(),
            branch_reader(&["main", "feature"]),
        );

        let mr = use_case
            .execute(
                Uuid::new_v4(),
                "path",
                Uuid::new_v4(),
                "feature".to_string(),
                "main".to_string(),
                "Add feature".to_string(),
                "desc".to_string(),
            )
            .await
            .unwrap();

        assert_eq!(mr.source_branch, "feature");
        assert_eq!(mr.target_branch, "main");
        assert_eq!(mr.status, MergeRequestStatus::Open);
    }

    #[tokio::test]
    async fn rejects_a_merge_request_with_the_same_source_and_target_branch() {
        let merge_requests = Arc::new(FakeMergeRequests::empty());
        let use_case =
            CreateMergeRequestUseCase::new(merge_requests.clone(), branch_reader(&["main"]));

        let result = use_case
            .execute(
                Uuid::new_v4(),
                "path",
                Uuid::new_v4(),
                "main".to_string(),
                "main".to_string(),
                "t".to_string(),
                "d".to_string(),
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
        assert!(merge_requests.snapshot().is_empty());
    }

    #[tokio::test]
    async fn rejects_a_merge_request_whose_source_branch_does_not_exist() {
        let use_case = CreateMergeRequestUseCase::new(
            Arc::new(FakeMergeRequests::empty()),
            branch_reader(&["main"]),
        );

        let result = use_case
            .execute(
                Uuid::new_v4(),
                "path",
                Uuid::new_v4(),
                "does-not-exist".to_string(),
                "main".to_string(),
                "t".to_string(),
                "d".to_string(),
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn rejects_a_merge_request_whose_target_branch_does_not_exist() {
        let use_case = CreateMergeRequestUseCase::new(
            Arc::new(FakeMergeRequests::empty()),
            branch_reader(&["feature"]),
        );

        let result = use_case
            .execute(
                Uuid::new_v4(),
                "path",
                Uuid::new_v4(),
                "feature".to_string(),
                "does-not-exist".to_string(),
                "t".to_string(),
                "d".to_string(),
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }
}
