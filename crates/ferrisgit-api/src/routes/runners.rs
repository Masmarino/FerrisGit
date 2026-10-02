use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::routing::{delete, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::token_hash::hash_token;
use ferrisgit_application::use_cases::delete_runner::DeleteRunnerUseCase;
use ferrisgit_application::use_cases::register_runner::RegisterRunnerUseCase;
use ferrisgit_domain::error::DomainError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::AdminUser;
use crate::error::ApiError;
use crate::state::AppState;

/// Constant time, so comparing a caller-supplied hash can't leak how many leading bytes matched.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Deserialize)]
struct RegisterRunnerRequest {
    name: String,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RegisterRunnerResponse {
    id: Uuid,
    name: String,
    tags: Vec<String>,
    token: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunnerSummary {
    id: Uuid,
    name: String,
    tags: Vec<String>,
    last_heartbeat_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

async fn register(
    AdminUser(_admin_id): AdminUser,
    State(state): State<AppState>,
    Json(req): Json<RegisterRunnerRequest>,
) -> Result<Json<RegisterRunnerResponse>, ApiError> {
    let use_case = RegisterRunnerUseCase::new(state.runners.clone());
    let (runner, token) = use_case.execute(req.name, req.tags).await?;
    Ok(Json(RegisterRunnerResponse {
        id: runner.id,
        name: runner.name,
        tags: runner.tags,
        token,
    }))
}

async fn list(
    AdminUser(_admin_id): AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<RunnerSummary>>, ApiError> {
    let runners = state.runners.list().await?;
    Ok(Json(
        runners
            .into_iter()
            .map(|r| RunnerSummary {
                id: r.id,
                name: r.name,
                tags: r.tags,
                last_heartbeat_at: r.last_heartbeat_at,
                created_at: r.created_at,
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
struct SelfRegisterRunnerRequest {
    registration_token: String,
    name: String,
    #[serde(default)]
    tags: Vec<String>,
}

async fn self_register(
    State(state): State<AppState>,
    Json(req): Json<SelfRegisterRunnerRequest>,
) -> Result<Json<RegisterRunnerResponse>, ApiError> {
    let settings = state.system_settings.get().await?;
    let Some(configured_token_hash) = settings.runner_registration_token else {
        return Err(ApiError(DomainError::Unauthorized(
            "self-service runner registration is disabled".to_string(),
        )));
    };
    if !constant_time_eq(&hash_token(&req.registration_token), &configured_token_hash) {
        return Err(ApiError(DomainError::Unauthorized(
            "invalid registration token".to_string(),
        )));
    }

    let use_case = RegisterRunnerUseCase::new(state.runners.clone());
    let (runner, token) = use_case.execute(req.name, req.tags).await?;
    Ok(Json(RegisterRunnerResponse {
        id: runner.id,
        name: runner.name,
        tags: runner.tags,
        token,
    }))
}

/// The only way to invalidate a runner token: they never expire and read across the whole instance.
async fn revoke(
    AdminUser(_admin_id): AdminUser,
    State(state): State<AppState>,
    AxumPath(runner_id): AxumPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    let use_case = DeleteRunnerUseCase::new(state.runners.clone(), state.jobs.clone());
    use_case.execute(runner_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/runners", post(register).get(list))
        .route("/admin/runners/{id}", delete(revoke))
        .route("/runner/register", post(self_register))
}

#[cfg(test)]
mod constant_time_eq_tests {
    use super::constant_time_eq;

    #[test]
    fn equal_strings_are_equal() {
        assert!(constant_time_eq("abc123", "abc123"));
    }

    #[test]
    fn different_strings_of_the_same_length_are_not_equal() {
        assert!(!constant_time_eq("abc123", "abc124"));
    }

    #[test]
    fn strings_of_different_lengths_are_not_equal() {
        assert!(!constant_time_eq("abc", "abc123"));
    }
}
