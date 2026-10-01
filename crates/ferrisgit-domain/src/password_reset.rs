use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::DomainError;

/// What consuming a valid password-reset token yields: whose password it lets the bearer set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordReset {
    pub user_id: Uuid,
    pub expires_at: DateTime<Utc>,
}

/// Pending password-reset links triggered by an admin. Only the hash is stored. Like `UserInvitationPort` without
/// `renew` and `expiries`: every reset issues a fresh link.
#[async_trait]
pub trait PasswordResetPort: Send + Sync {
    /// Replaces any existing reset link of the user (one live link per user): a second admin reset kills the first
    /// link instead of leaving two valid ones in the wild.
    async fn replace(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), DomainError>;
    /// Atomic single use: `Some` only for a token that exists and has not expired; the row is deleted.
    async fn consume(&self, token_hash: &str) -> Result<Option<PasswordReset>, DomainError>;
    /// Puts back a link that was consumed but could not be used (the password write failed), but only if the user has
    /// no link at all by now. Returns `true` only if it was put back. Unlike `replace` it never overwrites: an admin
    /// who issued a new reset in the meantime must not see the older link revived over theirs.
    async fn restore(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<bool, DomainError>;
    /// Whether the user has a reset link at all, expired or not. An admin reset scrambles the password until the link
    /// is used (and `consume` deletes the row), so a row means the user cannot sign in. `SetAdminUseCase` must not
    /// count such an admin toward the "never zero admins" floor.
    async fn is_pending(&self, user_id: Uuid) -> Result<bool, DomainError>;
}
