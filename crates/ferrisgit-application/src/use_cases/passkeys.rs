//! Passkeys (WebAuthn) as a second factor after the password, with the non-discoverable flow.
//! `webauthn-rs` runs the ceremonies, this adds single-use ceremonies bound to a user and the storage of credentials
//! and counters. A failed assertion looks like a wrong TOTP code, so nothing reveals which factor exists.
//! Like `MfaService`, security events and token-epoch bumps are the caller's job.

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
/// Caps the ceremony states (each copies the user's credentials) and the credential lists sent to the browser.
const MAX_PASSKEYS_PER_USER: i64 = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
struct RelyingParty {
    /// The RP ID: host only, no scheme or port.
    pub id: String,
    /// As the browser reports it, with the port when it isn't the default.
    pub origin: Url,
}

/// `None` (with a warning) when the public URL can't carry passkeys: not http(s), an IP host (RP ids must be domain
/// names), or plain http anywhere but localhost (browsers only allow WebAuthn in a secure context).
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

/// `None` (with a warning) when passkeys can't work here. The passkey routes then answer 503 and TOTP carries on.
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

    /// Listing and deleting stored passkeys works even when this is `false`.
    pub fn available(&self) -> bool {
        self.webauthn.is_some()
    }

    fn client(&self) -> Result<&Webauthn, DomainError> {
        self.webauthn.as_deref().ok_or_else(|| {
            DomainError::ServiceUnavailable("passkeys are not available on this server".to_string())
        })
    }

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

    /// Name and cap are checked before the ceremony is consumed. A credential id that already exists is a conflict and
    /// is never overwritten.
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

    /// The ceremony is spent whatever happens. If saving the new counter fails the login fails too, otherwise the clone
    /// check would be useless.
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
        // The library only compared with the counter the ceremony started with, which is stale if another login was
        // saved since. Check the saved one too to catch a cloned key. Synced passkeys always report 0, so skip those.
        let stored_counter = stored_counter(row)?;
        if (result.counter() != 0 || stored_counter != 0)
            && u64::from(result.counter()) <= stored_counter
        {
            return Err(invalid_code());
        }
        let mut passkey = deserialize_passkey(row)?;
        // Returns whether anything changed. Ignored: `last_used_at` is recorded either way.
        passkey.update_credential(&result);
        self.credentials
            .update_after_authentication(row.id, &serialize_passkey(&passkey)?)
            .await
    }

    pub async fn list(&self, user_id: Uuid) -> Result<Vec<StoredPasskey>, DomainError> {
        self.credentials.list_for_user(user_id).await
    }

    // No `delete` on purpose: dropping the last factor must also drop the backup codes, which `MfaService::remove_passkey`
    // does.
}

pub(super) fn invalid_code() -> DomainError {
    DomainError::Unauthorized("invalid code".to_string())
}

fn invalid_passkey() -> DomainError {
    DomainError::Validation("invalid passkey".to_string())
}

/// 1 to 40 characters once trimmed, no control characters.
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

/// Fails closed on an unexpected layout, an unreadable counter would defeat the clone check.
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

/// The error deliberately leaves out the stored JSON.
fn deserialize_passkey(stored: &StoredPasskey) -> Result<Passkey, DomainError> {
    serde_json::from_str(&stored.passkey_json).map_err(|_| {
        DomainError::Infrastructure(format!("stored passkey {} is unreadable", stored.id))
    })
}

#[cfg(test)]
mod tests;
