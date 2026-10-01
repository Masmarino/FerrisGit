use std::sync::RwLock;

use chrono::{Duration, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::user::TokenIssuerPort;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Fixed `iss`/`aud`. On their own they don't scope audiences, but a token minted elsewhere with the
/// same HS256 secret is rejected unless it carries these claims too.
const ISSUER: &str = "ferrisgit";
const AUDIENCE: &str = "ferrisgit-api";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Claims {
    sub: Uuid,
    exp: i64,
    epoch: i32,
    iss: String,
    aud: String,
}

pub struct JwtTokenIssuer {
    secret: String,
    ttl: RwLock<Duration>,
}

impl JwtTokenIssuer {
    pub fn new(secret: String) -> Self {
        Self {
            secret,
            ttl: RwLock::new(Duration::hours(12)),
        }
    }
}

impl TokenIssuerPort for JwtTokenIssuer {
    fn issue(&self, user_id: Uuid, token_epoch: i32) -> Result<String, DomainError> {
        // Recover from poison: nothing run under this lock can panic, so poison only means another panic already happened.
        let ttl = *self
            .ttl
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let claims = Claims {
            sub: user_id,
            exp: (Utc::now() + ttl).timestamp(),
            epoch: token_epoch,
            iss: ISSUER.to_string(),
            aud: AUDIENCE.to_string(),
        };
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )
        .map_err(|e| DomainError::Infrastructure(e.to_string()))
    }

    fn verify(&self, token: &str) -> Result<(Uuid, i32), DomainError> {
        let mut validation = Validation::default();
        validation.set_issuer(&[ISSUER]);
        validation.set_audience(&[AUDIENCE]);
        decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &validation,
        )
        .map(|data| (data.claims.sub, data.claims.epoch))
        .map_err(|_| DomainError::Unauthorized("invalid or expired token".to_string()))
    }

    fn set_ttl_hours(&self, hours: i64) {
        *self
            .ttl
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Duration::hours(hours);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issuing_then_verifying_returns_the_same_user_id_and_epoch() {
        let issuer = JwtTokenIssuer::new("test-secret".to_string());
        let user_id = Uuid::new_v4();
        let token = issuer.issue(user_id, 3).unwrap();
        assert_eq!(issuer.verify(&token).unwrap(), (user_id, 3));
    }

    #[test]
    fn verifying_a_token_signed_with_a_different_secret_fails() {
        let issuer_a = JwtTokenIssuer::new("secret-a".to_string());
        let issuer_b = JwtTokenIssuer::new("secret-b".to_string());
        let token = issuer_a.issue(Uuid::new_v4(), 0).unwrap();
        assert!(issuer_b.verify(&token).is_err());
    }

    #[test]
    fn verifying_a_token_with_the_wrong_issuer_or_audience_fails_even_with_the_right_secret() {
        let issuer = JwtTokenIssuer::new("test-secret".to_string());
        let key = EncodingKey::from_secret(b"test-secret");

        let wrong_issuer = Claims {
            sub: Uuid::new_v4(),
            exp: (Utc::now() + Duration::hours(1)).timestamp(),
            epoch: 0,
            iss: "someone-else".to_string(),
            aud: AUDIENCE.to_string(),
        };
        let token = encode(&Header::default(), &wrong_issuer, &key).unwrap();
        assert!(issuer.verify(&token).is_err());

        let wrong_audience = Claims {
            sub: Uuid::new_v4(),
            exp: (Utc::now() + Duration::hours(1)).timestamp(),
            epoch: 0,
            iss: ISSUER.to_string(),
            aud: "someone-else".to_string(),
        };
        let token = encode(&Header::default(), &wrong_audience, &key).unwrap();
        assert!(issuer.verify(&token).is_err());
    }

    #[test]
    fn set_ttl_hours_changes_the_expiry_of_subsequently_issued_tokens() {
        let issuer = JwtTokenIssuer::new("test-secret".to_string());
        issuer.set_ttl_hours(1);

        let token = issuer.issue(Uuid::new_v4(), 0).unwrap();

        let mut validation = Validation::default();
        validation.set_audience(&[AUDIENCE]);
        let decoded = jsonwebtoken::decode::<Claims>(
            &token,
            &DecodingKey::from_secret(b"test-secret"),
            &validation,
        )
        .unwrap();
        let expected_exp = (Utc::now() + Duration::hours(1)).timestamp();
        assert!(
            (decoded.claims.exp - expected_exp).abs() < 5,
            "expected exp near {expected_exp}, got {}",
            decoded.claims.exp
        );
    }

    #[test]
    fn tokens_issued_at_different_epochs_carry_their_own_epoch() {
        let issuer = JwtTokenIssuer::new("test-secret".to_string());
        let user_id = Uuid::new_v4();

        let old_token = issuer.issue(user_id, 0).unwrap();
        let new_token = issuer.issue(user_id, 1).unwrap();

        assert_eq!(issuer.verify(&old_token).unwrap(), (user_id, 0));
        assert_eq!(issuer.verify(&new_token).unwrap(), (user_id, 1));
    }
}
