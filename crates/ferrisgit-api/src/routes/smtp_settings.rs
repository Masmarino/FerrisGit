use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use ferrisgit_application::use_cases::smtp_settings::{
    SendTestEmailUseCase, SmtpSettingsInput, UpdateSmtpSettingsUseCase,
};
use ferrisgit_domain::email::SmtpSettings;
use ferrisgit_domain::error::DomainError;
use serde::{Deserialize, Serialize};

use crate::auth_middleware::AdminUser;
use crate::error::ApiError;
use crate::state::AppState;

// The password is stored encrypted and never sent back, only whether one is set.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SmtpSettingsResponse {
    configured: bool,
    host: String,
    port: u16,
    security: String,
    username: String,
    password_set: bool,
    from_address: String,
    from_name: String,
}

impl SmtpSettingsResponse {
    fn unconfigured() -> Self {
        Self {
            configured: false,
            host: String::new(),
            port: 587,
            security: "starttls".to_string(),
            username: String::new(),
            password_set: false,
            from_address: String::new(),
            from_name: "FerrisGit".to_string(),
        }
    }

    fn from_settings(s: SmtpSettings) -> Self {
        Self {
            configured: true,
            host: s.host,
            port: s.port,
            security: s.security.as_str().to_string(),
            username: s.username,
            password_set: s.password.is_some(),
            from_address: s.from_address,
            from_name: s.from_name,
        }
    }
}

// No Debug derive, it holds the plaintext password.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SmtpSettingsRequest {
    host: String,
    port: i32,
    security: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: Option<String>,
    from_address: String,
    #[serde(default)]
    from_name: String,
}

#[derive(Deserialize)]
struct TestEmailRequest {
    to: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TestEmailResponse {
    sent: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

async fn get_smtp(
    AdminUser(_admin_id): AdminUser,
    State(state): State<AppState>,
) -> Result<Json<SmtpSettingsResponse>, ApiError> {
    let response = match state.smtp_settings.get().await? {
        Some(settings) => SmtpSettingsResponse::from_settings(settings),
        None => SmtpSettingsResponse::unconfigured(),
    };
    Ok(Json(response))
}

async fn put_smtp(
    AdminUser(_admin_id): AdminUser,
    State(state): State<AppState>,
    Json(req): Json<SmtpSettingsRequest>,
) -> Result<Json<SmtpSettingsResponse>, ApiError> {
    let input = SmtpSettingsInput {
        host: req.host,
        port: req.port,
        security: req.security,
        username: req.username,
        password: req.password,
        from_address: req.from_address,
        from_name: req.from_name,
    };
    let saved = UpdateSmtpSettingsUseCase::new(state.smtp_settings.clone())
        .execute(input)
        .await?;
    Ok(Json(SmtpSettingsResponse::from_settings(saved)))
}

/// A delivery problem (bad credentials, unreachable server, nothing configured) is what this endpoint is there to
/// show, so it's a `200 { sent: false, error }`. Only a bad recipient is a 400.
async fn test_smtp(
    AdminUser(_admin_id): AdminUser,
    State(state): State<AppState>,
    Json(req): Json<TestEmailRequest>,
) -> Result<Json<TestEmailResponse>, ApiError> {
    match SendTestEmailUseCase::new(state.mailer.clone())
        .execute(&req.to)
        .await
    {
        Ok(()) => Ok(Json(TestEmailResponse {
            sent: true,
            error: None,
        })),
        Err(e @ DomainError::Validation(_)) => Err(e.into()),
        Err(DomainError::Infrastructure(message)) => Ok(Json(TestEmailResponse {
            sent: false,
            error: Some(message),
        })),
        Err(other) => Ok(Json(TestEmailResponse {
            sent: false,
            error: Some(other.to_string()),
        })),
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/settings/smtp", get(get_smtp).put(put_smtp))
        .route("/admin/settings/smtp/test", post(test_smtp))
}
