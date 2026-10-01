use async_trait::async_trait;
use serde::Serialize;

use crate::error::DomainError;

#[derive(Debug, Clone, Serialize)]
pub struct TagInfo {
    pub name: String,
    pub target_sha: String,
}

/// Live read from git refs; nothing about tags is persisted.
#[async_trait]
pub trait TagReaderPort: Send + Sync {
    async fn list_tags(&self, repository_disk_path: &str) -> Result<Vec<TagInfo>, DomainError>;
}

/// Creates or deletes a repository's lightweight git tags (a plain `refs/tags/<name>` ref pointing at a commit, never
/// an annotated tag object).
#[async_trait]
pub trait TagCreatorPort: Send + Sync {
    /// Errs `Conflict` if a tag with this name already exists.
    async fn create_tag(
        &self,
        repository_disk_path: &str,
        tag_name: &str,
        target_commit_sha: &str,
    ) -> Result<(), DomainError>;
    /// `NotFound` if no such tag. Unconditional (no compare-and-swap): `DeleteTagUseCase` decides whether a tag is
    /// safe to delete (no release references it).
    async fn delete_tag(
        &self,
        repository_disk_path: &str,
        tag_name: &str,
    ) -> Result<(), DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct FakeReader(Vec<TagInfo>);
    #[async_trait]
    impl TagReaderPort for FakeReader {
        async fn list_tags(
            &self,
            _repository_disk_path: &str,
        ) -> Result<Vec<TagInfo>, DomainError> {
            Ok(self.0.clone())
        }
    }

    #[tokio::test]
    async fn a_port_implementation_can_be_exercised_through_the_trait_object() {
        let reader: Arc<dyn TagReaderPort> = Arc::new(FakeReader(vec![TagInfo {
            name: "v1.0.0".to_string(),
            target_sha: "abc123".to_string(),
        }]));
        let tags = reader.list_tags("hello.git").await.unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "v1.0.0");
    }
}
