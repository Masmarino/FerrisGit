use async_trait::async_trait;

use crate::error::DomainError;

/// Commits a suggestion's replacement to `branch` as one new single-parent commit, touching no other branch.
/// `expected_tip` is the branch sha the caller validated the anchor against. The git write re-checks it atomically
/// (compare-and-swap on the ref), like `MergeExecutorPort::merge`.
///
/// `start_line`/`end_line` are 1-based inclusive lines of `file_path` at `expected_tip` (the source side a suggestion
/// is anchored to). `new_content` replaces the range as is, with any number of lines. An empty one deletes the range,
/// which is not an error.
///
/// Known limitation: the file is read with `git show` and decoded lossily as UTF-8. Binary files (NUL byte) are
/// rejected upstream, but a text file with invalid UTF-8 and no NUL byte would be silently corrupted. The whole diff
/// and comment subsystem assumes UTF-8.
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
