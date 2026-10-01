use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::Group;
use ferrisgit_domain::group_membership::GroupMembershipPort;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use uuid::Uuid;

/// True if no Maintainer role remains anywhere in `chain` once the Maintainer grant of
/// `(excluded_group_id, excluded_user_id)` is left out. Nobody could then manage the hierarchy again (groups have no
/// `owner_id` fallback). `chain` is the target's `ancestor_chain` (root-first, including itself). The whole chain
/// counts, since a subgroup stays manageable through an ancestor's Maintainer.
pub(crate) async fn would_leave_chain_without_a_maintainer(
    groups: &dyn GroupMembershipPort,
    chain: &[Group],
    excluded_group_id: Uuid,
    excluded_user_id: Uuid,
) -> Result<bool, DomainError> {
    no_other_maintainer_in_chain(groups, chain, |group_id, user_id| {
        group_id == excluded_group_id && user_id == excluded_user_id
    })
    .await
}

/// The same danger when `user_id` leaves every group at once (their account is deleted): true if no Maintainer role
/// remains anywhere in `chain` once all of `user_id`'s grants are gone. Leaving out their grants one at a time would
/// not work: a user who is Maintainer of both a group and its parent would count as the other group's remaining
/// Maintainer.
pub(crate) async fn would_leave_chain_without_a_maintainer_without_user(
    groups: &dyn GroupMembershipPort,
    chain: &[Group],
    user_id: Uuid,
) -> Result<bool, DomainError> {
    no_other_maintainer_in_chain(groups, chain, |_, member_id| member_id == user_id).await
}

async fn no_other_maintainer_in_chain(
    groups: &dyn GroupMembershipPort,
    chain: &[Group],
    is_excluded: impl Fn(Uuid, Uuid) -> bool,
) -> Result<bool, DomainError> {
    for group in chain {
        for member in groups.list_members(group.id).await? {
            if member.role == CollaboratorRole::Maintainer && !is_excluded(group.id, member.user_id)
            {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
