use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::repository::{Repository, RepositoryVisibility};
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use uuid::Uuid;

use crate::state::AppState;

/// Same policy as `require_role_by_id`, for routes that only have an owner/name pair.
pub async fn require_role(
    state: &AppState,
    caller_id: Uuid,
    owner: &str,
    name: &str,
    min_role: CollaboratorRole,
) -> Result<Repository, DomainError> {
    let not_found = || DomainError::NotFound("repository".to_string());
    let owner_user = state
        .users
        .find_by_username(owner)
        .await?
        .ok_or_else(not_found)?;
    let repo = state
        .repositories
        .find_by_owner_and_name(owner_user.id, name)
        .await?
        .ok_or_else(not_found)?;
    require_role_by_id(state, caller_id, repo.id, min_role).await
}

use ferrisgit_domain::group::Group;
use ferrisgit_domain::group_membership::GroupMembershipPort;

/// Role label for a response. No role at all (a public repository, say) reads as `reader`.
pub fn role_label_or_reader(role: Option<CollaboratorRole>) -> String {
    role.map_or_else(|| "reader".to_string(), |r| r.as_str().to_string())
}

/// `chain` is root first and ends with the group being checked.
pub async fn effective_role_in_group_chain(
    groups: &dyn GroupMembershipPort,
    chain: &[Group],
    user_id: Uuid,
) -> Result<Option<CollaboratorRole>, DomainError> {
    let mut best: Option<CollaboratorRole> = None;
    for group in chain {
        if let Some(role) = groups.get_member_role(group.id, user_id).await? {
            best = Some(best.map_or(role, |b| b.max(role)));
        }
    }
    Ok(best)
}

/// The gate for every repository-scoped route, personal or group.
///
/// A public repository lets any authenticated caller read (`min_role <= Reader`), like the git read bypass. It
/// runs after the lookup, so an unknown repository is still a `NotFound`. Everything else goes through the role
/// policy the git smart-HTTP path shares (`effective_repository_role`).
pub async fn require_role_by_id(
    state: &AppState,
    caller_id: Uuid,
    repository_id: Uuid,
    min_role: CollaboratorRole,
) -> Result<Repository, DomainError> {
    let not_found = || DomainError::NotFound("repository".to_string());
    let repo = state
        .repositories
        .find_by_id(repository_id)
        .await?
        .ok_or_else(not_found)?;

    if repo.visibility == RepositoryVisibility::Public && min_role <= CollaboratorRole::Reader {
        return Ok(repo);
    }

    let role = ferrisgit_domain::repository_authz::effective_repository_role(
        state.repository_collaborators.as_ref(),
        state.groups.as_ref(),
        state.group_membership.as_ref(),
        &repo,
        caller_id,
    )
    .await?;
    match role {
        Some(role) if role >= min_role => Ok(repo),
        _ => Err(not_found()),
    }
}

/// `require_role_by_id` without the public bypass, for routes where a public repository shouldn't make the
/// membership check optional (the collaborator listing, for one).
pub async fn require_explicit_role_by_id(
    state: &AppState,
    caller_id: Uuid,
    repository_id: Uuid,
    min_role: CollaboratorRole,
) -> Result<Repository, DomainError> {
    let not_found = || DomainError::NotFound("repository".to_string());
    let repo = state
        .repositories
        .find_by_id(repository_id)
        .await?
        .ok_or_else(not_found)?;
    let role = ferrisgit_domain::repository_authz::effective_repository_role(
        state.repository_collaborators.as_ref(),
        state.groups.as_ref(),
        state.group_membership.as_ref(),
        &repo,
        caller_id,
    )
    .await?;
    match role {
        Some(role) if role >= min_role => Ok(repo),
        _ => Err(not_found()),
    }
}

/// The group version of `require_role_by_id`: no public bypass, no `owner_id` fallback. A group the caller can't
/// reach is a `NotFound`.
pub async fn require_group_role_by_id(
    state: &AppState,
    caller_id: Uuid,
    group_id: Uuid,
    min_role: CollaboratorRole,
) -> Result<Group, DomainError> {
    let (mut chain, _) = require_group_chain_role(state, caller_id, group_id, min_role).await?;
    Ok(chain.pop().expect("the chain ends with the group"))
}

/// Like `require_group_role_by_id`, but also returns the ancestor chain (root first) and the caller's role in it,
/// for routes that build paths from the chain.
pub async fn require_group_chain_role(
    state: &AppState,
    caller_id: Uuid,
    group_id: Uuid,
    min_role: CollaboratorRole,
) -> Result<(Vec<Group>, CollaboratorRole), DomainError> {
    let not_found = || DomainError::NotFound("group".to_string());
    let chain = state.groups.ancestor_chain(group_id).await?;
    if chain.is_empty() {
        return Err(not_found());
    }
    let role =
        effective_role_in_group_chain(state.group_membership.as_ref(), &chain, caller_id).await?;
    match role {
        Some(role) if role >= min_role => Ok((chain, role)),
        _ => Err(not_found()),
    }
}
