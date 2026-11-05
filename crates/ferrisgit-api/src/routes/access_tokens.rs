use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::HeaderMap;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::use_cases::access_tokens::{
    CreateAccessTokenUseCase, DeviceCollect, DeviceLoginService,
};
use ferrisgit_domain::access_token::{
    AccessToken, TokenScope, ensure_owner_can_hold, parse_requested_scopes,
};
use ferrisgit_domain::audit::SecurityEvent;
use ferrisgit_domain::error::DomainError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use webauthn_rs::prelude::PublicKeyCredential;

use crate::auth_middleware::AuthUser;
use crate::error::ApiError;
use crate::routes::auth::MaybeConnectInfo;
use crate::routes::mfa::{
    ChallengeResponse, TOO_MANY_ATTEMPTS, UNAUTHENTICATED_BODY_LIMIT_BYTES, challenge_response,
    ensure_passkeys_available, parse_credential, within_start_budget,
};
use crate::routes::user_ref::require_user;
use crate::state::AppState;

fn scope_strings(scopes: &[TokenScope]) -> Vec<String> {
    scopes.iter().map(ToString::to_string).collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateRequest {
    name: String,
    scopes: Vec<String>,
    expires_in_days: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TokenSummary {
    id: Uuid,
    name: String,
    scopes: Vec<String>,
    expires_at: DateTime<Utc>,
    expired: bool,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
}

impl From<AccessToken> for TokenSummary {
    fn from(token: AccessToken) -> Self {
        TokenSummary {
            expired: token.is_expired(Utc::now()),
            id: token.id,
            name: token.name,
            scopes: scope_strings(&token.scopes),
            expires_at: token.expires_at,
            created_at: token.created_at,
            last_used_at: token.last_used_at,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreatedToken {
    #[serde(flatten)]
    summary: TokenSummary,
    token: String,
}

async fn publish(state: &AppState, event: SecurityEvent, actor: Uuid) {
    state
        .events
        .publish_security_event(event, Some(actor))
        .await
        .ok();
}

async fn create(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateRequest>,
) -> Result<Json<CreatedToken>, ApiError> {
    let owner = require_user(&state, user_id).await?;
    let scopes = parse_requested_scopes(&req.scopes)?;
    let (token, plain) = CreateAccessTokenUseCase::new(state.access_tokens.clone())
        .execute(
            user_id,
            owner.is_admin,
            &req.name,
            scopes,
            req.expires_in_days,
        )
        .await?;
    publish(
        &state,
        SecurityEvent::AccessTokenCreated {
            user_id,
            token_id: token.id,
        },
        user_id,
    )
    .await;
    Ok(Json(CreatedToken {
        summary: token.into(),
        token: plain,
    }))
}

async fn list(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<TokenSummary>>, ApiError> {
    let tokens = state.access_tokens.list_for_user(user_id).await?;
    Ok(Json(tokens.into_iter().map(Into::into).collect()))
}

async fn revoke(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, ApiError> {
    state.access_tokens.revoke(id, user_id).await?;
    publish(
        &state,
        SecurityEvent::AccessTokenRevoked {
            user_id,
            token_id: id,
        },
        user_id,
    )
    .await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

fn device_service(state: &AppState) -> DeviceLoginService {
    DeviceLoginService::new(
        state.device_authorizations.clone(),
        state.access_tokens.clone(),
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceStartRequest {
    client_name: String,
    scopes: Vec<String>,
    expires_in_days: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceStartResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: String,
    expires_in: i64,
    interval: i32,
}

async fn device_start(
    MaybeConnectInfo(connect_info): MaybeConnectInfo,
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<DeviceStartRequest>,
) -> Result<Json<DeviceStartResponse>, ApiError> {
    let ip = crate::routes::auth::client_ip(&state, connect_info, &headers);
    if !state.device_start_limiter.check(ip) {
        return Err(DomainError::RateLimited(TOO_MANY_ATTEMPTS.to_string()).into());
    }
    let scopes = parse_requested_scopes(&req.scopes)?;
    let started = device_service(&state)
        .start(&req.client_name, scopes, req.expires_in_days)
        .await?;
    let verification_uri = format!(
        "{}/account/device",
        state.config.public_url.trim_end_matches('/')
    );
    Ok(Json(DeviceStartResponse {
        verification_uri_complete: format!("{verification_uri}?code={}", started.user_code),
        verification_uri,
        device_code: started.device_code,
        user_code: started.user_code,
        expires_in: started.expires_in_seconds,
        interval: started.interval_seconds,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceTokenRequest {
    device_code: String,
}

/// A pending request is an expected answer, not a failure, but a terminal reads it the way RFC 8628 spells it, as a 400
/// with the state as the error.
async fn device_token(
    State(state): State<AppState>,
    Json(req): Json<DeviceTokenRequest>,
) -> Result<Json<CreatedToken>, ApiError> {
    let state_error = |name: &str| ApiError(DomainError::Validation(name.to_string()));
    match device_service(&state).collect(&req.device_code).await? {
        DeviceCollect::Pending => Err(state_error("authorization_pending")),
        DeviceCollect::SlowDown => Err(state_error("slow_down")),
        DeviceCollect::Denied => Err(state_error("access_denied")),
        DeviceCollect::Expired => Err(state_error("expired_token")),
        DeviceCollect::Token { token, plain } => {
            publish(
                &state,
                SecurityEvent::AccessTokenCreated {
                    user_id: token.user_id,
                    token_id: token.id,
                },
                token.user_id,
            )
            .await;
            Ok(Json(CreatedToken {
                summary: token.into(),
                token: plain,
            }))
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceRequestResponse {
    user_code: String,
    client_name: String,
    scopes: Vec<String>,
    token_lifetime_days: i32,
    requested_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    has_totp: bool,
    has_passkey: bool,
}

async fn device_request(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(user_code): Path<String>,
) -> Result<Json<DeviceRequestResponse>, ApiError> {
    let request = device_service(&state).find_pending(&user_code).await?;
    let factors = state.mfa.factors(user_id).await?;
    Ok(Json(DeviceRequestResponse {
        user_code: request.user_code,
        client_name: request.client_name,
        scopes: scope_strings(&request.scopes),
        token_lifetime_days: request.token_lifetime_days,
        requested_at: request.created_at,
        expires_at: request.expires_at,
        has_totp: factors.has_totp,
        has_passkey: factors.has_passkey,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceApproveRequest {
    code: Option<String>,
    backup_code: Option<String>,
    challenge_id: Option<Uuid>,
    credential: Option<serde_json::Value>,
}

/// Approving hands out a long-lived token, so the second factor is asked again even with a live session. A refusal is a
/// 400 and not a 401, or the web interface would read it as a session that ended.
async fn device_approve(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(user_code): Path<String>,
    Json(req): Json<DeviceApproveRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    let service = device_service(&state);
    let request = service.find_pending(&user_code).await?;
    // Before the factor is checked: a refusal mustn't cost a backup code or a TOTP step.
    let owner = require_user(&state, user_id).await?;
    ensure_owner_can_hold(&request.scopes, owner.is_admin)?;
    if !state.mfa_limiter.check(user_id) {
        return Err(DomainError::RateLimited(TOO_MANY_ATTEMPTS.to_string()).into());
    }

    let verified = match (req.challenge_id, req.credential) {
        (Some(challenge_id), Some(credential)) => {
            ensure_passkeys_available(&state)?;
            let credential: PublicKeyCredential = parse_credential(credential)?;
            state
                .passkeys
                .finish_authentication(user_id, challenge_id, &credential)
                .await
        }
        _ => {
            state
                .mfa
                .verify_challenge(user_id, req.code.as_deref(), req.backup_code.as_deref())
                .await
        }
    };
    match verified {
        Ok(()) => {}
        Err(DomainError::Unauthorized(_) | DomainError::NotFound(_)) => {
            publish(
                &state,
                SecurityEvent::MfaVerificationFailed { user_id },
                user_id,
            )
            .await;
            return Err(DomainError::Validation("invalid code".to_string()).into());
        }
        Err(e) => return Err(e.into()),
    }

    service
        .approve(&request.user_code, user_id, owner.is_admin)
        .await?;
    publish(
        &state,
        SecurityEvent::DeviceLoginApproved { user_id },
        user_id,
    )
    .await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn device_passkey_start(
    AuthUser(user_id): AuthUser,
    MaybeConnectInfo(connect_info): MaybeConnectInfo,
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(user_code): Path<String>,
) -> Result<Json<ChallengeResponse>, ApiError> {
    within_start_budget(&state, connect_info, &headers)?;
    ensure_passkeys_available(&state)?;
    device_service(&state).find_pending(&user_code).await?;
    let (challenge_id, challenge) = state.passkeys.start_authentication(user_id).await?;
    challenge_response(challenge_id, &challenge.public_key)
}

async fn device_deny(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
    Path(user_code): Path<String>,
) -> Result<axum::http::StatusCode, ApiError> {
    device_service(&state).deny(&user_code).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    let anonymous = Router::new()
        .route("/auth/device/start", post(device_start))
        .route("/auth/device/token", post(device_token))
        .layer(DefaultBodyLimit::max(UNAUTHENTICATED_BODY_LIMIT_BYTES));
    Router::new()
        .route("/access-tokens", post(create).get(list))
        .route("/access-tokens/{id}", delete(revoke))
        .route("/auth/device/{user_code}", get(device_request))
        .route("/auth/device/{user_code}/approve", post(device_approve))
        .route(
            "/auth/device/{user_code}/passkey",
            post(device_passkey_start),
        )
        .route("/auth/device/{user_code}/deny", post(device_deny))
        .merge(anonymous)
}
