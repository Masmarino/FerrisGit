use async_trait::async_trait;

use crate::error::DomainError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    Merged { commit_sha: String },
    Conflicting,
}

/// Merges `source_branch` into `target_branch`, clean merges only. On success the target ref points to a new two-parent
/// commit, on `Conflicting` nothing was written.
#[async_trait]
pub trait MergeExecutorPort: Send + Sync {
    async fn merge(
        &self,
        repository_disk_path: &str,
        source_branch: &str,
        target_branch: &str,
        message: &str,
    ) -> Result<MergeOutcome, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct FakeExecutor {
        outcome: MergeOutcome,
    }

    #[async_trait]
    impl MergeExecutorPort for FakeExecutor {
        async fn merge(
            &self,
            _repository_disk_path: &str,
            _source_branch: &str,
            _target_branch: &str,
            _message: &str,
        ) -> Result<MergeOutcome, DomainError> {
            Ok(self.outcome.clone())
        }
    }

    #[tokio::test]
    async fn a_port_implementation_can_be_exercised_through_the_trait_object() {
        let executor: Arc<dyn MergeExecutorPort> = Arc::new(FakeExecutor {
            outcome: MergeOutcome::Merged {
                commit_sha: "abc".to_string(),
            },
        });
        let outcome = executor
            .merge("path", "feature", "main", "Merge feature into main")
            .await
            .unwrap();
        assert_eq!(
            outcome,
            MergeOutcome::Merged {
                commit_sha: "abc".to_string()
            }
        );
    }
}
