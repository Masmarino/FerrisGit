use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::group_membership::GroupMembershipPort;
use ferrisgit_domain::repository::{Repository, RepositoryStorePort};
use ferrisgit_domain::repository_authz::effective_repository_role;
use ferrisgit_domain::repository_collaborator::{
    CollaboratorRole, RepositoryCollaboratorStorePort,
};
use uuid::Uuid;

/// Loads the repo if `caller_id` may manage its collaborators: owner of a personal repo, or anyone whose effective
/// role (inherited group roles included) is Maintainer. Everyone else gets the same `NotFound` as for an unknown repo.
pub(crate) async fn require_collaborator_manager(
    repositories: &dyn RepositoryStorePort,
    collaborators: &dyn RepositoryCollaboratorStorePort,
    groups: &dyn GroupStorePort,
    group_membership: &dyn GroupMembershipPort,
    repository_id: Uuid,
    caller_id: Uuid,
) -> Result<Repository, DomainError> {
    let not_found = || DomainError::NotFound("repository".to_string());
    let repo = repositories
        .find_by_id(repository_id)
        .await?
        .ok_or_else(not_found)?;
    let role = effective_repository_role(collaborators, groups, group_membership, &repo, caller_id)
        .await?;
    if role.is_some_and(|r| r >= CollaboratorRole::Maintainer) {
        Ok(repo)
    } else {
        Err(not_found())
    }
}
