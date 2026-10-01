use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::email_templates;
use ferrisgit_application::use_cases::admin_reset_password::{
    AdminResetPasswordUseCase, IssuedPasswordReset,
};
use ferrisgit_application::use_cases::create_user::CreateUserUseCase;
use ferrisgit_application::use_cases::delete_user::DeleteUserUseCase;
use ferrisgit_application::use_cases::invitations::{
    InviteUserUseCase, InvitedUser, ResendInvitationUseCase,
};
use ferrisgit_application::use_cases::list_users::{ListUsersUseCase, UserListing, UserState};
use ferrisgit_application::use_cases::set_admin::SetAdminUseCase;
use ferrisgit_domain::audit::SecurityEvent;
use ferrisgit_domain::email::is_valid_mailbox;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::user::User;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::AdminUser;
use crate::error::ApiError;
use crate::routes::repositories::remove_git_storage;
use crate::state::AppState;

#[derive(Deserialize)]
struct CreateUserRequest {
    username: String,
    email: String,
    password: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UserResponse {
    id: Uuid,
    username: String,
    email: String,
    is_admin: bool,
    created_at: DateTime<Utc>,
}

async fn create(
    AdminUser(_admin_id): AdminUser,
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<UserResponse>, ApiError> {
    let use_case = CreateUserUseCase::new(
        state.users.clone(),
        state.hasher.clone(),
        state.groups.clone(),
    );
    let user = use_case
        .execute(req.username, req.email, req.password)
        .await?;
    Ok(Json(UserResponse {
        id: user.id,
        username: user.username,
        email: user.email,
        is_admin: user.is_admin,
        created_at: user.created_at,
    }))
}

/// Bumps the target's token epoch, which kills their sessions and pending `mfa` tokens (the admin's own session
/// only if they reset themselves), then deletes their factors. Idempotent. 404 for an unknown user.
async fn reset_mfa(
    AdminUser(admin_id): AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let user = state
        .users
        .find_by_id(user_id)
        .await?
        .ok_or_else(|| DomainError::NotFound("user".to_string()))?;
    // Bump first, delete second. If the deletion fails, the sessions are dead and the factor intact (a retry
    // finishes the job), never the other way round.
    state.users.bump_token_epoch(user_id).await?;
    state.mfa.reset(user_id).await?;
    state
        .events
        .publish_security_event(
            SecurityEvent::MfaResetByAdmin {
                target_user_id: user_id,
            },
            Some(admin_id),
        )
        .await
        .ok();
    // Best effort: mail failures never fail a reset that happened.
    if is_valid_mailbox(&user.email) {
        state.mailer.send_in_background(
            user.email.clone(),
            email_templates::mfa_reset(&user.username),
        );
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminUserRow {
    id: Uuid,
    username: String,
    email: String,
    is_admin: bool,
    created_at: DateTime<Utc>,
    state: &'static str,
    invitation_expires_at: Option<DateTime<Utc>>,
    mfa_enabled: bool,
}

impl AdminUserRow {
    fn new(user: User, state: UserState, mfa_enabled: bool) -> Self {
        let (state, invitation_expires_at) = match state {
            UserState::Active => ("active", None),
            UserState::Invited { expires_at } => ("invited", Some(expires_at)),
        };
        Self {
            id: user.id,
            username: user.username,
            email: user.email,
            is_admin: user.is_admin,
            created_at: user.created_at,
            state,
            invitation_expires_at,
            mfa_enabled,
        }
    }
}

impl From<UserListing> for AdminUserRow {
    fn from(listing: UserListing) -> Self {
        Self::new(listing.user, listing.state, listing.mfa_enabled)
    }
}

/// No pagination, capped at 1000 rows (instance-scale data).
async fn list(
    AdminUser(_admin_id): AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<AdminUserRow>>, ApiError> {
    let listings = ListUsersUseCase::new(
        state.users.clone(),
        state.invitations.clone(),
        state.totp_credentials.clone(),
        state.passkey_credentials.clone(),
    )
    .execute()
    .await?;
    Ok(Json(listings.into_iter().map(AdminUserRow::from).collect()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InviteRequest {
    username: String,
    email: String,
    #[serde(default)]
    is_admin: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InvitationResponse {
    user: AdminUserRow,
    email_sent: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    email_error: Option<String>,
    /// Only present when the mail could not be sent, so the admin can hand it over by other means.
    #[serde(skip_serializing_if = "Option::is_none")]
    activation_url: Option<String>,
}

/// The mail is awaited so the admin learns whether it went out, but a delivery failure never fails the request.
/// The token sits in the URL fragment (`#token=`), which browsers never send to a server, so it stays out of
/// access logs. Neither the token nor the URL is logged.
async fn deliver_invitation(
    state: &AppState,
    invited: InvitedUser,
) -> Result<Json<InvitationResponse>, ApiError> {
    let InvitedUser { user, token } = invited;
    let activation_url = format!("{}/activate#token={}", state.config.public_url, token);
    let expires_at = state
        .invitations
        .expiries(&[user.id])
        .await?
        .into_iter()
        .find(|(id, _)| *id == user.id)
        .map(|(_, expires_at)| expires_at);
    let mfa_enabled = state
        .totp_credentials
        .confirmed_user_ids(&[user.id])
        .await?
        .contains(&user.id)
        || state
            .passkey_credentials
            .user_ids_with_passkeys(&[user.id])
            .await?
            .contains(&user.id);
    let user_state = expires_at.map_or(UserState::Active, |expires_at| UserState::Invited {
        expires_at,
    });
    let user_id = user.id;
    let to = user.email.clone();
    let username = user.username.clone();
    let row = AdminUserRow::new(user, user_state, mfa_enabled);

    // Reads come first: a database failure must not surface after the mail already left.
    let outcome = state
        .mailer
        .send(
            &to,
            email_templates::account_created(&username, &activation_url),
        )
        .await;

    Ok(Json(match outcome {
        Ok(()) => InvitationResponse {
            user: row,
            email_sent: true,
            email_error: None,
            activation_url: None,
        },
        Err(error) => {
            tracing::warn!(%user_id, %error, "could not send the invitation e-mail");
            let reason = match error {
                DomainError::Infrastructure(message) => message,
                other => other.to_string(),
            };
            InvitationResponse {
                user: row,
                email_sent: false,
                email_error: Some(reason),
                activation_url: Some(activation_url),
            }
        }
    }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PasswordResetResponse {
    email_sent: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    email_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reset_url: Option<String>,
}

/// Revokes every session of the target and kills their current password at once, then mails a 1 h link. 400 for
/// the admin's own account (a sole admin could lock themselves out with nobody to issue a link) and for an account
/// pending activation. Like `deliver_invitation`, the mail is awaited and the link is returned only when it did not
/// go out (SMTP failure, or an address that is not a deliverable mailbox such as the bootstrap admin's
/// `@localhost`).
async fn reset_password(
    AdminUser(admin_id): AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<PasswordResetResponse>, ApiError> {
    let IssuedPasswordReset { user, token } = AdminResetPasswordUseCase::new(
        state.users.clone(),
        state.hasher.clone(),
        state.invitations.clone(),
        state.password_resets.clone(),
    )
    .execute(admin_id, user_id)
    .await?;
    state
        .events
        .publish_security_event(
            SecurityEvent::PasswordResetByAdmin {
                target_user_id: user.id,
            },
            Some(admin_id),
        )
        .await
        .ok();
    let reset_url = format!("{}/reset-password#token={}", state.config.public_url, token);

    if !is_valid_mailbox(&user.email) {
        return Ok(Json(PasswordResetResponse {
            email_sent: false,
            email_error: Some("the user has no deliverable e-mail address".to_string()),
            reset_url: Some(reset_url),
        }));
    }
    Ok(Json(
        match state
            .mailer
            .send(
                &user.email,
                email_templates::password_reset(&user.username, &reset_url),
            )
            .await
        {
            Ok(()) => PasswordResetResponse {
                email_sent: true,
                email_error: None,
                reset_url: None,
            },
            Err(error) => {
                tracing::warn!(%user_id, %error, "could not send the password reset e-mail");
                let reason = match error {
                    DomainError::Infrastructure(message) => message,
                    other => other.to_string(),
                };
                PasswordResetResponse {
                    email_sent: false,
                    email_error: Some(reason),
                    reset_url: Some(reset_url),
                }
            }
        },
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetAdminRequest {
    is_admin: bool,
}

/// 409 for demoting the last active admin (an admin pending activation does not count). Effective on the target's
/// next request since `AdminUser` reads the flag from the database. Audited only when the flag actually changed.
async fn set_admin(
    AdminUser(admin_id): AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Json(req): Json<SetAdminRequest>,
) -> Result<StatusCode, ApiError> {
    let changed = SetAdminUseCase::new(
        state.users.clone(),
        state.invitations.clone(),
        state.password_resets.clone(),
    )
    .execute(user_id, req.is_admin)
    .await?;
    if changed {
        let event = if req.is_admin {
            SecurityEvent::AdminGranted {
                target_user_id: user_id,
            }
        } else {
            SecurityEvent::AdminRevoked {
                target_user_id: user_id,
            }
        };
        state
            .events
            .publish_security_event(event, Some(admin_id))
            .await
            .ok();
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn invite(
    AdminUser(_admin_id): AdminUser,
    State(state): State<AppState>,
    Json(req): Json<InviteRequest>,
) -> Result<Json<InvitationResponse>, ApiError> {
    let use_case = InviteUserUseCase::new(
        state.users.clone(),
        state.hasher.clone(),
        state.groups.clone(),
        state.invitations.clone(),
    );
    let invited = use_case
        .execute(req.username, req.email, req.is_admin)
        .await?;
    deliver_invitation(&state, invited).await
}

async fn resend_invitation(
    AdminUser(_admin_id): AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<InvitationResponse>, ApiError> {
    let invited = ResendInvitationUseCase::new(state.users.clone(), state.invitations.clone())
        .execute(user_id)
        .await?;
    deliver_invitation(&state, invited).await
}

/// Metadata and size only, never file content, so this route is no way into what the user wrote. Group
/// repositories they created are not listed because they belong to the group.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminUserRepositoryRow {
    id: Uuid,
    name: String,
    description: String,
    visibility: &'static str,
    created_at: DateTime<Utc>,
    size_bytes: Option<u64>,
}

/// All sizes are computed in a single blocking task, off the async runtime.
async fn list_repositories(
    AdminUser(_admin_id): AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<Vec<AdminUserRepositoryRow>>, ApiError> {
    state
        .users
        .find_by_id(user_id)
        .await?
        .ok_or_else(|| DomainError::NotFound("user".to_string()))?;
    let repositories = state.repositories.list_for_owner(user_id).await?;
    let git_backend = state.git_backend.clone();
    let disk_paths: Vec<String> = repositories.iter().map(|r| r.disk_path.clone()).collect();
    let count = disk_paths.len();
    // A size that cannot be computed is `null`, never an error.
    let sizes: Vec<Option<u64>> = tokio::task::spawn_blocking(move || {
        disk_paths
            .iter()
            .map(|disk_path| git_backend.directory_size(disk_path).ok())
            .collect()
    })
    .await
    .unwrap_or_else(|_| vec![None; count]);
    Ok(Json(
        repositories
            .into_iter()
            .zip(sizes)
            .map(|(repo, size_bytes)| AdminUserRepositoryRow {
                id: repo.id,
                name: repo.name,
                description: repo.description,
                visibility: repo.visibility.as_str(),
                created_at: repo.created_at,
                size_bytes,
            })
            .collect(),
    ))
}

/// Deletes the account and personal repositories in one transaction, then their storage on disk. What they wrote
/// elsewhere stays, attributed to a deleted user. 409 for the last active admin or the last Maintainer of a group
/// hierarchy. Every refusal comes before anything is written. The audit event keeps the username and destroyed
/// repository names, since the rows are gone.
async fn delete_user(
    AdminUser(admin_id): AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let deleted = DeleteUserUseCase::new(
        state.users.clone(),
        state.invitations.clone(),
        state.password_resets.clone(),
        state.release_asset_storage.clone(),
        state.groups.clone(),
        state.group_membership.clone(),
    )
    .execute(admin_id, user_id, |repo| {
        remove_git_storage(&state.git_backend, repo)
    })
    .await?;
    let event = SecurityEvent::UserDeletedByAdmin {
        target_user_id: user_id,
        username: deleted.username,
        deleted_repositories: deleted.deleted_repositories,
    };
    state
        .events
        .publish_security_event(event, Some(admin_id))
        .await
        .ok();
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/users", post(create).get(list))
        .route("/admin/users/invite", post(invite))
        .route("/admin/users/{id}/invitation", post(resend_invitation))
        .route("/admin/users/{id}/mfa", delete(reset_mfa))
        .route("/admin/users/{id}/reset-password", post(reset_password))
        .route("/admin/users/{id}", delete(delete_user))
        .route("/admin/users/{id}/repositories", get(list_repositories))
        .route("/admin/users/{id}/admin", put(set_admin))
}
