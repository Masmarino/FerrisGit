//! Passkeys (WebAuthn) as an MFA factor: registration, login assertion, listing and deletion.
//! `webauthn-rs` runs the ceremonies. This service adds single-use ceremonies bound to a user, persistence of
//! credentials and counters, and the API error semantics: any failed assertion is the same
//! `Unauthorized("invalid code")` as a wrong TOTP code, so nothing reveals which factor exists. Passkeys are a second
//! factor after the password (non-discoverable flow).
//! Security events and token-epoch bumps are left to the caller, like in `MfaService`.

use std::sync::Arc;

use chrono::Utc;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::user::UserRepositoryPort;
use ferrisgit_domain::webauthn::{StoredPasskey, WebauthnCredentialPort};
use url::{Host, Url};
use uuid::Uuid;
use webauthn_rs::prelude::*;

use crate::passkey_ceremonies::{CeremonyState, PasskeyCeremonies};

const RELYING_PARTY_NAME: &str = "FerrisGit";
const MAX_NAME_CHARS: usize = 40;
/// Bounds the size of every ceremony state (each one copies the user's credentials) and of the
/// `allowCredentials` / `excludeCredentials` lists sent to the browser.
const MAX_PASSKEYS_PER_USER: i64 = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
struct RelyingParty {
    /// The registrable host, no scheme and no port (WebAuthn's "RP ID").
    pub id: String,
    /// Scheme, host and any non-default port, exactly as the browser reports it.
    pub origin: Url,
}

/// `None` (with a warning) when the public URL cannot carry passkeys: not a URL, not http(s), an IP-literal host
/// (relying-party ids must be domain names), or plain `http` on a host other than `localhost` / `*.localhost`
/// (browsers only offer WebAuthn in a secure context).
fn relying_party(public_url: &str) -> Option<RelyingParty> {
    let url = Url::parse(public_url)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"));
    let Some(url) = url else {
        tracing::warn!("the public URL is not an http(s) URL: passkeys are unavailable");
        return None;
    };
    let id = match url.host() {
        Some(Host::Domain(domain))
            if url.scheme() == "http"
                && domain != "localhost"
                && !domain.ends_with(".localhost") =>
        {
            tracing::warn!(
                "the public URL is plain http on a host other than localhost: browsers refuse WebAuthn there, passkeys are unavailable"
            );
            return None;
        }
        Some(Host::Domain(domain)) => domain.to_string(),
        Some(_) => {
            tracing::warn!(
                "the public URL host is an IP address: passkeys are unavailable (WebAuthn needs a domain name)"
            );
            return None;
        }
        None => {
            tracing::warn!("the public URL has no host: passkeys are unavailable");
            return None;
        }
    };
    let origin = Url::parse(&url.origin().ascii_serialization()).ok()?;
    Some(RelyingParty { id, origin })
}

/// `None` (with a warning) when passkeys are impossible here: every passkey route then answers 503 while TOTP keeps
/// working.
pub fn build_webauthn(public_url: &str) -> Option<Webauthn> {
    let relying_party = relying_party(public_url)?;
    match WebauthnBuilder::new(&relying_party.id, &relying_party.origin)
        .and_then(|builder| builder.rp_name(RELYING_PARTY_NAME).build())
    {
        Ok(webauthn) => Some(webauthn),
        Err(e) => {
            tracing::warn!(error = %e, rp_id = %relying_party.id, "the public URL cannot be used as a WebAuthn relying party: passkeys are unavailable");
            None
        }
    }
}

pub struct PasskeyService {
    webauthn: Option<Arc<Webauthn>>,
    credentials: Arc<dyn WebauthnCredentialPort>,
    ceremonies: Arc<PasskeyCeremonies>,
    users: Arc<dyn UserRepositoryPort>,
}

impl PasskeyService {
    pub fn new(
        webauthn: Option<Arc<Webauthn>>,
        credentials: Arc<dyn WebauthnCredentialPort>,
        ceremonies: Arc<PasskeyCeremonies>,
        users: Arc<dyn UserRepositoryPort>,
    ) -> Self {
        Self {
            webauthn,
            credentials,
            ceremonies,
            users,
        }
    }

    /// `false` for an IP-literal public URL. Listing and deleting stored passkeys never needs it.
    pub fn available(&self) -> bool {
        self.webauthn.is_some()
    }

    fn client(&self) -> Result<&Webauthn, DomainError> {
        self.webauthn.as_deref().ok_or_else(|| {
            DomainError::ServiceUnavailable("passkeys are not available on this server".to_string())
        })
    }

    /// The challenge excludes the user's stored credentials.
    pub async fn start_registration(
        &self,
        user_id: Uuid,
    ) -> Result<(Uuid, CreationChallengeResponse), DomainError> {
        let webauthn = self.client()?;
        let user = self
            .users
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("user".to_string()))?;
        let stored = self.credentials.list_for_user(user_id).await?;
        ensure_below_cap(stored.len() as i64)?;
        let excluded: Vec<CredentialID> = stored
            .into_iter()
            .map(|stored| CredentialID::from(stored.credential_id))
            .collect();
        let (challenge, registration) = webauthn
            .start_passkey_registration(
                user_id,
                &user.username,
                &user.username,
                if excluded.is_empty() {
                    None
                } else {
                    Some(excluded)
                },
            )
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        let challenge_id = self
            .ceremonies
            .put(user_id, CeremonyState::Registration(registration));
        Ok((challenge_id, challenge))
    }

    /// The name and per-user cap are checked before the ceremony is consumed. A failed ceremony is the generic
    /// `Validation("invalid passkey")`. An existing credential id is a `Conflict` and is never overwritten.
    pub async fn finish_registration(
        &self,
        user_id: Uuid,
        challenge_id: Uuid,
        credential: &RegisterPublicKeyCredential,
        name: &str,
    ) -> Result<StoredPasskey, DomainError> {
        let webauthn = self.client()?;
        let name = validated_name(name)?;
        ensure_below_cap(self.credentials.count_for_user(user_id).await?)?;
        let Some(CeremonyState::Registration(registration)) =
            self.ceremonies.take(challenge_id, user_id)
        else {
            return Err(invalid_passkey());
        };
        let passkey = webauthn
            .finish_passkey_registration(credential, &registration)
            .map_err(|_| invalid_passkey())?;
        let stored = StoredPasskey {
            id: Uuid::new_v4(),
            user_id,
            name,
            credential_id: passkey.cred_id().as_ref().to_vec(),
            passkey_json: serialize_passkey(&passkey)?,
            created_at: Utc::now(),
            last_used_at: None,
        };
        if !self.credentials.insert(&stored).await? {
            return Err(DomainError::Conflict(
                "this passkey is already registered".to_string(),
            ));
        }
        Ok(stored)
    }

    pub async fn start_authentication(
        &self,
        user_id: Uuid,
    ) -> Result<(Uuid, RequestChallengeResponse), DomainError> {
        let webauthn = self.client()?;
        let stored = self.credentials.list_for_user(user_id).await?;
        if stored.is_empty() {
            return Err(DomainError::Validation(
                "no passkey is registered".to_string(),
            ));
        }
        let passkeys = stored
            .iter()
            .map(deserialize_passkey)
            .collect::<Result<Vec<_>, _>>()?;
        let (challenge, authentication) = webauthn
            .start_passkey_authentication(&passkeys)
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        let challenge_id = self
            .ceremonies
            .put(user_id, CeremonyState::Authentication(authentication));
        Ok((challenge_id, challenge))
    }

    /// Every failure is the same `Unauthorized("invalid code")` and the ceremony is spent either way. On success the
    /// counter and backup state are saved along with `last_used_at`. If that write fails, the login fails: a counter
    /// that is not recorded would defeat the clone check.
    pub async fn finish_authentication(
        &self,
        user_id: Uuid,
        challenge_id: Uuid,
        credential: &PublicKeyCredential,
    ) -> Result<(), DomainError> {
        let webauthn = self.client()?;
        let Some(CeremonyState::Authentication(authentication)) =
            self.ceremonies.take(challenge_id, user_id)
        else {
            return Err(invalid_code());
        };
        let result = webauthn
            .finish_passkey_authentication(credential, &authentication)
            .map_err(|_| invalid_code())?;
        let stored = self.credentials.list_for_user(user_id).await?;
        let Some(row) = stored
            .iter()
            .find(|row| row.credential_id.as_slice() == result.cred_id().as_ref())
        else {
            return Err(invalid_code());
        };
        // The library compared against the counter the ceremony held, and a ceremony started before another login was
        // saved holds an older one. Compare against the saved counter too, to catch a cloned key. Synced passkeys
        // always report 0 and are exempt.
        let stored_counter = stored_counter(row)?;
        if (result.counter() != 0 || stored_counter != 0)
            && u64::from(result.counter()) <= stored_counter
        {
            return Err(invalid_code());
        }
        let mut passkey = deserialize_passkey(row)?;
        // `Some(false)` (nothing changed, typical of synced passkeys) still records `last_used_at` below.
        passkey.update_credential(&result);
        self.credentials
            .update_after_authentication(row.id, &serialize_passkey(&passkey)?)
            .await
    }

    pub async fn list(&self, user_id: Uuid) -> Result<Vec<StoredPasskey>, DomainError> {
        self.credentials.list_for_user(user_id).await
    }

    // There is no `delete` here on purpose: removing a passkey must also revoke the backup codes when it was the last
    // factor, which is `MfaService::remove_passkey`'s job (it owns both ports).
}

pub(super) fn invalid_code() -> DomainError {
    DomainError::Unauthorized("invalid code".to_string())
}

fn invalid_passkey() -> DomainError {
    DomainError::Validation("invalid passkey".to_string())
}

/// 1 to 40 characters once trimmed, no control characters (newlines and escape sequences included).
fn validated_name(name: &str) -> Result<String, DomainError> {
    let name = name.trim();
    if name.is_empty()
        || name.chars().count() > MAX_NAME_CHARS
        || name.chars().any(char::is_control)
    {
        return Err(DomainError::Validation("invalid passkey name".to_string()));
    }
    Ok(name.to_string())
}

fn ensure_below_cap(existing: i64) -> Result<(), DomainError> {
    if existing >= MAX_PASSKEYS_PER_USER {
        Err(DomainError::Validation("too many passkeys".to_string()))
    } else {
        Ok(())
    }
}

/// Fails closed when the stored layout is unexpected: an unreadable counter would defeat the clone check.
fn stored_counter(stored: &StoredPasskey) -> Result<u64, DomainError> {
    serde_json::from_str::<serde_json::Value>(&stored.passkey_json)
        .ok()
        .and_then(|json| json["cred"]["counter"].as_u64())
        .ok_or_else(|| {
            DomainError::Infrastructure(format!(
                "stored passkey {} has no readable signature counter",
                stored.id
            ))
        })
}

fn serialize_passkey(passkey: &Passkey) -> Result<String, DomainError> {
    serde_json::to_string(passkey)
        .map_err(|e| DomainError::Infrastructure(format!("cannot serialize a passkey: {e}")))
}

/// The error never carries the stored JSON.
fn deserialize_passkey(stored: &StoredPasskey) -> Result<Passkey, DomainError> {
    serde_json::from_str(&stored.passkey_json).map_err(|_| {
        DomainError::Infrastructure(format!("stored passkey {} is unreadable", stored.id))
    })
}

#[cfg(test)]
mod tests;
