use axum::extract::{ConnectInfo, DefaultBodyLimit, FromRequestParts, State};
use axum::http::header::HeaderName;
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use ferrisgit_application::email_templates;
use ferrisgit_application::use_cases::admin_reset_password::ConsumePasswordResetUseCase;
use ferrisgit_application::use_cases::change_password::ChangePasswordUseCase;
use ferrisgit_application::use_cases::invitations::ActivateAccountUseCase;
use ferrisgit_application::use_cases::login::LoginUseCase;
use ferrisgit_application::use_cases::register_user::RegisterUserUseCase;
use ferrisgit_application::use_cases::update_email::UpdateEmailUseCase;
use ferrisgit_domain::audit::SecurityEvent;
use ferrisgit_domain::email::is_valid_mailbox;
use ferrisgit_domain::error::DomainError;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::net::SocketAddr;
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::client_ip::resolve_client_ip;
use crate::error::ApiError;
use crate::routes::mfa::UNAUTHENTICATED_BODY_LIMIT_BYTES;
use crate::state::AppState;

const FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");

/// Axum's `ConnectInfo` fails the request when the server was not built with
/// `into_make_service_with_connect_info`, and `Option<ConnectInfo>` is not supported. This one never fails.
pub(crate) struct MaybeConnectInfo(pub(crate) Option<SocketAddr>);

impl<S: Send + Sync> FromRequestParts<S> for MaybeConnectInfo {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(MaybeConnectInfo(
            parts
                .extensions
                .get::<ConnectInfo<SocketAddr>>()
                .map(|ci| ci.0),
        ))
    }
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct RegisterRequest {
    username: String,
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct ActivateRequest {
    token: String,
    password: String,
}

#[derive(Deserialize)]
struct ResetPasswordRequest {
    token: String,
    password: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthConfigResponse {
    registration_enabled: bool,
    passkeys_available: bool,
    public_pages_enabled: bool,
}

/// `token` is `null` for a local account (always, in production): the password step only yields an `mfaToken`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LoginResponse {
    token: Option<String>,
    mfa_token: Option<String>,
    mfa_setup_required: bool,
    mfa_has_totp: bool,
    mfa_has_passkey: bool,
}

#[derive(Serialize)]
pub(crate) struct SessionResponse {
    pub token: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MeResponse {
    id: Uuid,
    username: String,
    email: String,
    is_admin: bool,
}

#[derive(Deserialize)]
struct UpdateEmailRequest {
    email: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

/// Behind a trusted proxy (`TRUSTED_PROXY_CIDRS`) the forwarded client address, else the TCP peer. Without
/// `ConnectInfo` everybody shares one bucket, which beats no rate limiting.
pub(crate) fn client_ip(
    state: &AppState,
    connect_info: Option<SocketAddr>,
    headers: &HeaderMap,
) -> std::net::IpAddr {
    let forwarded_for = headers
        .get_all(FORWARDED_FOR)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .collect::<Vec<_>>()
        .join(",");
    resolve_client_ip(
        connect_info.map(|addr| addr.ip()),
        Some(&forwarded_for),
        &state.config.trusted_proxy_cidrs,
    )
}

async fn login(
    MaybeConnectInfo(connect_info): MaybeConnectInfo,
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    if !state
        .login_rate_limiter
        .check(client_ip(&state, connect_info, &headers))
    {
        return Err(DomainError::RateLimited(
            "too many login attempts, try again later".to_string(),
        )
        .into());
    }
    let use_case = LoginUseCase::new(
        state.users.clone(),
        state.hasher.clone(),
        state.events.clone(),
    );
    let user_id = use_case.execute(&req.username, &req.password).await?;
    login_outcome(&state, user_id).await.map(Json)
}

/// Shared by login and registration, so a fresh registration lands in the MFA setup like a first login. With
/// `mfa_enforced == false` (tests only) it is a plain session.
async fn login_outcome(state: &AppState, user_id: Uuid) -> Result<LoginResponse, ApiError> {
    if !state.mfa_enforced {
        let token = issue_session(state, user_id).await?;
        return Ok(LoginResponse {
            token: Some(token),
            mfa_token: None,
            mfa_setup_required: false,
            mfa_has_totp: false,
            mfa_has_passkey: false,
        });
    }
    mfa_login_response(state, user_id).await
}

async fn auth_config(State(state): State<AppState>) -> Result<Json<AuthConfigResponse>, ApiError> {
    Ok(Json(AuthConfigResponse {
        registration_enabled: state.registration_settings.is_enabled().await?,
        passkeys_available: state.passkeys.available(),
        public_pages_enabled: state
            .public_pages_settings
            .get()
            .await?
            .public_pages_enabled,
    }))
}

/// The answer is the login response: no session, only the `mfaToken` of the mandatory MFA setup. 409 for a taken
/// username or e-mail (the per-IP throttle bounds probing).
async fn register(
    MaybeConnectInfo(connect_info): MaybeConnectInfo,
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    if !state
        .register_rate_limiter
        .check(client_ip(&state, connect_info, &headers))
    {
        return Err(DomainError::RateLimited(
            "too many registration attempts, try again later".to_string(),
        )
        .into());
    }
    let use_case = RegisterUserUseCase::new(
        state.users.clone(),
        state.hasher.clone(),
        state.groups.clone(),
        state.registration_settings.clone(),
    );
    let user = use_case
        .execute(req.username, req.email, req.password)
        .await?;
    login_outcome(&state, user.id).await.map(Json)
}

/// 204 with no session. One generic 400 for an unknown, expired or used token and for a rule-breaking password
/// (the link stays usable).
async fn activate(
    MaybeConnectInfo(connect_info): MaybeConnectInfo,
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<ActivateRequest>,
) -> Result<StatusCode, ApiError> {
    if !state
        .activation_rate_limiter
        .check(client_ip(&state, connect_info, &headers))
    {
        return Err(DomainError::RateLimited(
            "too many activation attempts, try again later".to_string(),
        )
        .into());
    }
    ActivateAccountUseCase::new(
        state.users.clone(),
        state.hasher.clone(),
        state.invitations.clone(),
    )
    .execute(&req.token, &req.password)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Mirrors `activate`. A success sends a mail, because the link may have been handed over by an admin or
/// intercepted, and the account holder must learn their password was set. It shares activation's per-IP budget: a
/// well-formed token costs an argon2 run before the lookup, and a single budget caps a client's total argon2 cost.
async fn reset_password(
    MaybeConnectInfo(connect_info): MaybeConnectInfo,
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<ResetPasswordRequest>,
) -> Result<StatusCode, ApiError> {
    if !state
        .activation_rate_limiter
        .check(client_ip(&state, connect_info, &headers))
    {
        return Err(DomainError::RateLimited(
            "too many password reset attempts, try again later".to_string(),
        )
        .into());
    }
    let user_id = ConsumePasswordResetUseCase::new(
        state.users.clone(),
        state.hasher.clone(),
        state.password_resets.clone(),
    )
    .execute(&req.token, &req.password)
    .await?;
    // Best effort: mail failures never fail a reset that already succeeded.
    if let Ok(Some(user)) = state.users.find_by_id(user_id).await
        && is_valid_mailbox(&user.email)
    {
        state.mailer.send_in_background(
            user.email.clone(),
            email_templates::password_changed(&user.username),
        );
    }
    Ok(StatusCode::NO_CONTENT)
}

/// The setup is only offered to an account with no factor at all. A factor that cannot be read gives a 5xx here,
/// not "no factor", which would offer an enrolment to an account that already has one.
pub(crate) async fn mfa_login_response(
    state: &AppState,
    user_id: Uuid,
) -> Result<LoginResponse, ApiError> {
    let factors = state.mfa.factors(user_id).await?;
    let epoch = state.users.get_token_epoch(user_id).await?;
    let mfa_token = state.mfa_pending.issue(user_id, epoch)?;
    Ok(LoginResponse {
        token: None,
        mfa_token: Some(mfa_token),
        mfa_setup_required: !factors.any(),
        mfa_has_totp: factors.has_totp,
        mfa_has_passkey: factors.has_passkey,
    })
}

pub(crate) async fn issue_session(state: &AppState, user_id: Uuid) -> Result<String, ApiError> {
    let epoch = state.users.get_token_epoch(user_id).await?;
    issue_session_with_epoch(state, user_id, epoch).await
}

/// Uses an epoch the caller already read. If the user's epoch has moved since, the session starts out revoked.
pub(crate) async fn issue_session_with_epoch(
    state: &AppState,
    user_id: Uuid,
    epoch: i32,
) -> Result<String, ApiError> {
    let token = state.token_issuer.issue(user_id, epoch)?;
    state
        .events
        .publish_security_event(SecurityEvent::LoginSucceeded { user_id }, Some(user_id))
        .await
        .ok();
    Ok(token)
}

async fn me(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
) -> Result<Json<MeResponse>, ApiError> {
    let user = state
        .users
        .find_by_id(user_id)
        .await?
        .ok_or_else(|| DomainError::NotFound("user".to_string()))?;
    Ok(Json(MeResponse {
        id: user.id,
        username: user.username,
        email: user.email,
        is_admin: user.is_admin,
    }))
}

async fn update_email(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<UpdateEmailRequest>,
) -> Result<Json<MeResponse>, ApiError> {
    let use_case = UpdateEmailUseCase::new(state.users.clone());
    let user = use_case.execute(user_id, req.email).await?;
    Ok(Json(MeResponse {
        id: user.id,
        username: user.username,
        email: user.email,
        is_admin: user.is_admin,
    }))
}

async fn change_password(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<Json<SessionResponse>, ApiError> {
    // Same per-user budget as the MFA endpoints: this is also a current-password oracle.
    crate::routes::mfa::within_budget(&state, user_id)?;
    // Sessions only come from MFA (tests aside) and disabling a factor bumps the epoch, so no factor-less session
    // exists. The fresh session below cannot renew one that should be forced through enrolment.
    let use_case = ChangePasswordUseCase::new(state.users.clone(), state.hasher.clone());
    use_case
        .execute(user_id, &req.current_password, &req.new_password)
        .await?;
    // The epoch bump revoked this request's JWT; hand back a fresh one so the caller's session survives.
    let epoch = state.users.get_token_epoch(user_id).await?;
    let token = state.token_issuer.issue(user_id, epoch)?;
    // Best effort: mail failures never fail a password change that already succeeded.
    if let Ok(Some(user)) = state.users.find_by_id(user_id).await
        && is_valid_mailbox(&user.email)
    {
        state.mailer.send_in_background(
            user.email.clone(),
            email_templates::password_changed(&user.username),
        );
    }
    Ok(Json(SessionResponse { token }))
}

pub fn router() -> Router<AppState> {
    // `layer` wraps only what was added before it, so the body limit hits the unauthenticated routes alone.
    let unauthenticated = Router::new()
        .route("/auth/config", get(auth_config))
        .route("/auth/register", post(register))
        .route("/auth/activate", post(activate))
        .route("/auth/reset-password", post(reset_password))
        .layer(DefaultBodyLimit::max(UNAUTHENTICATED_BODY_LIMIT_BYTES));
    unauthenticated.merge(
        Router::new()
            .route("/auth/login", post(login))
            .route("/auth/me", get(me).patch(update_email))
            .route("/auth/me/password", post(change_password)),
    )
}
