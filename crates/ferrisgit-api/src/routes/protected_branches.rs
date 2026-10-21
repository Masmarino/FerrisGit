use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get};
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
    created_at: DateTime<Utc>,
}

impl From<ProtectedBranch> for ProtectedBranchResponse {
    fn from(rule: ProtectedBranch) -> Self {
        Self {
            id: rule.id,
            pattern: rule.pattern,
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
struct ProtectBranchRequest {
    pattern: String,
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
        .protect(repository_id, &req.pattern)
        .await?;
    Ok((StatusCode::CREATED, Json(rule.into())))
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
            delete(unprotect_branch),
        )
}
