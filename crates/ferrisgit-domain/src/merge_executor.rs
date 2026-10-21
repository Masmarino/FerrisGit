use async_trait::async_trait;
use serde::Serialize;

use crate::error::DomainError;

/// How a merge request lands on its target branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeMethod {
    /// A two-parent merge commit; the source commits keep their ids.
    #[default]
    Merge,
    /// The whole change as one new commit on top of the target.
    Squash,
    /// Each source commit replayed on top of the target, one by one, so the target history stays linear.
    Rebase,
}

impl MergeMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            MergeMethod::Merge => "merge",
            MergeMethod::Squash => "squash",
            MergeMethod::Rebase => "rebase",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "merge" => Ok(MergeMethod::Merge),
            "squash" => Ok(MergeMethod::Squash),
            "rebase" => Ok(MergeMethod::Rebase),
            other => Err(DomainError::Validation(format!(
                "unknown merge method: {other}"
            ))),
        }
    }
}

/// Who a squash commit is attributed to: the author of the merge request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitAuthor {
    pub name: String,
    pub email: String,
}

/// What to merge and how. `message` is the merge commit's, or the squash commit's; a rebase keeps each commit's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeRequestMerge {
    pub source_branch: String,
    pub target_branch: String,
    pub method: MergeMethod,
    pub message: String,
    /// The author of a squash commit; `None` gives it the server's identity.
    pub squash_author: Option<CommitAuthor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    Merged { commit_sha: String },
    Conflicting,
}

/// The merge of a merge request's source branch into its target, written as a commit that no branch points to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeRef {
    pub commit_sha: String,
    pub source_sha: String,
    pub target_sha: String,
}

/// Lands the source branch on the target branch with the requested method, clean merges only. On success the target
/// ref points to the new tip (the merge commit, the squash commit or the last rebased commit), on `Conflicting` nothing
/// was written. The source branch is never moved. A rebase of a branch that holds merge commits is a `Validation` error.
#[async_trait]
pub trait MergeExecutorPort: Send + Sync {
    async fn merge(
        &self,
        repository_disk_path: &str,
        merge: &MergeRequestMerge,
    ) -> Result<MergeOutcome, DomainError>;

    /// Writes what merging `source_branch` into `target_branch` would give, as a two-parent commit under `ref_name`
    /// (outside `refs/heads/`), leaving both branches alone. `None` when they conflict. Defaults to `unimplemented!`
    /// so test doubles need no stub.
    async fn write_merge_ref(
        &self,
        _repository_disk_path: &str,
        _source_branch: &str,
        _target_branch: &str,
        _ref_name: &str,
        _message: &str,
    ) -> Result<Option<MergeRef>, DomainError> {
        unimplemented!("write_merge_ref")
    }
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
            _merge: &MergeRequestMerge,
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
            .merge(
                "path",
                &MergeRequestMerge {
                    source_branch: "feature".to_string(),
                    target_branch: "main".to_string(),
                    method: MergeMethod::Merge,
                    message: "Merge feature into main".to_string(),
                    squash_author: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            outcome,
            MergeOutcome::Merged {
                commit_sha: "abc".to_string()
            }
        );
    }

    #[test]
    fn a_merge_method_round_trips_through_its_string_form() {
        for method in [MergeMethod::Merge, MergeMethod::Squash, MergeMethod::Rebase] {
            assert_eq!(MergeMethod::parse(method.as_str()).unwrap(), method);
        }
        assert_eq!(MergeMethod::default(), MergeMethod::Merge);
        assert!(matches!(
            MergeMethod::parse("fast-forward"),
            Err(DomainError::Validation(_))
        ));
    }
}
