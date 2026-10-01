use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, patch};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::use_cases::create_label::CreateLabelUseCase;
use ferrisgit_application::use_cases::delete_label::DeleteLabelUseCase;
use ferrisgit_application::use_cases::update_label::UpdateLabelUseCase;
use ferrisgit_domain::label::Label;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::authz::{require_group_role_by_id, require_role_by_id};
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LabelResponse {
    id: Uuid,
    name: String,
    color: String,
    repository_id: Option<Uuid>,
    group_id: Option<Uuid>,
    created_at: DateTime<Utc>,
}

impl From<Label> for LabelResponse {
    fn from(l: Label) -> Self {
        LabelResponse {
            id: l.id,
            name: l.name,
            color: l.color,
            repository_id: l.repository_id,
            group_id: l.group_id,
            created_at: l.created_at,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateLabelRequest {
    name: String,
    color: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateLabelRequest {
    name: String,
    color: String,
}

async fn list_for_repository(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
) -> Result<Json<Vec<LabelResponse>>, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Reader).await?;
    let labels = state.labels.list_for_repository(repository_id).await?;
    Ok(Json(labels.into_iter().map(Into::into).collect()))
}

async fn create_for_repository(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
    Json(req): Json<CreateLabelRequest>,
) -> Result<Json<LabelResponse>, ApiError> {
    require_role_by_id(
        &state,
        user_id,
        repository_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    let use_case = CreateLabelUseCase::new(state.labels.clone());
    let label = use_case
        .execute(req.name, req.color, Some(repository_id), None)
        .await?;
    Ok(Json(label.into()))
}

async fn list_for_group(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(group_id): Path<Uuid>,
) -> Result<Json<Vec<LabelResponse>>, ApiError> {
    require_group_role_by_id(&state, user_id, group_id, CollaboratorRole::Reader).await?;
    let labels = state.labels.list_for_group(group_id).await?;
    Ok(Json(labels.into_iter().map(Into::into).collect()))
}

async fn create_for_group(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(group_id): Path<Uuid>,
    Json(req): Json<CreateLabelRequest>,
) -> Result<Json<LabelResponse>, ApiError> {
    require_group_role_by_id(&state, user_id, group_id, CollaboratorRole::Contributor).await?;
    let use_case = CreateLabelUseCase::new(state.labels.clone());
    let label = use_case
        .execute(req.name, req.color, None, Some(group_id))
        .await?;
    Ok(Json(label.into()))
}

async fn require_manage_access(
    state: &AppState,
    user_id: Uuid,
    label_id: Uuid,
) -> Result<Label, ApiError> {
    let label = state.labels.find_by_id(label_id).await?.ok_or(
        ferrisgit_domain::error::DomainError::NotFound("label".to_string()),
    )?;
    if let Some(repository_id) = label.repository_id {
        require_role_by_id(state, user_id, repository_id, CollaboratorRole::Contributor).await?;
    } else if let Some(group_id) = label.group_id {
        require_group_role_by_id(state, user_id, group_id, CollaboratorRole::Contributor).await?;
    } else {
        // Unreachable while the `labels_scope_xor` CHECK holds, but must never fall through with no permission check.
        return Err(ferrisgit_domain::error::DomainError::NotFound("label".to_string()).into());
    }
    Ok(label)
}

async fn update(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(label_id): Path<Uuid>,
    Json(req): Json<UpdateLabelRequest>,
) -> Result<Json<LabelResponse>, ApiError> {
    require_manage_access(&state, user_id, label_id).await?;
    let use_case = UpdateLabelUseCase::new(state.labels.clone());
    let updated = use_case.execute(label_id, req.name, req.color).await?;
    Ok(Json(updated.into()))
}

async fn delete(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(label_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    require_manage_access(&state, user_id, label_id).await?;
    let use_case = DeleteLabelUseCase::new(state.labels.clone());
    use_case.execute(label_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/repositories/{repository_id}/labels",
            get(list_for_repository).post(create_for_repository),
        )
        .route(
            "/groups/{group_id}/labels",
            get(list_for_group).post(create_for_group),
        )
        .route("/labels/{id}", patch(update).delete(delete))
}
