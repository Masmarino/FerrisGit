use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_domain::protected_branch::ProtectedBranch;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::authz::require_role_by_id;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProtectedBranchResponse {
    id: Uuid,
    pattern: String,
    required_approvals: i32,
    required_checks: Vec<String>,
    created_at: DateTime<Utc>,
}

impl From<ProtectedBranch> for ProtectedBranchResponse {
    fn from(rule: ProtectedBranch) -> Self {
        Self {
            id: rule.id,
            pattern: rule.pattern,
            required_approvals: rule.required_approvals,
            required_checks: rule.required_checks,
            created_at: rule.created_at,
        }
    }
}

/// Readable by anyone who reads the repository, so the interface can tell which branches only take merge requests.
async fn list_protected_branches(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
) -> Result<Json<Vec<ProtectedBranchResponse>>, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Reader).await?;
    let rules = state.protected_branches.list(repository_id).await?;
    Ok(Json(rules.into_iter().map(Into::into).collect()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProtectBranchRequest {
    pattern: String,
    #[serde(default)]
    required_approvals: i32,
    #[serde(default)]
    required_checks: Vec<String>,
}

/// Only the fields sent change.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProtectedBranchRequest {
    #[serde(default)]
    required_approvals: Option<i32>,
    #[serde(default)]
    required_checks: Option<Vec<String>>,
}

async fn protect_branch(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
    Json(req): Json<ProtectBranchRequest>,
) -> Result<(StatusCode, Json<ProtectedBranchResponse>), ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    let rule = state
        .protected_branches
        .protect(
            repository_id,
            &req.pattern,
            req.required_approvals,
            &req.required_checks,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(rule.into())))
}

async fn update_protected_branch(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateProtectedBranchRequest>,
) -> Result<Json<ProtectedBranchResponse>, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    let rule = state
        .protected_branches
        .update_requirements(
            repository_id,
            id,
            req.required_approvals,
            req.required_checks,
        )
        .await?;
    Ok(Json(rule.into()))
}

async fn unprotect_branch(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    state
        .protected_branches
        .unprotect(repository_id, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/repositories/{repository_id}/protected-branches",
            get(list_protected_branches).post(protect_branch),
        )
        .route(
            "/repositories/{repository_id}/protected-branches/{id}",
            put(update_protected_branch).delete(unprotect_branch),
        )
}
