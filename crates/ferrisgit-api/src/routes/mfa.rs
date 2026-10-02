//! MFA endpoints. A user has MFA with a confirmed TOTP or at least one passkey.
//!
//! The unauthenticated routes are authorised by the `mfa-pending` token in the body, and every handler goes through
//! `pending_user` first. Only a success spends the token, so a failed attempt doesn't burn it, and the per-user
//! limiter caps guessing. The passkey `start` routes verify nothing and cost no per-user attempt (a dismissed
//! browser prompt mustn't lock anyone out), but each parks a ceremony in memory, so they spend a per-IP budget.
//! First-enrolment routes refuse a user who already has a factor, so a password-only token can't add a second one.
//!
//! Self-service routes (`/me/mfa/...`) take a session. Removing a factor bumps the token epoch first (killing all
//! sessions), then removes it.
//!
//! When `PUBLIC_URL` can't carry passkeys (IP literal, or plain http on a non-localhost host) the ceremony routes
//! answer 503, while listing and deleting keep working. Malformed passkey bodies are a 400 (`StrictJson`).

use axum::extract::{DefaultBodyLimit, FromRequest, Path, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::email_templates;
use ferrisgit_domain::audit::SecurityEvent;
use ferrisgit_domain::email::is_valid_mailbox;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::mfa::PendingToken;
use ferrisgit_domain::webauthn::StoredPasskey;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use webauthn_rs::prelude::{PublicKeyCredential, RegisterPublicKeyCredential};

use crate::auth_middleware::AuthUser;
use crate::error::ApiError;
use crate::routes::auth::{MaybeConnectInfo, SessionResponse, client_ip, issue_session_with_epoch};
use crate::state::AppState;

/// Plenty for a token and a short code. Axum's 2 MB default would let anyone push megabytes without logging in.
pub(crate) const UNAUTHENTICATED_BODY_LIMIT_BYTES: usize = 16 * 1024;

const TOTP_METHOD_LABEL: &str = "une application d'authentification (TOTP)";
const PASSKEY_METHOD_LABEL: &str = "une clé d'accès (passkey)";

const ALREADY_SET_UP: &str = "MFA is already set up";
const PASSKEYS_UNAVAILABLE: &str = "passkeys are not available on this server";
const TOO_MANY_ATTEMPTS: &str = "too many attempts, try again later";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VerifyRequest {
    mfa_token: String,
    code: Option<String>,
    backup_code: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetupTokenRequest {
    mfa_token: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfirmRequest {
    mfa_token: String,
    code: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EnrollResponse {
    secret: String,
    otpauth_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupCompleteResponse {
    token: String,
    backup_codes: Vec<String>,
}

fn invalid_token() -> ApiError {
    DomainError::Unauthorized("invalid or expired token".to_string()).into()
}

/// One error for every token problem, so a forged token looks like an expired, revoked or spent one. Read-only and
/// ahead of the limiter: asking again about a spent token mustn't burn the user's budget or a TOTP step the next
/// legitimate login needs.
async fn verified_pending(state: &AppState, mfa_token: &str) -> Result<PendingToken, ApiError> {
    let pending = state.mfa_pending.verify(mfa_token)?;
    ensure_epoch_is_current(state, &pending).await?;
    if state.mfa_spent_tokens.is_spent(mfa_token) {
        return Err(invalid_token());
    }
    Ok(pending)
}

async fn pending_user(state: &AppState, mfa_token: &str) -> Result<PendingToken, ApiError> {
    let pending = verified_pending(state, mfa_token).await?;
    if !state.mfa_limiter.check(pending.user_id) {
        return Err(DomainError::RateLimited(TOO_MANY_ATTEMPTS.to_string()).into());
    }
    Ok(pending)
}

/// An epoch bump (password change, MFA reset, disable) makes the token stale. A vanished user reads as an invalid
/// token too, but infrastructure failures stay 5xx.
async fn ensure_epoch_is_current(state: &AppState, pending: &PendingToken) -> Result<(), ApiError> {
    match state.users.get_token_epoch(pending.user_id).await {
        Ok(current) if current == pending.epoch => Ok(()),
        Ok(_) | Err(DomainError::NotFound(_)) => Err(invalid_token()),
        Err(e) => Err(e.into()),
    }
}

/// `consume` returning false means a concurrent request already spent the token. The session carries the token's
/// epoch, so a bump since the gate leaves it revoked.
async fn complete_login(
    state: &AppState,
    mfa_token: &str,
    pending: &PendingToken,
) -> Result<String, ApiError> {
    if !state.mfa_spent_tokens.consume(mfa_token) {
        return Err(invalid_token());
    }
    ensure_epoch_is_current(state, pending).await?;
    issue_session_with_epoch(state, pending.user_id, pending.epoch).await
}

async fn verify(
    State(state): State<AppState>,
    Json(req): Json<VerifyRequest>,
) -> Result<Json<SessionResponse>, ApiError> {
    let pending = pending_user(&state, &req.mfa_token).await?;
    match state
        .mfa
        .verify_challenge(
            pending.user_id,
            req.code.as_deref(),
            req.backup_code.as_deref(),
        )
        .await
    {
        Ok(()) => {
            let token = complete_login(&state, &req.mfa_token, &pending).await?;
            Ok(Json(SessionResponse { token }))
        }
        Err(refused @ DomainError::Unauthorized(_)) => {
            state
                .events
                .publish_security_event(
                    SecurityEvent::MfaVerificationFailed {
                        user_id: pending.user_id,
                    },
                    Some(pending.user_id),
                )
                .await
                .ok();
            Err(refused.into())
        }
        Err(e) => Err(e.into()),
    }
}

/// Held from the "no factor yet" check until the token is spent. The token is checked again once the lock is
/// taken, so the second of two concurrent setups fails before it replaces the first one's backup codes.
async fn first_setup_guard<'a>(
    state: &'a AppState,
    mfa_token: &str,
    user_id: Uuid,
) -> Result<tokio::sync::MutexGuard<'a, ()>, ApiError> {
    let guard = state.first_setup_locks.lock(user_id).await;
    if state.mfa_spent_tokens.is_spent(mfa_token) {
        return Err(invalid_token());
    }
    Ok(guard)
}

/// The pending token only proves the password. If a user with a passkey could enrol a TOTP through it, anyone
/// with the password could skip the passkey.
async fn refuse_when_a_passkey_exists(state: &AppState, user_id: Uuid) -> Result<(), ApiError> {
    if state.mfa.factors(user_id).await?.has_passkey {
        return Err(DomainError::Validation(ALREADY_SET_UP.to_string()).into());
    }
    Ok(())
}

async fn enroll(
    State(state): State<AppState>,
    Json(req): Json<SetupTokenRequest>,
) -> Result<Json<EnrollResponse>, ApiError> {
    let pending = pending_user(&state, &req.mfa_token).await?;
    // Under the lock too, or an enrolment started just after a passkey setup finished would survive it.
    let _setup = first_setup_guard(&state, &req.mfa_token, pending.user_id).await?;
    refuse_when_a_passkey_exists(&state, pending.user_id).await?;
    let enrollment = state.mfa.enroll_totp(pending.user_id).await?;
    Ok(Json(EnrollResponse {
        secret: enrollment.secret_base32,
        otpauth_url: enrollment.otpauth_url,
    }))
}

/// Refused like the first step if a passkey appeared since the enrolment started.
async fn confirm(
    State(state): State<AppState>,
    Json(req): Json<ConfirmRequest>,
) -> Result<Json<SetupCompleteResponse>, ApiError> {
    let pending = pending_user(&state, &req.mfa_token).await?;
    let _setup = first_setup_guard(&state, &req.mfa_token, pending.user_id).await?;
    refuse_when_a_passkey_exists(&state, pending.user_id).await?;
    let backup_codes = state.mfa.confirm_totp(pending.user_id, &req.code).await?;
    state
        .events
        .publish_security_event(
            SecurityEvent::MfaEnrolled {
                user_id: pending.user_id,
            },
            Some(pending.user_id),
        )
        .await
        .ok();
    notify_enrolled(&state, pending.user_id, TOTP_METHOD_LABEL).await;
    let token = complete_login(&state, &req.mfa_token, &pending).await?;
    Ok(Json(SetupCompleteResponse {
        token,
        backup_codes,
    }))
}

/// A mail failure doesn't undo an enrolment that already succeeded.
async fn notify_enrolled(state: &AppState, user_id: Uuid, method_label: &str) {
    if let Ok(Some(user)) = state.users.find_by_id(user_id).await
        && is_valid_mailbox(&user.email)
    {
        state.mailer.send_in_background(
            user.email.clone(),
            email_templates::mfa_enrolled(&user.username, method_label),
        );
    }
}

struct StrictJson<T>(T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for StrictJson<T> {
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(Self(value)),
            Err(rejection) if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE => {
                Err(rejection.into_response())
            }
            Err(_) => Err(bad_body().into_response()),
        }
    }
}

fn bad_body() -> ApiError {
    DomainError::Validation("invalid request body".to_string()).into()
}

/// Raw JSON, so the handler picks where it's parsed: after the availability check, before the token lookup.
fn parse_credential<T: DeserializeOwned>(credential: serde_json::Value) -> Result<T, ApiError> {
    serde_json::from_value(credential).map_err(|_| bad_body())
}

fn ensure_passkeys_available(state: &AppState) -> Result<(), ApiError> {
    if state.passkeys.available() {
        Ok(())
    } else {
        Err(DomainError::ServiceUnavailable(PASSKEYS_UNAVAILABLE.to_string()).into())
    }
}

/// Spent first, since every start parks a ceremony in memory whatever the token.
fn within_start_budget(
    state: &AppState,
    connect_info: Option<std::net::SocketAddr>,
    headers: &HeaderMap,
) -> Result<(), ApiError> {
    if state
        .passkey_start_limiter
        .check(client_ip(state, connect_info, headers))
    {
        Ok(())
    } else {
        Err(DomainError::RateLimited(TOO_MANY_ATTEMPTS.to_string()).into())
    }
}

/// webauthn-rs nests the challenge under a second `publicKey`, which we unwrap.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChallengeResponse {
    challenge_id: Uuid,
    public_key: serde_json::Value,
}

fn challenge_response(
    challenge_id: Uuid,
    public_key: &impl Serialize,
) -> Result<Json<ChallengeResponse>, ApiError> {
    let public_key =
        serde_json::to_value(public_key).map_err(|e| DomainError::Infrastructure(e.to_string()))?;
    Ok(Json(ChallengeResponse {
        challenge_id,
        public_key,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssertionRequest {
    mfa_token: String,
    challenge_id: Uuid,
    credential: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetupPasskeyFinishRequest {
    mfa_token: String,
    challenge_id: Uuid,
    credential: serde_json::Value,
    name: String,
}

async fn passkey_start(
    MaybeConnectInfo(connect_info): MaybeConnectInfo,
    headers: HeaderMap,
    State(state): State<AppState>,
    StrictJson(req): StrictJson<SetupTokenRequest>,
) -> Result<Json<ChallengeResponse>, ApiError> {
    within_start_budget(&state, connect_info, &headers)?;
    ensure_passkeys_available(&state)?;
    // Not counted against the per-user budget: a start verifies nothing, and a dismissed prompt shouldn't lock
    // the user out of every MFA path. The per-IP budget and the per-user ceremony cap bound the cost.
    let pending = verified_pending(&state, &req.mfa_token).await?;
    let (challenge_id, challenge) = state.passkeys.start_authentication(pending.user_id).await?;
    challenge_response(challenge_id, &challenge.public_key)
}

/// A failure is the same 401 as a wrong TOTP code and doesn't spend the token.
async fn passkey_finish(
    State(state): State<AppState>,
    StrictJson(req): StrictJson<AssertionRequest>,
) -> Result<Json<SessionResponse>, ApiError> {
    ensure_passkeys_available(&state)?;
    let credential: PublicKeyCredential = parse_credential(req.credential)?;
    let pending = pending_user(&state, &req.mfa_token).await?;
    match state
        .passkeys
        .finish_authentication(pending.user_id, req.challenge_id, &credential)
        .await
    {
        Ok(()) => {
            let token = complete_login(&state, &req.mfa_token, &pending).await?;
            Ok(Json(SessionResponse { token }))
        }
        // Passkey deleted between the assertion and the counter write: a failed login, not a 404.
        Err(DomainError::Unauthorized(_) | DomainError::NotFound(_)) => {
            state
                .events
                .publish_security_event(
                    SecurityEvent::PasskeyVerificationFailed {
                        user_id: pending.user_id,
                    },
                    Some(pending.user_id),
                )
                .await
                .ok();
            Err(DomainError::Unauthorized("invalid code".to_string()).into())
        }
        Err(e) => Err(e.into()),
    }
}

async fn setup_passkey_start(
    MaybeConnectInfo(connect_info): MaybeConnectInfo,
    headers: HeaderMap,
    State(state): State<AppState>,
    StrictJson(req): StrictJson<SetupTokenRequest>,
) -> Result<Json<ChallengeResponse>, ApiError> {
    within_start_budget(&state, connect_info, &headers)?;
    ensure_passkeys_available(&state)?;
    let pending = verified_pending(&state, &req.mfa_token).await?;
    if state.mfa.factors(pending.user_id).await?.any() {
        return Err(DomainError::Validation(ALREADY_SET_UP.to_string()).into());
    }
    let (challenge_id, challenge) = state.passkeys.start_registration(pending.user_id).await?;
    challenge_response(challenge_id, &challenge.public_key)
}

/// `finish_first_passkey_setup` re-checks that no factor exists before taking the ceremony, in case of a
/// start/finish race.
async fn setup_passkey_finish(
    State(state): State<AppState>,
    StrictJson(req): StrictJson<SetupPasskeyFinishRequest>,
) -> Result<Json<SetupCompleteResponse>, ApiError> {
    ensure_passkeys_available(&state)?;
    let credential: RegisterPublicKeyCredential = parse_credential(req.credential)?;
    let pending = pending_user(&state, &req.mfa_token).await?;
    let _setup = first_setup_guard(&state, &req.mfa_token, pending.user_id).await?;
    let (_passkey, backup_codes) = state
        .mfa
        .finish_first_passkey_setup(
            &state.passkeys,
            pending.user_id,
            req.challenge_id,
            &credential,
            &req.name,
        )
        .await?;
    state
        .events
        .publish_security_event(
            SecurityEvent::PasskeyAdded {
                user_id: pending.user_id,
            },
            Some(pending.user_id),
        )
        .await
        .ok();
    notify_enrolled(&state, pending.user_id, PASSKEY_METHOD_LABEL).await;
    let token = complete_login(&state, &req.mfa_token, &pending).await?;
    Ok(Json(SetupCompleteResponse {
        token,
        backup_codes,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PasswordRequest {
    current_password: String,
}

#[derive(Deserialize)]
struct CodeRequest {
    code: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusResponse {
    totp_enabled: bool,
    backup_codes_remaining: i64,
    passkeys: Vec<PasskeyResponse>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PasskeyResponse {
    id: Uuid,
    name: String,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
}

impl From<StoredPasskey> for PasskeyResponse {
    fn from(passkey: StoredPasskey) -> Self {
        Self {
            id: passkey.id,
            name: passkey.name,
            created_at: passkey.created_at,
            last_used_at: passkey.last_used_at,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterFinishRequest {
    challenge_id: Uuid,
    credential: serde_json::Value,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupCodesResponse {
    backup_codes: Vec<String>,
}

/// Called before the business call whatever its outcome, so a wrong password can't be probed faster than a wrong
/// code.
pub(crate) fn within_budget(state: &AppState, user_id: Uuid) -> Result<(), ApiError> {
    if state.mfa_limiter.check(user_id) {
        Ok(())
    } else {
        Err(DomainError::RateLimited(TOO_MANY_ATTEMPTS.to_string()).into())
    }
}

/// Not counted against the limiter, there's nothing in the answer to guess at.
async fn status(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
) -> Result<Json<StatusResponse>, ApiError> {
    let status = state.mfa.status(user_id).await?;
    let passkeys = state
        .passkeys
        .list(user_id)
        .await?
        .into_iter()
        .map(PasskeyResponse::from)
        .collect();
    Ok(Json(StatusResponse {
        totp_enabled: status.totp_enabled,
        backup_codes_remaining: status.backup_codes_remaining,
        passkeys,
    }))
}

/// 400 while a confirmed TOTP exists, so a stolen session can't quietly swap the factor.
async fn enroll_self(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<PasswordRequest>,
) -> Result<Json<EnrollResponse>, ApiError> {
    within_budget(&state, user_id)?;
    state
        .mfa
        .check_password(user_id, &req.current_password)
        .await?;
    let enrollment = state.mfa.enroll_totp(user_id).await?;
    Ok(Json(EnrollResponse {
        secret: enrollment.secret_base32,
        otpauth_url: enrollment.otpauth_url,
    }))
}

async fn confirm_self(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CodeRequest>,
) -> Result<Json<BackupCodesResponse>, ApiError> {
    within_budget(&state, user_id)?;
    let backup_codes = state.mfa.confirm_totp(user_id, &req.code).await?;
    state
        .events
        .publish_security_event(SecurityEvent::MfaEnrolled { user_id }, Some(user_id))
        .await
        .ok();
    notify_enrolled(&state, user_id, TOTP_METHOD_LABEL).await;
    Ok(Json(BackupCodesResponse { backup_codes }))
}

async fn regenerate(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<PasswordRequest>,
) -> Result<Json<BackupCodesResponse>, ApiError> {
    within_budget(&state, user_id)?;
    let backup_codes = state
        .mfa
        .regenerate_backup_codes(user_id, &req.current_password)
        .await?;
    Ok(Json(BackupCodesResponse { backup_codes }))
}

/// Bumps the epoch before deleting: if the delete fails, the sessions are dead and the factor intact, never the
/// reverse. Backup codes stay while a passkey remains.
async fn disable(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<PasswordRequest>,
) -> Result<StatusCode, ApiError> {
    within_budget(&state, user_id)?;
    state
        .mfa
        .check_password(user_id, &req.current_password)
        .await?;
    state.users.bump_token_epoch(user_id).await?;
    state.mfa.remove_totp(user_id).await?;
    state
        .events
        .publish_security_event(SecurityEvent::MfaDisabled { user_id }, Some(user_id))
        .await
        .ok();
    Ok(StatusCode::NO_CONTENT)
}

/// Needs the current password (a stolen session shouldn't be able to plant an attacker's authenticator) and spends
/// the per-user budget before the password is checked.
async fn register_passkey_start(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    StrictJson(req): StrictJson<PasswordRequest>,
) -> Result<Json<ChallengeResponse>, ApiError> {
    ensure_passkeys_available(&state)?;
    within_budget(&state, user_id)?;
    state
        .mfa
        .check_password(user_id, &req.current_password)
        .await?;
    let (challenge_id, challenge) = state.passkeys.start_registration(user_id).await?;
    challenge_response(challenge_id, &challenge.public_key)
}

async fn register_passkey_finish(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    StrictJson(req): StrictJson<RegisterFinishRequest>,
) -> Result<(StatusCode, Json<PasskeyResponse>), ApiError> {
    ensure_passkeys_available(&state)?;
    let credential: RegisterPublicKeyCredential = parse_credential(req.credential)?;
    within_budget(&state, user_id)?;
    let passkey = state
        .passkeys
        .finish_registration(user_id, req.challenge_id, &credential, &req.name)
        .await?;
    state
        .events
        .publish_security_event(SecurityEvent::PasskeyAdded { user_id }, Some(user_id))
        .await
        .ok();
    notify_enrolled(&state, user_id, PASSKEY_METHOD_LABEL).await;
    Ok((StatusCode::CREATED, Json(passkey.into())))
}

/// 404 for someone else's or an unknown passkey, without signing anyone out. Bumps the epoch before removing, like
/// `disable`. Works when passkeys are unavailable.
async fn delete_passkey(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(passkey_id): Path<Uuid>,
    StrictJson(req): StrictJson<PasswordRequest>,
) -> Result<StatusCode, ApiError> {
    within_budget(&state, user_id)?;
    state
        .mfa
        .check_password(user_id, &req.current_password)
        .await?;
    // Before the bump, so a stale or foreign id is a plain 404 that doesn't log anyone out.
    if !state
        .passkeys
        .list(user_id)
        .await?
        .iter()
        .any(|passkey| passkey.id == passkey_id)
    {
        return Err(DomainError::NotFound("passkey".to_string()).into());
    }
    state.users.bump_token_epoch(user_id).await?;
    state.mfa.remove_passkey(user_id, passkey_id).await?;
    state
        .events
        .publish_security_event(SecurityEvent::PasskeyDeleted { user_id }, Some(user_id))
        .await
        .ok();
    Ok(StatusCode::NO_CONTENT)
}

fn self_service_router() -> Router<AppState> {
    Router::new()
        .route("/me/mfa", get(status))
        .route(
            "/me/mfa/passkeys/register/start",
            post(register_passkey_start),
        )
        .route(
            "/me/mfa/passkeys/register/finish",
            post(register_passkey_finish),
        )
        .route("/me/mfa/passkeys/{id}/delete", post(delete_passkey))
        .route("/me/mfa/totp/enroll", post(enroll_self))
        .route("/me/mfa/totp/confirm", post(confirm_self))
        .route("/me/mfa/totp/disable", post(disable))
        .route("/me/mfa/backup-codes/regenerate", post(regenerate))
}

pub fn router() -> Router<AppState> {
    // layer() only wraps the routes added before it, so the body limit hits the unauthenticated ones alone.
    let unauthenticated = Router::new()
        .route("/auth/mfa/verify", post(verify))
        .route("/auth/mfa/setup/totp/enroll", post(enroll))
        .route("/auth/mfa/setup/totp/confirm", post(confirm))
        .route("/auth/mfa/passkey/start", post(passkey_start))
        .route("/auth/mfa/passkey/finish", post(passkey_finish))
        .route("/auth/mfa/setup/passkey/start", post(setup_passkey_start))
        .route("/auth/mfa/setup/passkey/finish", post(setup_passkey_finish))
        .layer(DefaultBodyLimit::max(UNAUTHENTICATED_BODY_LIMIT_BYTES));
    unauthenticated.merge(self_service_router())
}
