//! MFA use cases: TOTP, backup codes, admin reset. Passkey ceremonies are in `PasskeyService`.
//! The TOTP secret is plaintext here (the adapter encrypts it at rest). Security events and token-epoch bumps are the
//! caller's job.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Utc;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::mfa::{BackupCodePort, TotpCredential, TotpCredentialPort};
use ferrisgit_domain::user::{PasswordHasherPort, UserRepositoryPort};
use ferrisgit_domain::webauthn::{StoredPasskey, WebauthnCredentialPort};
use uuid::Uuid;
use webauthn_rs::prelude::RegisterPublicKeyCredential;

use super::passkeys::{PasskeyService, invalid_code};
use crate::mfa_crypto;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MfaStatus {
    pub totp_enabled: bool,
    pub backup_codes_remaining: i64,
}

#[derive(Clone, PartialEq, Eq)]
pub struct TotpEnrollment {
    pub secret_base32: String,
    /// The secret is in here too.
    pub otpauth_url: String,
}

impl std::fmt::Debug for TotpEnrollment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TotpEnrollment")
            .field("secret_base32", &"[redacted]")
            .field("otpauth_url", &"[redacted]")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MfaFactors {
    pub has_totp: bool,
    pub has_passkey: bool,
}

impl MfaFactors {
    pub fn any(&self) -> bool {
        self.has_totp || self.has_passkey
    }
}

/// Confirming needs no password, so an abandoned enrolment can't stay confirmable for long.
const PENDING_TOTP_TTL_MINUTES: i64 = 15;

pub struct MfaService {
    pending_ttl: chrono::Duration,
    totp: Arc<dyn TotpCredentialPort>,
    backup: Arc<dyn BackupCodePort>,
    users: Arc<dyn UserRepositoryPort>,
    hasher: Arc<dyn PasswordHasherPort>,
    passkeys: Arc<dyn WebauthnCredentialPort>,
}

impl MfaService {
    pub fn new(
        totp: Arc<dyn TotpCredentialPort>,
        backup: Arc<dyn BackupCodePort>,
        users: Arc<dyn UserRepositoryPort>,
        hasher: Arc<dyn PasswordHasherPort>,
        passkeys: Arc<dyn WebauthnCredentialPort>,
    ) -> Self {
        Self {
            pending_ttl: chrono::Duration::minutes(PENDING_TOTP_TTL_MINUTES),
            totp,
            backup,
            users,
            hasher,
            passkeys,
        }
    }

    #[cfg(test)]
    pub fn with_pending_ttl(mut self, ttl: chrono::Duration) -> Self {
        self.pending_ttl = ttl;
        self
    }

    pub async fn factors(&self, user_id: Uuid) -> Result<MfaFactors, DomainError> {
        Ok(MfaFactors {
            has_totp: self.has_confirmed_totp(user_id).await?,
            has_passkey: self.passkeys.count_for_user(user_id).await? > 0,
        })
    }

    pub async fn status(&self, user_id: Uuid) -> Result<MfaStatus, DomainError> {
        let factors = self.factors(user_id).await?;
        let backup_codes_remaining = if factors.any() {
            self.backup.count_unused(user_id).await?
        } else {
            0
        };
        Ok(MfaStatus {
            totp_enabled: factors.has_totp,
            backup_codes_remaining,
        })
    }

    pub async fn enroll_totp(&self, user_id: Uuid) -> Result<TotpEnrollment, DomainError> {
        let user = self
            .users
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("user".to_string()))?;
        if self.has_confirmed_totp(user_id).await? {
            return Err(DomainError::Validation(
                "TOTP is already enrolled".to_string(),
            ));
        }
        let secret_base32 = mfa_crypto::generate_secret_base32();
        let otpauth_url = mfa_crypto::otpauth_url(&secret_base32, &user.username)?;
        // The store never overwrites a confirmed row, so `false` means one appeared since the check above.
        if !self
            .totp
            .upsert(&TotpCredential {
                user_id,
                secret: secret_base32.clone(),
                confirmed: false,
                last_used_step: None,
                created_at: Utc::now(),
            })
            .await?
        {
            return Err(DomainError::Validation(
                "TOTP is already enrolled".to_string(),
            ));
        }
        Ok(TotpEnrollment {
            secret_base32,
            otpauth_url,
        })
    }

    /// Returns the plaintext backup codes, the only time they exist outside their hashes.
    /// Two racing confirms can't both get codes: the step is claimed first, then `confirm` only matches a row still
    /// unconfirmed at that step. If writing the codes fails, the user regenerates them with the password.
    pub async fn confirm_totp(
        &self,
        user_id: Uuid,
        code: &str,
    ) -> Result<Vec<String>, DomainError> {
        let credential =
            self.totp.get(user_id).await?.ok_or_else(|| {
                DomainError::Validation("no TOTP enrolment to confirm".to_string())
            })?;
        if credential.confirmed {
            return Err(DomainError::Validation(
                "TOTP is already enrolled".to_string(),
            ));
        }
        // A stale enrolment looks like a wrong code, and we don't even check the code.
        if Utc::now() - credential.created_at > self.pending_ttl {
            return Err(DomainError::Validation("invalid code".to_string()));
        }
        let step = mfa_crypto::matching_step(&credential.secret, code, now_unix())
            .ok_or_else(|| DomainError::Validation("invalid code".to_string()))?;
        if credential.last_used_step.is_some_and(|last| step <= last)
            || !self.totp.set_last_used_step(user_id, step).await?
        {
            return Err(DomainError::Validation("invalid code".to_string()));
        }
        if !self.totp.confirm(user_id, step).await? {
            return Err(DomainError::Validation("invalid code".to_string()));
        }
        self.store_new_backup_codes(user_id).await
    }

    /// Every refusal is the same "invalid code", so nothing reveals which factor exists.
    pub async fn verify_challenge(
        &self,
        user_id: Uuid,
        code: Option<&str>,
        backup_code: Option<&str>,
    ) -> Result<(), DomainError> {
        match (code, backup_code) {
            (Some(code), None) => self.verify_totp(user_id, code).await,
            (None, Some(backup_code)) => self.verify_backup(user_id, backup_code).await,
            _ => Err(invalid_code()),
        }
    }

    pub async fn regenerate_backup_codes(
        &self,
        user_id: Uuid,
        current_password: &str,
    ) -> Result<Vec<String>, DomainError> {
        self.check_password(user_id, current_password).await?;
        self.issue_backup_codes(user_id).await
    }

    /// Private on purpose: no password is asked, so callers must have authorised the user already.
    async fn issue_backup_codes(&self, user_id: Uuid) -> Result<Vec<String>, DomainError> {
        if !self.factors(user_id).await?.any() {
            // Leftover codes from a removed factor, they must not come back to life.
            self.backup.delete_all(user_id).await?;
            return Err(DomainError::Validation(
                "no MFA factor is enrolled".to_string(),
            ));
        }
        self.store_new_backup_codes(user_id).await
    }

    async fn store_new_backup_codes(&self, user_id: Uuid) -> Result<Vec<String>, DomainError> {
        let (plaintext, hashes) = mfa_crypto::generate_backup_codes();
        self.backup.replace_all(user_id, &hashes).await?;
        Ok(plaintext)
    }

    /// Registers the first passkey and issues the backup codes in one go, so nobody can mint codes for a user who
    /// already has a factor. Refuses before the ceremony is taken, and removes the passkey again if the codes fail.
    pub async fn finish_first_passkey_setup(
        &self,
        passkeys: &PasskeyService,
        user_id: Uuid,
        challenge_id: Uuid,
        credential: &RegisterPublicKeyCredential,
        name: &str,
    ) -> Result<(StoredPasskey, Vec<String>), DomainError> {
        if self.factors(user_id).await?.any() {
            return Err(DomainError::Validation("MFA is already set up".to_string()));
        }
        self.backup.delete_all(user_id).await?;
        // Any TOTP row left here is an unconfirmed enrolment. Whoever knows its secret could confirm it with just a
        // session and get backup codes.
        self.totp.delete(user_id).await?;
        let stored = passkeys
            .finish_registration(user_id, challenge_id, credential, name)
            .await?;
        match self.issue_backup_codes(user_id).await {
            Ok(codes) => Ok((stored, codes)),
            Err(e) => {
                // Best effort. If this fails too, the user has a passkey and no codes, fixable with the password.
                let _ = self.passkeys.delete(stored.id, user_id).await;
                Err(e)
            }
        }
    }

    /// A wrong password is a 400, not a 401: the client would log the user out over a typo.
    pub async fn check_password(
        &self,
        user_id: Uuid,
        current_password: &str,
    ) -> Result<(), DomainError> {
        let user = self
            .users
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("user".to_string()))?;
        let hasher = self.hasher.clone();
        let password = current_password.to_string();
        let is_valid =
            tokio::task::spawn_blocking(move || hasher.verify(&password, &user.password_hash))
                .await
                .map_err(|e| DomainError::Infrastructure(e.to_string()))??;
        if is_valid {
            Ok(())
        } else {
            Err(DomainError::Validation(
                "current password is incorrect".to_string(),
            ))
        }
    }

    /// Pending enrolments go too. When no factor would remain, the backup codes are deleted first: a failure halfway
    /// then leaves a TOTP without codes, not live codes without a factor. The count is checked again afterwards in case
    /// a passkey was removed at the same time. The caller bumps the token epoch before calling.
    pub async fn remove_totp(&self, user_id: Uuid) -> Result<(), DomainError> {
        let passkey_remains = self.passkeys.count_for_user(user_id).await? > 0;
        if !passkey_remains {
            self.backup.delete_all(user_id).await?;
        }
        self.totp.delete(user_id).await?;
        if passkey_remains && self.passkeys.count_for_user(user_id).await? == 0 {
            self.backup.delete_all(user_id).await?;
        }
        Ok(())
    }

    /// The only way to delete a passkey. Same codes-first ordering and re-check as `remove_totp`, and the caller bumps
    /// the token epoch before calling. Someone else's passkey id is a plain not found.
    pub async fn remove_passkey(&self, user_id: Uuid, passkey_id: Uuid) -> Result<(), DomainError> {
        let owned = self.passkeys.list_for_user(user_id).await?;
        if !owned.iter().any(|passkey| passkey.id == passkey_id) {
            return Err(DomainError::NotFound("passkey".to_string()));
        }
        let another_factor_remains = owned.len() > 1 || self.has_confirmed_totp(user_id).await?;
        if !another_factor_remains {
            self.backup.delete_all(user_id).await?;
        }
        if !self.passkeys.delete(passkey_id, user_id).await? {
            return Err(DomainError::NotFound("passkey".to_string()));
        }
        if another_factor_remains && !self.factors(user_id).await?.any() {
            self.backup.delete_all(user_id).await?;
        }
        Ok(())
    }

    /// Codes first, then passkeys, then TOTP, so a failure halfway never leaves live codes without a factor. The
    /// caller bumps the token epoch before calling.
    pub async fn reset(&self, user_id: Uuid) -> Result<(), DomainError> {
        self.backup.delete_all(user_id).await?;
        self.passkeys.delete_all_for_user(user_id).await?;
        self.totp.delete(user_id).await
    }

    async fn has_confirmed_totp(&self, user_id: Uuid) -> Result<bool, DomainError> {
        Ok(self
            .totp
            .get(user_id)
            .await?
            .is_some_and(|credential| credential.confirmed))
    }

    async fn verify_totp(&self, user_id: Uuid, code: &str) -> Result<(), DomainError> {
        let credential = self
            .totp
            .get(user_id)
            .await?
            .filter(|credential| credential.confirmed)
            .ok_or_else(invalid_code)?;
        let step = mfa_crypto::matching_step(&credential.secret, code, now_unix())
            .ok_or_else(invalid_code)?;
        // A step already used (or older) is a replay. This check alone doesn't stop concurrent attempts, the
        // compare-and-swap does.
        if credential.last_used_step.is_some_and(|last| step <= last)
            || !self.totp.set_last_used_step(user_id, step).await?
        {
            return Err(invalid_code());
        }
        Ok(())
    }

    async fn verify_backup(&self, user_id: Uuid, backup_code: &str) -> Result<(), DomainError> {
        // Backup codes only back up a TOTP or a passkey. With neither (after a reset, say) leftover codes are useless.
        if !self.factors(user_id).await?.any() {
            return Err(invalid_code());
        }
        let normalised = backup_code.trim().to_ascii_lowercase();
        if self.backup.try_consume(user_id, &normalised).await? {
            Ok(())
        } else {
            Err(invalid_code())
        }
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests;
