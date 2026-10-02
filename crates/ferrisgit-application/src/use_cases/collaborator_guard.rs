use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::group_membership::GroupMembershipPort;
use ferrisgit_domain::repository::{Repository, RepositoryStorePort};
use ferrisgit_domain::repository_authz::effective_repository_role;
use ferrisgit_domain::repository_collaborator::{
    CollaboratorRole, RepositoryCollaboratorStorePort,
};
use uuid::Uuid;

/// Loads the repository and checks that `caller_id` is allowed to manage its collaborators: the owner of a personal
/// repository, or anyone whose effective role is Maintainer, a group role inherited down the hierarchy included. A
/// caller who may not manage them gets the same `NotFound` as an unknown repository.
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
