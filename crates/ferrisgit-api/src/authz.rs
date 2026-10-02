use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::repository::{Repository, RepositoryVisibility};
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use uuid::Uuid;

use crate::state::AppState;

/// Same policy as `require_role_by_id`, for the few routes that only have an owner/name pair.
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

/// `chain` is root-first, ending with the group being checked.
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

/// The authorization entry point for every repository-scoped route, personal or group.
///
/// A `Public` repository grants any authenticated caller Reader access (`min_role <= Reader`), mirroring the
/// git-protocol read bypass. It runs after the lookup, so it never bypasses the `NotFound` masking.
///
/// Otherwise this is a hand copy of `AuthenticateGitRequestUseCase::effective_role` in `ferrisgit-application`
/// (which cannot depend on this crate). In a personal repo the owner gets the top role and anyone else needs a
/// direct collaborator grant. In a group repo it is the higher of the caller's role in the ancestor chain and any
/// direct grant, with no owner bypass. Change both together: `tests/authz_parity_flow.rs` fails if they drift.
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

/// Same as `require_role_by_id` minus the `Public` bypass, for routes where public readability must not make an
/// explicit membership check optional (for example the collaborator listing).
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

/// The group analog of `require_role_by_id`: no `Public` bypass and no `owner_id` fallback. Masks unreachable
/// groups as `NotFound`.
pub async fn require_group_role_by_id(
    state: &AppState,
    caller_id: Uuid,
    group_id: Uuid,
    min_role: CollaboratorRole,
) -> Result<Group, DomainError> {
    let (mut chain, _) = require_group_chain_role(state, caller_id, group_id, min_role).await?;
    Ok(chain.pop().expect("the chain ends with the group"))
}

/// Like `require_group_role_by_id`, but hands back what the check resolved: the group's ancestor chain (root first,
/// ending with the group) and the caller's effective role in it. For routes that build paths from the chain.
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
