use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::{Group, GroupStorePort};
use ferrisgit_domain::group_membership::GroupMembershipPort;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use uuid::Uuid;

/// Checks that `caller_id` may manage `group_id`: a Maintainer role on the group or on any of its ancestors counts.
/// Returns the group's ancestor chain (root first, the group itself last), which the last-maintainer checks need. A
/// caller who may not manage it gets the same `NotFound` as an unknown group.
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
