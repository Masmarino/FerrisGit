use chrono::{Duration, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::mfa::{MfaPendingTokenPort, PendingToken};
use hkdf::Hkdf;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

const MFA_PENDING_TOKEN_TYPE: &str = "mfa-pending";
const HKDF_INFO: &[u8] = b"ferrisgit-mfa-pending-v1";
const TTL_MINUTES: i64 = 5;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Claims {
    sub: Uuid,
    exp: i64,
    iat: i64,
    typ: String,
    /// The user's token epoch when the password step succeeded; the caller rejects the token once it differs.
    epoch: i32,
    /// Makes two tokens minted in the same second differ, so single-use tracking can tell them apart.
    jti: Uuid,
}

/// Signs `mfa-pending` tokens with a key derived from `JWT_SECRET` (HKDF-SHA256, its own info string).
/// Session tokens are signed with the raw secret, so neither kind of token can verify as the other.
pub struct JwtMfaPendingTokenIssuer {
    key: [u8; 32],
}

impl JwtMfaPendingTokenIssuer {
    pub fn new(jwt_secret: String) -> Self {
        let mut key = [0u8; 32];
        Hkdf::<Sha256>::new(None, jwt_secret.as_bytes())
            .expand(HKDF_INFO, &mut key)
            .expect("32 bytes is a valid HKDF-SHA256 output length");
        Self { key }
    }

    fn validation() -> Validation {
        let mut validation = Validation::default();
        validation.set_required_spec_claims(&["exp", "sub"]);
        validation.leeway = 0;
        validation
    }
}

impl MfaPendingTokenPort for JwtMfaPendingTokenIssuer {
    fn issue(&self, user_id: Uuid, epoch: i32) -> Result<String, DomainError> {
        let now = Utc::now();
        let claims = Claims {
            sub: user_id,
            exp: (now + Duration::minutes(TTL_MINUTES)).timestamp(),
            iat: now.timestamp(),
            typ: MFA_PENDING_TOKEN_TYPE.to_string(),
            epoch,
            jti: Uuid::new_v4(),
        };
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&self.key),
        )
        .map_err(|e| DomainError::Infrastructure(e.to_string()))
    }

    fn verify(&self, token: &str) -> Result<PendingToken, DomainError> {
        let invalid = || DomainError::Unauthorized("invalid or expired token".to_string());
        let data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(&self.key),
            &Self::validation(),
        )
        .map_err(|_| invalid())?;
        if data.claims.typ != MFA_PENDING_TOKEN_TYPE {
            return Err(invalid());
        }
        Ok(PendingToken {
            user_id: data.claims.sub,
            epoch: data.claims.epoch,
            jti: data.claims.jti,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jwt_token_issuer::JwtTokenIssuer;
    use ferrisgit_domain::user::TokenIssuerPort;

    fn claims(typ: &str, exp: i64) -> Claims {
        Claims {
            sub: Uuid::new_v4(),
            exp,
            iat: Utc::now().timestamp(),
            typ: typ.to_string(),
            epoch: 0,
            jti: Uuid::new_v4(),
        }
    }

    fn sign(issuer: &JwtMfaPendingTokenIssuer, claims: &Claims) -> String {
        encode(
            &Header::default(),
            claims,
            &EncodingKey::from_secret(&issuer.key),
        )
        .unwrap()
    }

    fn assert_unauthorized(result: Result<PendingToken, DomainError>) {
        assert!(
            matches!(result, Err(DomainError::Unauthorized(_))),
            "expected Unauthorized, got {result:?}"
        );
    }

    #[test]
    fn issuing_then_verifying_returns_the_user_id_the_epoch_and_a_jti() {
        let issuer = JwtMfaPendingTokenIssuer::new("test-secret".to_string());
        let user_id = Uuid::new_v4();

        let pending = issuer.verify(&issuer.issue(user_id, 4).unwrap()).unwrap();

        assert_eq!(pending.user_id, user_id);
        assert_eq!(pending.epoch, 4);
        assert!(!pending.jti.is_nil());
    }

    #[test]
    fn the_token_lives_five_minutes() {
        let issuer = JwtMfaPendingTokenIssuer::new("test-secret".to_string());
        let token = issuer.issue(Uuid::new_v4(), 0).unwrap();

        let decoded = decode::<Claims>(
            &token,
            &DecodingKey::from_secret(&issuer.key),
            &JwtMfaPendingTokenIssuer::validation(),
        )
        .unwrap();

        assert_eq!(decoded.claims.typ, "mfa-pending");
        assert_eq!(decoded.claims.exp - decoded.claims.iat, 300);
    }

    #[test]
    fn two_tokens_for_the_same_user_differ_and_carry_different_jtis() {
        let issuer = JwtMfaPendingTokenIssuer::new("test-secret".to_string());
        let user_id = Uuid::new_v4();
        let first = issuer.issue(user_id, 0).unwrap();
        let second = issuer.issue(user_id, 0).unwrap();

        assert_ne!(first, second);
        assert_ne!(
            issuer.verify(&first).unwrap().jti,
            issuer.verify(&second).unwrap().jti
        );
    }

    #[test]
    fn an_expired_token_is_rejected() {
        let issuer = JwtMfaPendingTokenIssuer::new("test-secret".to_string());
        let token = sign(
            &issuer,
            &claims(MFA_PENDING_TOKEN_TYPE, Utc::now().timestamp() - 1),
        );

        assert_unauthorized(issuer.verify(&token));
    }

    #[test]
    fn a_token_signed_with_another_secret_is_rejected() {
        let token = JwtMfaPendingTokenIssuer::new("secret-a".to_string())
            .issue(Uuid::new_v4(), 0)
            .unwrap();

        assert_unauthorized(JwtMfaPendingTokenIssuer::new("secret-b".to_string()).verify(&token));
    }

    #[test]
    fn a_session_token_from_the_same_secret_is_rejected_here() {
        let secret = "shared-secret".to_string();
        let session_token = JwtTokenIssuer::new(secret.clone())
            .issue(Uuid::new_v4(), 0)
            .unwrap();

        assert_unauthorized(JwtMfaPendingTokenIssuer::new(secret).verify(&session_token));
    }

    #[test]
    fn a_pending_token_is_rejected_by_the_session_issuer() {
        let secret = "shared-secret".to_string();
        let pending_token = JwtMfaPendingTokenIssuer::new(secret.clone())
            .issue(Uuid::new_v4(), 0)
            .unwrap();

        assert!(
            JwtTokenIssuer::new(secret).verify(&pending_token).is_err(),
            "an mfa-pending token must not verify as a session token"
        );
    }

    #[test]
    fn a_token_signed_with_the_raw_secret_instead_of_the_derived_key_is_rejected() {
        let token = encode(
            &Header::default(),
            &claims(MFA_PENDING_TOKEN_TYPE, Utc::now().timestamp() + 300),
            &EncodingKey::from_secret(b"test-secret"),
        )
        .unwrap();

        assert_unauthorized(
            JwtMfaPendingTokenIssuer::new("test-secret".to_string()).verify(&token),
        );
    }

    #[test]
    fn a_token_with_the_right_key_but_another_type_is_rejected() {
        let issuer = JwtMfaPendingTokenIssuer::new("test-secret".to_string());
        let token = sign(&issuer, &claims("session", Utc::now().timestamp() + 300));

        assert_unauthorized(issuer.verify(&token));
    }

    #[test]
    fn a_token_with_an_unknown_claim_is_rejected() {
        #[derive(Serialize)]
        struct Extra {
            sub: Uuid,
            exp: i64,
            iat: i64,
            typ: &'static str,
            epoch: i32,
            jti: Uuid,
            admin: bool,
        }
        let issuer = JwtMfaPendingTokenIssuer::new("test-secret".to_string());
        let extra = Extra {
            sub: Uuid::new_v4(),
            exp: Utc::now().timestamp() + 300,
            iat: Utc::now().timestamp(),
            typ: MFA_PENDING_TOKEN_TYPE,
            epoch: 0,
            jti: Uuid::new_v4(),
            admin: true,
        };
        let token = encode(
            &Header::default(),
            &extra,
            &EncodingKey::from_secret(&issuer.key),
        )
        .unwrap();

        assert_unauthorized(issuer.verify(&token));
    }

    #[test]
    fn garbage_is_rejected() {
        let issuer = JwtMfaPendingTokenIssuer::new("test-secret".to_string());

        assert_unauthorized(issuer.verify("not-a-jwt"));
        assert_unauthorized(issuer.verify(""));
        assert_unauthorized(issuer.verify("a.b.c"));
    }

    // Produced by hkdf 0.12. The key derivation must not change, or pending tokens issued before an upgrade
    // would stop verifying.
    #[test]
    fn the_signing_key_derivation_matches_the_previous_hkdf_version() {
        let issuer = JwtMfaPendingTokenIssuer::new(
            "a-jwt-secret-of-at-least-32-characters-long".to_string(),
        );
        assert_eq!(
            hex::encode(issuer.key),
            "edcc120c9f1d1e0693343bc5e85fa9a079d00976abbce914f910e1160df51c88"
        );
    }
}
