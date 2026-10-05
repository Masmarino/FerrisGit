use axum::extract::{ConnectInfo, DefaultBodyLimit, FromRequestParts, State};
use axum::http::header::HeaderName;
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::email_templates;
use ferrisgit_application::use_cases::admin_reset_password::ConsumePasswordResetUseCase;
use ferrisgit_application::use_cases::change_password::ChangePasswordUseCase;
use ferrisgit_application::use_cases::invitations::{ActivateAccountUseCase, InvitedUser};
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
use crate::routes::user_ref::require_user;
use crate::state::AppState;

const FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");

/// Axum's `ConnectInfo` fails the request when the server wasn't built with `into_make_service_with_connect_info`,
/// and `Option<ConnectInfo>` isn't supported. This one never fails.
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
}

#[derive(Deserialize)]
struct ActivateRequest {
    token: String,
    password: String,
    /// The name an invitee chooses, when an admin invited them by e-mail. Left out when it was chosen at registration.
    #[serde(default)]
    username: Option<String>,
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

/// `token` is null for a local account (always, in production): the password step only gives an `mfaToken`.
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
    created_at: DateTime<Utc>,
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

/// Behind a trusted proxy (`TRUSTED_PROXY_CIDRS`) the forwarded client address, otherwise the TCP peer. Without
/// `ConnectInfo` everybody shares one bucket, which still beats no limit.
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

/// With `mfa_enforced` off (tests only) it's a plain session.
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
        // Without mail nobody could receive their activation link, so the sign-up page isn't offered.
        registration_enabled: state.registration_settings.is_enabled().await?
            && state.smtp_settings.get().await?.is_some(),
        passkeys_available: state.passkeys.available(),
        public_pages_enabled: state
            .public_pages_settings
            .get()
            .await?
            .public_pages_enabled,
    }))
}

/// 204 with no session: the account stays unusable until its owner follows the link mailed to the address they gave
/// and picks a password. 409 for a taken username or e-mail (the per-IP throttle limits probing), and the same name
/// and address as an account nobody has activated yet gets a fresh link instead. 503 when mail isn't configured or
/// the message can't be sent, without saying why: this route is open to anyone.
async fn register(
    MaybeConnectInfo(connect_info): MaybeConnectInfo,
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<StatusCode, ApiError> {
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
        state.invitations.clone(),
        state.smtp_settings.clone(),
    );
    let InvitedUser { user, token } = use_case.execute(req.username, req.email).await?;
    // The token is in the URL fragment, which browsers never send, so it stays out of access logs.
    let activation_url = format!("{}/activate#token={}", state.config.public_url, token);
    state
        .mailer
        .send(
            &user.email,
            email_templates::registration_confirmation(&user.username, &activation_url),
        )
        .await
        .map_err(|error| {
            // The account stays pending: registering again with the same name and address sends a new link.
            tracing::warn!(user_id = %user.id, %error, "could not send the registration e-mail");
            DomainError::ServiceUnavailable(
                "the confirmation e-mail could not be sent, try again later".to_string(),
            )
        })?;
    Ok(StatusCode::NO_CONTENT)
}

/// 204 with no session. One generic 400 for an unknown, expired or used token; a 400 for a password or a name that
/// breaks the rules and a 409 for a name already taken, the link staying usable.
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
        state.groups.clone(),
        state.invitations.clone(),
    )
    .execute(&req.token, &req.password, req.username.as_deref())
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Mirrors `activate`. A success sends a mail, since the link may have come from an admin or been intercepted and
/// the account holder should know their password was set. Shares activation's per-IP budget: a well-formed token
/// costs an argon2 run before the lookup, so one budget caps a client's total argon2 cost.
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
    // A mail failure doesn't undo a reset that already succeeded.
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

/// Only offered to an account with no factor at all. A factor we can't read is a 5xx, not "no factor", which
/// would offer an enrolment to an account that already has one.
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

/// Takes an epoch the caller already read: if the user's epoch moved since, the session is born revoked.
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
    let user = require_user(&state, user_id).await?;
    Ok(Json(MeResponse {
        id: user.id,
        username: user.username,
        email: user.email,
        is_admin: user.is_admin,
        created_at: user.created_at,
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
        created_at: user.created_at,
    }))
}

async fn change_password(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<Json<SessionResponse>, ApiError> {
    // Same per-user budget as the MFA endpoints, since this is also a current-password oracle.
    crate::routes::mfa::within_budget(&state, user_id)?;
    // Sessions only come from MFA (tests aside) and disabling a factor bumps the epoch, so no session without a
    // factor exists. The fresh session below can't renew one that should be forced through enrolment.
    let use_case = ChangePasswordUseCase::new(state.users.clone(), state.hasher.clone());
    use_case
        .execute(user_id, &req.current_password, &req.new_password)
        .await?;
    // The epoch bump killed this request's JWT, so hand back a fresh one to keep the caller signed in.
    let epoch = state.users.get_token_epoch(user_id).await?;
    let token = state.token_issuer.issue(user_id, epoch)?;
    // A mail failure doesn't undo a password change that already succeeded.
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

/// Signs the account out everywhere: bumping the epoch ends every session, this one included. The Git tokens stay:
/// they are revoked one by one, from the account's tokens.
async fn logout_all(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
) -> Result<StatusCode, ApiError> {
    state.users.bump_token_epoch(user_id).await?;
    state
        .events
        .publish_security_event(SecurityEvent::SessionsRevoked { user_id }, Some(user_id))
        .await
        .ok();
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    // layer() only wraps the routes added before it, so the body limit hits the unauthenticated ones alone.
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
            .route("/auth/me/password", post(change_password))
            .route("/auth/logout-all", post(logout_all)),
    )
}
