use std::sync::Arc;

use chrono::{Duration, Utc};
use ferrisgit_domain::access_token::{
    ACCESS_TOKEN_PREFIX, AccessToken, AccessTokenRepositoryPort, DeviceAuthorization,
    DeviceAuthorizationPort, DevicePoll, NewAccessToken, NewDeviceAuthorization, TokenScope,
    ensure_owner_can_hold, validate_lifetime_days, validate_token_name,
};
use ferrisgit_domain::error::DomainError;
use rand::{Rng, RngExt};
use uuid::Uuid;

use crate::token_hash::hash_token;

/// How long a terminal waits for the user to confirm.
pub const DEVICE_REQUEST_TTL_MINUTES: i64 = 15;
/// Seconds a terminal waits between two polls.
pub const DEVICE_POLL_INTERVAL_SECONDS: i32 = 5;
/// No vowels, so a code can't spell a word, and nothing that reads like a digit.
const USER_CODE_ALPHABET: &[u8] = b"BCDFGHJKLMNPQRSTVWXZ";
const USER_CODE_LENGTH: usize = 8;
const UNIQUE_CODE_ATTEMPTS: usize = 3;

pub struct CreateAccessTokenUseCase {
    tokens: Arc<dyn AccessTokenRepositoryPort>,
}

impl CreateAccessTokenUseCase {
    pub fn new(tokens: Arc<dyn AccessTokenRepositoryPort>) -> Self {
        Self { tokens }
    }

    /// Returns the stored record with the plaintext token. Only the hash is persisted, so this is the one time the
    /// plaintext is available.
    pub async fn execute(
        &self,
        user_id: Uuid,
        owner_is_admin: bool,
        name: &str,
        scopes: Vec<TokenScope>,
        lifetime_days: i64,
    ) -> Result<(AccessToken, String), DomainError> {
        let name = validate_token_name(name)?;
        let lifetime_days = validate_lifetime_days(lifetime_days)?;
        ensure_owner_can_hold(&scopes, owner_is_admin)?;

        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        let plain = format!("{ACCESS_TOKEN_PREFIX}{}", hex::encode(bytes));
        let token = self
            .tokens
            .create(NewAccessToken {
                user_id,
                name,
                token_hash: hash_token(&plain),
                scopes,
                expires_at: Utc::now() + Duration::days(lifetime_days),
            })
            .await?;
        Ok((token, plain))
    }
}

/// What a terminal gets back when it asks to sign in. `device_code` is its secret, `user_code` what the person confirms.
pub struct StartedDeviceLogin {
    pub device_code: String,
    pub user_code: String,
    pub expires_in_seconds: i64,
    pub interval_seconds: i32,
}

/// Upper case, and the dash is optional, since people type these by hand.
pub fn normalize_user_code(input: &str) -> String {
    let compact: String = input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if compact.len() == USER_CODE_LENGTH {
        format!(
            "{}-{}",
            &compact[..USER_CODE_LENGTH / 2],
            &compact[USER_CODE_LENGTH / 2..]
        )
    } else {
        compact
    }
}

fn new_user_code() -> String {
    let mut rng = rand::rng();
    let compact: String = (0..USER_CODE_LENGTH)
        .map(|_| USER_CODE_ALPHABET[rng.random_range(0..USER_CODE_ALPHABET.len())] as char)
        .collect();
    normalize_user_code(&compact)
}

pub struct DeviceLoginService {
    requests: Arc<dyn DeviceAuthorizationPort>,
    tokens: Arc<dyn AccessTokenRepositoryPort>,
}

/// What a terminal's poll comes back with.
pub enum DeviceCollect {
    Pending,
    SlowDown,
    Denied,
    Expired,
    Token { token: AccessToken, plain: String },
}

impl DeviceLoginService {
    pub fn new(
        requests: Arc<dyn DeviceAuthorizationPort>,
        tokens: Arc<dyn AccessTokenRepositoryPort>,
    ) -> Self {
        Self { requests, tokens }
    }

    /// The caller is anonymous here. Nothing it sends is trusted until someone with a session reads it and approves.
    pub async fn start(
        &self,
        client_name: &str,
        scopes: Vec<TokenScope>,
        lifetime_days: i64,
    ) -> Result<StartedDeviceLogin, DomainError> {
        let client_name = validate_token_name(client_name)?;
        let lifetime_days = validate_lifetime_days(lifetime_days)?;
        self.requests.delete_expired().await?;

        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        let device_code = hex::encode(bytes);
        let device_code_hash = hash_token(&device_code);

        // A colliding user code is a unique violation, which only a retry with another one can clear.
        let mut last_error = None;
        for _ in 0..UNIQUE_CODE_ATTEMPTS {
            let user_code = new_user_code();
            let created = self
                .requests
                .create(NewDeviceAuthorization {
                    device_code_hash: device_code_hash.clone(),
                    user_code: user_code.clone(),
                    client_name: client_name.clone(),
                    scopes: scopes.clone(),
                    token_lifetime_days: lifetime_days as i32,
                    expires_at: Utc::now() + Duration::minutes(DEVICE_REQUEST_TTL_MINUTES),
                })
                .await;
            match created {
                Ok(()) => {
                    return Ok(StartedDeviceLogin {
                        device_code,
                        user_code,
                        expires_in_seconds: DEVICE_REQUEST_TTL_MINUTES * 60,
                        interval_seconds: DEVICE_POLL_INTERVAL_SECONDS,
                    });
                }
                Err(e) => last_error = Some(e),
            }
        }
        Err(last_error.expect("at least one attempt ran"))
    }

    pub async fn find_pending(&self, user_code: &str) -> Result<DeviceAuthorization, DomainError> {
        self.requests
            .find_pending_by_user_code(&normalize_user_code(user_code))
            .await?
            .ok_or_else(|| DomainError::NotFound("device request".to_string()))
    }

    /// The caller has already checked the second factor. An owner who can't hold the requested scopes can't approve them.
    pub async fn approve(
        &self,
        user_code: &str,
        user_id: Uuid,
        owner_is_admin: bool,
    ) -> Result<(), DomainError> {
        let request = self.find_pending(user_code).await?;
        ensure_owner_can_hold(&request.scopes, owner_is_admin)?;
        if self.requests.approve(&request.user_code, user_id).await? {
            Ok(())
        } else {
            Err(DomainError::NotFound("device request".to_string()))
        }
    }

    pub async fn deny(&self, user_code: &str) -> Result<(), DomainError> {
        if self.requests.deny(&normalize_user_code(user_code)).await? {
            Ok(())
        } else {
            Err(DomainError::NotFound("device request".to_string()))
        }
    }

    /// The token is only created here, so its plaintext never sits in the database waiting to be collected.
    pub async fn collect(&self, device_code: &str) -> Result<DeviceCollect, DomainError> {
        let polled = self
            .requests
            .poll(&hash_token(device_code), DEVICE_POLL_INTERVAL_SECONDS - 1)
            .await?;
        Ok(match polled {
            DevicePoll::Pending => DeviceCollect::Pending,
            DevicePoll::TooFast => DeviceCollect::SlowDown,
            DevicePoll::Denied => DeviceCollect::Denied,
            DevicePoll::Expired => DeviceCollect::Expired,
            DevicePoll::Approved(request) => {
                let user_id = request.user_id.ok_or_else(|| {
                    DomainError::Infrastructure("approved device request has no user".to_string())
                })?;
                let mut bytes = [0u8; 32];
                rand::rng().fill_bytes(&mut bytes);
                let plain = format!("{ACCESS_TOKEN_PREFIX}{}", hex::encode(bytes));
                let token = self
                    .tokens
                    .create(NewAccessToken {
                        user_id,
                        name: request.client_name,
                        token_hash: hash_token(&plain),
                        scopes: request.scopes,
                        expires_at: Utc::now()
                            + Duration::days(i64::from(request.token_lifetime_days)),
                    })
                    .await?;
                DeviceCollect::Token { token, plain }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeAccessTokens, FakeDeviceAuthorizations};

    fn scope(value: &str) -> TokenScope {
        value.parse().unwrap()
    }

    fn create_use_case() -> (Arc<FakeAccessTokens>, CreateAccessTokenUseCase) {
        let tokens = Arc::new(FakeAccessTokens::default());
        (tokens.clone(), CreateAccessTokenUseCase::new(tokens))
    }

    fn device_service() -> (Arc<FakeAccessTokens>, DeviceLoginService) {
        let tokens = Arc::new(FakeAccessTokens::default());
        let requests = Arc::new(FakeDeviceAuthorizations::default());
        (tokens.clone(), DeviceLoginService::new(requests, tokens))
    }

    #[tokio::test]
    async fn a_created_token_stores_only_its_hash_and_expires() {
        let (_, use_case) = create_use_case();

        let (stored, plain) = use_case
            .execute(Uuid::new_v4(), false, "ci", vec![scope("issues:read")], 30)
            .await
            .unwrap();

        assert!(plain.starts_with(ACCESS_TOKEN_PREFIX));
        assert_eq!(stored.token_hash, hash_token(&plain));
        assert_ne!(stored.token_hash, plain);
        assert!(stored.expires_at > Utc::now() + Duration::days(29));
    }

    #[tokio::test]
    async fn a_non_administrator_can_not_create_an_admin_token() {
        let (tokens, use_case) = create_use_case();

        let result = use_case
            .execute(Uuid::new_v4(), false, "ci", vec![scope("admin:read")], 30)
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
        assert!(tokens.snapshot().is_empty());
    }

    #[tokio::test]
    async fn a_token_past_the_longest_lifetime_is_refused() {
        let (_, use_case) = create_use_case();

        let result = use_case
            .execute(Uuid::new_v4(), false, "ci", vec![scope("issues:read")], 366)
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[test]
    fn a_user_code_reads_back_however_it_was_typed() {
        assert_eq!(normalize_user_code("bcdf-ghjk"), "BCDF-GHJK");
        assert_eq!(normalize_user_code(" BCDFGHJK "), "BCDF-GHJK");
        assert_eq!(normalize_user_code(&new_user_code()).len(), 9);
    }

    #[tokio::test]
    async fn a_request_can_not_be_collected_before_it_is_approved() {
        let (tokens, service) = device_service();

        let started = service
            .start("fgit", vec![scope("issues:read")], 90)
            .await
            .unwrap();

        assert!(matches!(
            service.collect(&started.device_code).await.unwrap(),
            DeviceCollect::Pending
        ));
        assert!(tokens.snapshot().is_empty());
    }

    #[tokio::test]
    async fn an_approved_request_gives_one_token_of_what_was_asked_for_its_approver() {
        let (tokens, service) = device_service();
        let user_id = Uuid::new_v4();
        let started = service
            .start("fgit on laptop", vec![scope("issues:write")], 90)
            .await
            .unwrap();

        service
            .approve(&started.user_code, user_id, false)
            .await
            .unwrap();
        let DeviceCollect::Token { token, plain } =
            service.collect(&started.device_code).await.unwrap()
        else {
            panic!("expected a token");
        };

        assert_eq!(token.user_id, user_id);
        assert_eq!(token.name, "fgit on laptop");
        assert_eq!(token.scopes, vec![scope("issues:write")]);
        assert_eq!(token.token_hash, hash_token(&plain));
        assert!(matches!(
            service.collect(&started.device_code).await.unwrap(),
            DeviceCollect::Expired
        ));
        assert_eq!(tokens.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn a_non_administrator_can_not_approve_a_request_for_the_admin_scope() {
        let (_, service) = device_service();
        let started = service
            .start("fgit", vec![scope("admin:write")], 90)
            .await
            .unwrap();

        let result = service
            .approve(&started.user_code, Uuid::new_v4(), false)
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
        assert!(matches!(
            service.collect(&started.device_code).await.unwrap(),
            DeviceCollect::Pending
        ));
    }

    #[tokio::test]
    async fn a_denied_request_tells_the_terminal() {
        let (_, service) = device_service();
        let started = service
            .start("fgit", vec![scope("issues:read")], 90)
            .await
            .unwrap();

        service.deny(&started.user_code).await.unwrap();

        assert!(matches!(
            service.collect(&started.device_code).await.unwrap(),
            DeviceCollect::Denied
        ));
    }

    #[tokio::test]
    async fn an_unknown_device_code_reads_as_expired() {
        let (_, service) = device_service();

        assert!(matches!(
            service.collect("nothing").await.unwrap(),
            DeviceCollect::Expired
        ));
    }

    #[tokio::test]
    async fn an_unknown_user_code_can_be_neither_approved_nor_denied() {
        let (_, service) = device_service();

        assert!(matches!(
            service.approve("BCDF-GHJK", Uuid::new_v4(), false).await,
            Err(DomainError::NotFound(_))
        ));
        assert!(matches!(
            service.deny("BCDF-GHJK").await,
            Err(DomainError::NotFound(_))
        ));
    }
}
