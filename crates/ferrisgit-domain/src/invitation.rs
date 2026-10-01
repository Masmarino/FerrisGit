use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::DomainError;

/// What consuming a valid activation token yields: whose account it activates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invitation {
    pub user_id: Uuid,
    pub expires_at: DateTime<Utc>,
}

/// Pending activation links of invited users. Only the hash of a token is ever stored.
#[async_trait]
pub trait UserInvitationPort: Send + Sync {
    /// Replaces any existing invitation of the user (one live invitation per user).
    async fn replace(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), DomainError>;
    /// Gives an existing invitation of the user a new token and expiry (expired rows included). Returns `true` only if
    /// a row was updated. Unlike `replace` it never creates a row: a user whose invitation is gone (activated
    /// meanwhile) gets `false`, so a resend racing an activation cannot bring an invitation back on an active account.
    async fn renew(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<bool, DomainError>;
    /// Atomic single use: `Some` only for a token that exists and has not expired; the row is deleted.
    async fn consume(&self, token_hash: &str) -> Result<Option<Invitation>, DomainError>;
    /// The invitations pending for these users (expired ones included), as (user, expiry).
    async fn expiries(&self, user_ids: &[Uuid]) -> Result<Vec<(Uuid, DateTime<Utc>)>, DomainError>;
}
