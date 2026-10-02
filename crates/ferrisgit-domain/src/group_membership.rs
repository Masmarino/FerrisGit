use async_trait::async_trait;
use uuid::Uuid;

use crate::error::DomainError;
use crate::group::GroupMember;
use crate::repository_collaborator::CollaboratorRole;

#[async_trait]
pub trait GroupMembershipPort: Send + Sync {
    async fn add_member(
        &self,
        group_id: Uuid,
        user_id: Uuid,
        role: CollaboratorRole,
    ) -> Result<(), DomainError>;
    /// `NotFound("group member")` if the pair doesn't exist.
    async fn set_member_role(
        &self,
        group_id: Uuid,
        user_id: Uuid,
        role: CollaboratorRole,
    ) -> Result<(), DomainError>;
    /// Doesn't fail if the pair doesn't exist.
    async fn remove_member(&self, group_id: Uuid, user_id: Uuid) -> Result<(), DomainError>;
    async fn list_members(&self, group_id: Uuid) -> Result<Vec<GroupMember>, DomainError>;
    async fn get_member_role(
        &self,
        group_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<CollaboratorRole>, DomainError>;
}
