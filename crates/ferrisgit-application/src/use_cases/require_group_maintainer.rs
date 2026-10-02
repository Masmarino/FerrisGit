use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::{Group, GroupStorePort};
use ferrisgit_domain::group_membership::GroupMembershipPort;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use uuid::Uuid;

/// Maintainer on the group or any ancestor is enough. Returns the ancestor chain (root first) for the
/// last-maintainer checks. Anyone else gets the same not found as for an unknown group.
pub(crate) async fn require_group_maintainer(
    groups: &dyn GroupStorePort,
    group_membership: &dyn GroupMembershipPort,
    group_id: Uuid,
    caller_id: Uuid,
) -> Result<Vec<Group>, DomainError> {
    let chain = groups.ancestor_chain(group_id).await?;
    for group in &chain {
        let role = group_membership
            .get_member_role(group.id, caller_id)
            .await?;
        if role.is_some_and(|role| role >= CollaboratorRole::Maintainer) {
            return Ok(chain);
        }
    }
    Err(DomainError::NotFound("group".to_string()))
}
