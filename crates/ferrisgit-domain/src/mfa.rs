use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::DomainError;

/// Only a confirmed credential counts at login. Unconfirmed means enrolled but not verified yet.
#[derive(Clone, PartialEq, Eq)]
pub struct TotpCredential {
    pub user_id: Uuid,
    /// Plaintext base32 here, the adapter encrypts at rest.
    pub secret: String,
    pub confirmed: bool,
    /// Anti-replay: a code is only accepted for a step strictly greater than this.
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
    /// Inserts or replaces the credential as given, but never overwrites a confirmed one. Returns `false` if nothing was
    /// written.
    async fn upsert(&self, credential: &TotpCredential) -> Result<bool, DomainError>;
    /// Atomic compare-and-swap: only advances when `step` is greater than the stored one. `false` means a concurrent call
    /// already took that step.
    async fn set_last_used_step(&self, user_id: Uuid, step: i64) -> Result<bool, DomainError>;
    /// Confirms only an unconfirmed credential whose `last_used_step` is `expected_step`, i.e. the one whose code was just
    /// verified. `false` means it was replaced, reset or already confirmed.
    async fn confirm(&self, user_id: Uuid, expected_step: i64) -> Result<bool, DomainError>;
    async fn delete(&self, user_id: Uuid) -> Result<(), DomainError>;
    /// Which users have a confirmed credential. Never decrypts a secret, so it's cheap over a whole list.
    async fn confirmed_user_ids(&self, user_ids: &[Uuid]) -> Result<Vec<Uuid>, DomainError>;
}

#[async_trait]
pub trait BackupCodePort: Send + Sync {
    /// Replaces the whole set, which invalidates every earlier code. Each entry is a salted hash (`<salt-hex>:<digest-hex>`).
    async fn replace_all(&self, user_id: Uuid, code_hashes: &[String]) -> Result<(), DomainError>;
    /// Takes the plaintext: every stored hash has its own salt, so implementations check it against the user's unused
    /// hashes. Atomic, so two concurrent attempts can't both succeed with the same code.
    async fn try_consume(&self, user_id: Uuid, plaintext_code: &str) -> Result<bool, DomainError>;
    async fn count_unused(&self, user_id: Uuid) -> Result<i64, DomainError>;
    async fn delete_all(&self, user_id: Uuid) -> Result<(), DomainError>;
}

/// The claims of a verified `mfa-pending` token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingToken {
    pub user_id: Uuid,
    /// The user's token epoch when issued. The caller rejects the token if it no longer matches.
    pub epoch: i32,
    pub jti: Uuid,
}

/// Short-lived token proving the password step of a login. It only opens the MFA endpoints.
pub trait MfaPendingTokenPort: Send + Sync {
    /// Valid for 5 minutes.
    fn issue(&self, user_id: Uuid, epoch: i32) -> Result<String, DomainError>;
    /// `Unauthorized` on any failure: bad signature, expired, wrong token type.
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
