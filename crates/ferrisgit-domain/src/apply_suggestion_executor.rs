use async_trait::async_trait;

use crate::error::DomainError;

/// Commits a suggestion's replacement to `branch` as one new commit, leaving other branches alone. The ref update is a
/// compare-and-swap on `expected_tip`, like the merge executor.
/// Lines are 1-based and inclusive at `expected_tip`. Empty `new_content` deletes the range, which is fine.
///
/// The file is decoded lossily as UTF-8: binary files are rejected upstream, but invalid UTF-8 without a NUL byte would
/// be silently mangled. The whole diff and comment code assumes UTF-8.
#[async_trait]
pub trait ApplySuggestionExecutorPort: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    async fn apply_replacement(
        &self,
        repository_disk_path: &str,
        branch: &str,
        expected_tip: &str,
        file_path: &str,
        start_line: i32,
        end_line: i32,
        new_content: &str,
        message: &str,
        committer_name: &str,
        committer_email: &str,
    ) -> Result<String, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct FakeExecutor;
    #[async_trait]
    impl ApplySuggestionExecutorPort for FakeExecutor {
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
            Ok("deadbeef".to_string())
        }
    }

    #[tokio::test]
    async fn a_port_implementation_can_be_exercised_through_the_trait_object() {
        let executor: Arc<dyn ApplySuggestionExecutorPort> = Arc::new(FakeExecutor);
        let sha = executor
            .apply_replacement(
                "path",
                "feature",
                "abc123",
                "README.md",
                2,
                4,
                "new\n",
                "Apply suggestion",
                "florian",
                "florian@example.com",
            )
            .await
            .unwrap();
        assert_eq!(sha, "deadbeef");
    }
}
