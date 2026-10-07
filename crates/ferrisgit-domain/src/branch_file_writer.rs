use async_trait::async_trait;

use crate::error::DomainError;

/// Commits one file to a branch that does not exist yet, as a single commit on top of `base_sha`. The new ref is
/// created only if it is absent, so two proposals can never land on the same branch name: the loser gets `Conflict`.
/// Other branches are left alone. `content` is written as is, with the regular file mode.
#[async_trait]
pub trait BranchFileWriterPort: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    async fn commit_file_to_new_branch(
        &self,
        repository_disk_path: &str,
        new_branch: &str,
        base_sha: &str,
        file_path: &str,
        content: &str,
        message: &str,
        committer_name: &str,
        committer_email: &str,
    ) -> Result<String, DomainError>;

    /// Removes a branch made by `commit_file_to_new_branch` while it still points at `commit_sha`: what a proposal undoes
    /// when its merge request cannot be opened. A branch that has moved since is someone's work, and is left alone
    /// (`Conflict`).
    async fn delete_new_branch(
        &self,
        repository_disk_path: &str,
        branch: &str,
        commit_sha: &str,
    ) -> Result<(), DomainError>;
}
