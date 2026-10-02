use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::DomainError;

/// A consumed activation token: the account it activates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invitation {
    pub user_id: Uuid,
    pub expires_at: DateTime<Utc>,
}

/// Pending activation links of invited users. Only the token hash is stored.
#[async_trait]
pub trait UserInvitationPort: Send + Sync {
    /// Replaces the user's invitation, if any: one live invitation per user.
    async fn replace(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), DomainError>;
    /// Gives the user's existing invitation a new token and expiry, expired or not. Returns `false` if there is none: unlike
    /// `replace` it never creates one, so a resend racing an activation can't revive an invitation on an active account.
    async fn renew(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<bool, DomainError>;
    /// Single use: returns the invitation only if the token exists and hasn't expired, and deletes it.
    async fn consume(&self, token_hash: &str) -> Result<Option<Invitation>, DomainError>;
    /// Pending invitations for these users, expired ones included, as (user, expiry).
    async fn expiries(&self, user_ids: &[Uuid]) -> Result<Vec<(Uuid, DateTime<Utc>)>, DomainError>;
}
