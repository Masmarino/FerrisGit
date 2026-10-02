use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, patch};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::use_cases::create_milestone::CreateMilestoneUseCase;
use ferrisgit_application::use_cases::delete_milestone::DeleteMilestoneUseCase;
use ferrisgit_application::use_cases::update_milestone::UpdateMilestoneUseCase;
use ferrisgit_domain::milestone::{Milestone, MilestoneState};
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::authz::{require_group_role_by_id, require_role_by_id};
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MilestoneResponse {
    id: Uuid,
    title: String,
    description: String,
    due_date: Option<DateTime<Utc>>,
    state: String,
    repository_id: Option<Uuid>,
    group_id: Option<Uuid>,
    created_at: DateTime<Utc>,
}

impl From<Milestone> for MilestoneResponse {
    fn from(m: Milestone) -> Self {
        MilestoneResponse {
            id: m.id,
            title: m.title,
            description: m.description,
            due_date: m.due_date,
            state: m.state.as_str().to_string(),
            repository_id: m.repository_id,
            group_id: m.group_id,
            created_at: m.created_at,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateMilestoneRequest {
    title: String,
    description: String,
    due_date: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateMilestoneRequest {
    title: String,
    description: String,
    due_date: Option<DateTime<Utc>>,
    state: String,
}

async fn list_for_repository(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
) -> Result<Json<Vec<MilestoneResponse>>, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Reader).await?;
    let milestones = state.milestones.list_for_repository(repository_id).await?;
    Ok(Json(milestones.into_iter().map(Into::into).collect()))
}

async fn create_for_repository(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
    Json(req): Json<CreateMilestoneRequest>,
) -> Result<Json<MilestoneResponse>, ApiError> {
    require_role_by_id(
        &state,
        user_id,
        repository_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    let use_case = CreateMilestoneUseCase::new(state.milestones.clone());
    let milestone = use_case
        .execute(
            req.title,
            req.description,
            req.due_date,
            Some(repository_id),
            None,
        )
        .await?;
    Ok(Json(milestone.into()))
}

async fn list_for_group(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(group_id): Path<Uuid>,
) -> Result<Json<Vec<MilestoneResponse>>, ApiError> {
    require_group_role_by_id(&state, user_id, group_id, CollaboratorRole::Reader).await?;
    let milestones = state.milestones.list_for_group(group_id).await?;
    Ok(Json(milestones.into_iter().map(Into::into).collect()))
}

async fn create_for_group(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(group_id): Path<Uuid>,
    Json(req): Json<CreateMilestoneRequest>,
) -> Result<Json<MilestoneResponse>, ApiError> {
    require_group_role_by_id(&state, user_id, group_id, CollaboratorRole::Contributor).await?;
    let use_case = CreateMilestoneUseCase::new(state.milestones.clone());
    let milestone = use_case
        .execute(
            req.title,
            req.description,
            req.due_date,
            None,
            Some(group_id),
        )
        .await?;
    Ok(Json(milestone.into()))
}

async fn require_manage_access(
    state: &AppState,
    user_id: Uuid,
    milestone_id: Uuid,
) -> Result<Milestone, ApiError> {
    let milestone = state.milestones.find_by_id(milestone_id).await?.ok_or(
        ferrisgit_domain::error::DomainError::NotFound("milestone".to_string()),
    )?;
    if let Some(repository_id) = milestone.repository_id {
        require_role_by_id(state, user_id, repository_id, CollaboratorRole::Contributor).await?;
    } else if let Some(group_id) = milestone.group_id {
        require_group_role_by_id(state, user_id, group_id, CollaboratorRole::Contributor).await?;
    } else {
        // Can't happen while the milestones_scope_xor CHECK holds, but never fall through without a permission check.
        return Err(ferrisgit_domain::error::DomainError::NotFound("milestone".to_string()).into());
    }
    Ok(milestone)
}

async fn update(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(milestone_id): Path<Uuid>,
    Json(req): Json<UpdateMilestoneRequest>,
) -> Result<Json<MilestoneResponse>, ApiError> {
    require_manage_access(&state, user_id, milestone_id).await?;
    let parsed_state = MilestoneState::parse(&req.state)?;
    let use_case = UpdateMilestoneUseCase::new(state.milestones.clone());
    let updated = use_case
        .execute(
            milestone_id,
            req.title,
            req.description,
            req.due_date,
            parsed_state,
        )
        .await?;
    Ok(Json(updated.into()))
}

async fn delete(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(milestone_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    require_manage_access(&state, user_id, milestone_id).await?;
    let use_case = DeleteMilestoneUseCase::new(state.milestones.clone());
    use_case.execute(milestone_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/repositories/{repository_id}/milestones",
            get(list_for_repository).post(create_for_repository),
        )
        .route(
            "/groups/{group_id}/milestones",
            get(list_for_group).post(create_for_group),
        )
        .route("/milestones/{id}", patch(update).delete(delete))
}
