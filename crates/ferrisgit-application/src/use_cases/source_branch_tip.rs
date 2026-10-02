use ferrisgit_domain::branch::BranchReaderPort;
use ferrisgit_domain::error::DomainError;

/// The commit a merge request's source branch points at right now. Reviews and approvals are pinned to it.
pub(crate) async fn source_branch_tip(
    branch_reader: &dyn BranchReaderPort,
    repository_disk_path: &str,
    source_branch: &str,
) -> Result<String, DomainError> {
    branch_reader
        .list_branches(repository_disk_path)
        .await?
        .into_iter()
        .find(|branch| branch.name == source_branch)
        .map(|branch| branch.tip_sha)
        .ok_or_else(|| DomainError::Validation("source branch no longer exists".to_string()))
}
