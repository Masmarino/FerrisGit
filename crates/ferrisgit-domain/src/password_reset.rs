use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::DomainError;

/// A consumed reset token: whose password it lets the bearer set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordReset {
    pub user_id: Uuid,
    pub expires_at: DateTime<Utc>,
}

/// Pending admin-triggered reset links. Only the hash is stored. Like the invitation port minus `renew` and `expiries`,
/// since every reset issues a fresh link.
#[async_trait]
pub trait PasswordResetPort: Send + Sync {
    /// Replaces the user's reset link, if any, so a second admin reset kills the first instead of leaving two valid ones.
    async fn replace(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), DomainError>;
    /// Single use: returns the reset only if the token exists and hasn't expired, and deletes it.
    async fn consume(&self, token_hash: &str) -> Result<Option<PasswordReset>, DomainError>;
    /// Puts back a consumed link whose password write failed, but only if the user has no link by now, so it never revives
    /// an old link over one an admin issued in the meantime. Returns `true` if it was put back.
    async fn restore(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<bool, DomainError>;
    /// Whether the user has a reset link at all, expired or not. An admin reset scrambles the password until the link is
    /// used, so a pending row means the user can't sign in and `SetAdminUseCase` shouldn't count them as an admin.
    async fn is_pending(&self, user_id: Uuid) -> Result<bool, DomainError>;
}
