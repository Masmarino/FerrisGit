use async_trait::async_trait;

use crate::error::DomainError;

/// Reads a file at a given commit without checking it out. `revision` is a full sha, `path` is `/`-separated and
/// repo-relative. `Ok(None)` when the file doesn't exist there, which is the common case and not an error.
#[async_trait]
pub trait PipelineFileReaderPort: Send + Sync {
    async fn read_file_at_revision(
        &self,
        repository_disk_path: &str,
        revision: &str,
        path: &str,
    ) -> Result<Option<Vec<u8>>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct FakeReader;
    #[async_trait]
    impl PipelineFileReaderPort for FakeReader {
        async fn read_file_at_revision(
            &self,
            _repository_disk_path: &str,
            _revision: &str,
            path: &str,
        ) -> Result<Option<Vec<u8>>, DomainError> {
            if path == "exists.yml" {
                Ok(Some(b"stages: []".to_vec()))
            } else {
                Ok(None)
            }
        }
    }

    #[tokio::test]
    async fn a_port_implementation_can_be_exercised_through_the_trait_object() {
        let reader: Arc<dyn PipelineFileReaderPort> = Arc::new(FakeReader);
        assert_eq!(
            reader
                .read_file_at_revision("path", "sha", "exists.yml")
                .await
                .unwrap(),
            Some(b"stages: []".to_vec())
        );
        assert_eq!(
            reader
                .read_file_at_revision("path", "sha", "missing.yml")
                .await
                .unwrap(),
            None
        );
    }
}
