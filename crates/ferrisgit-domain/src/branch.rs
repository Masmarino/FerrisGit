use async_trait::async_trait;
use serde::Serialize;

use crate::error::DomainError;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    pub name: String,
    pub tip_sha: String,
    pub is_default: bool,
}

/// Lists a repository's branches straight from its git refs. Nothing about branches is persisted, so this is always a
/// live read.
#[async_trait]
pub trait BranchReaderPort: Send + Sync {
    async fn list_branches(
        &self,
        repository_disk_path: &str,
    ) -> Result<Vec<BranchInfo>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct FakeReader;
    #[async_trait]
    impl BranchReaderPort for FakeReader {
        async fn list_branches(
            &self,
            _repository_disk_path: &str,
        ) -> Result<Vec<BranchInfo>, DomainError> {
            Ok(vec![
                BranchInfo {
                    name: "main".to_string(),
                    tip_sha: "abc".to_string(),
                    is_default: true,
                },
                BranchInfo {
                    name: "feature".to_string(),
                    tip_sha: "def".to_string(),
                    is_default: false,
                },
            ])
        }
    }

    #[tokio::test]
    async fn a_port_implementation_can_be_exercised_through_the_trait_object() {
        let reader: Arc<dyn BranchReaderPort> = Arc::new(FakeReader);
        let branches = reader.list_branches("path").await.unwrap();
        assert_eq!(branches.len(), 2);
        assert!(branches.iter().any(|b| b.name == "main" && b.is_default));
    }
}
