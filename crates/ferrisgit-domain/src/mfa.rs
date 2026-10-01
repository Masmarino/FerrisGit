use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::DomainError;

/// `confirmed = false` means enrolled but not yet verified: only a confirmed credential is consulted at login.
#[derive(Clone, PartialEq, Eq)]
pub struct TotpCredential {
    pub user_id: Uuid,
    /// Plaintext base32 at this layer; encryption at rest is the persistence adapter's job.
    pub secret: String,
    pub confirmed: bool,
    /// Anti-replay: a code is only accepted when its step is strictly greater than this.
    pub last_used_step: Option<i64>,
    pub created_at: DateTime<Utc>,
}

impl std::fmt::Debug for TotpCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TotpCredential")
            .field("user_id", &self.user_id)
            .field("secret", &"[redacted]")
            .field("confirmed", &self.confirmed)
            .field("last_used_step", &self.last_used_step)
            .field("created_at", &self.created_at)
            .finish()
    }
}

#[async_trait]
pub trait TotpCredentialPort: Send + Sync {
    async fn get(&self, user_id: Uuid) -> Result<Option<TotpCredential>, DomainError>;
    /// Inserts or replaces the user's credential as given, `confirmed` and `last_used_step` included, but never
    /// overwrites a row that is already `confirmed`. Returns `false` when it did not write (for example
    /// `ON CONFLICT (user_id) DO UPDATE ... WHERE totp_credentials.confirmed = false`).
    async fn upsert(&self, credential: &TotpCredential) -> Result<bool, DomainError>;
    /// Atomic compare-and-swap: advances `last_used_step` only when `step` is strictly greater than the
    /// stored one (or none is stored). `false` means a concurrent call already claimed that step.
    async fn set_last_used_step(&self, user_id: Uuid, step: i64) -> Result<bool, DomainError>;
    /// Marks the credential confirmed only if the row is still unconfirmed and its `last_used_step` equals
    /// `expected_step` (for example
    /// `UPDATE ... SET confirmed = true WHERE user_id = $1 AND confirmed = false AND last_used_step = $2`). So it can
    /// only confirm the credential whose code was just verified. `false` means the row was replaced, reset or already
    /// confirmed.
    async fn confirm(&self, user_id: Uuid, expected_step: i64) -> Result<bool, DomainError>;
    async fn delete(&self, user_id: Uuid) -> Result<(), DomainError>;
    /// Which of these users have a confirmed credential. Never decrypts a secret: it is a plain existence check,
    /// safe to run over a whole user list.
    async fn confirmed_user_ids(&self, user_ids: &[Uuid]) -> Result<Vec<Uuid>, DomainError>;
}

#[async_trait]
pub trait BackupCodePort: Send + Sync {
    /// Replaces the whole set, invalidating every prior code. Each entry is an opaque, individually
    /// salted hash (`<salt-hex>:<digest-hex>`), never directly comparable to a plaintext.
    async fn replace_all(&self, user_id: Uuid, code_hashes: &[String]) -> Result<(), DomainError>;
    /// Takes the plaintext candidate: each stored hash carries its own salt, so implementations fetch the
    /// user's unused hashes and verify the plaintext against each. Atomic: `true` only if a matching,
    /// still-unused code existed and was consumed, so two concurrent attempts cannot both succeed.
    async fn try_consume(&self, user_id: Uuid, plaintext_code: &str) -> Result<bool, DomainError>;
    async fn count_unused(&self, user_id: Uuid) -> Result<i64, DomainError>;
    async fn delete_all(&self, user_id: Uuid) -> Result<(), DomainError>;
}

/// What a verified `mfa-pending` token carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingToken {
    pub user_id: Uuid,
    /// The user's token epoch at issuance; the caller rejects the token when it differs from the current one.
    pub epoch: i32,
    pub jti: Uuid,
}

/// Short-lived, single-purpose token proving the password step of a login; it authorises the MFA endpoints only.
pub trait MfaPendingTokenPort: Send + Sync {
    /// Fixed 5-minute lifetime.
    fn issue(&self, user_id: Uuid, epoch: i32) -> Result<String, DomainError>;
    /// `Unauthorized` on any failure (bad signature, expired, wrong token type).
    fn verify(&self, token: &str) -> Result<PendingToken, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_never_contains_the_totp_secret() {
        let credential = TotpCredential {
            user_id: Uuid::new_v4(),
            secret: "TOTPSEEDVALUE".to_string(),
            confirmed: true,
            last_used_step: None,
            created_at: Utc::now(),
        };

        let debug = format!("{credential:?}");

        assert!(!debug.contains("TOTPSEEDVALUE"));
        assert!(debug.contains("[redacted]"));
    }
}
