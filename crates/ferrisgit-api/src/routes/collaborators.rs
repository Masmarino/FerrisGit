use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::use_cases::add_collaborator::AddCollaboratorUseCase;
use ferrisgit_application::use_cases::remove_collaborator::RemoveCollaboratorUseCase;
use ferrisgit_application::use_cases::set_collaborator_role::SetCollaboratorRoleUseCase;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::authz::{require_explicit_role_by_id, require_role_by_id};
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CollaboratorResponse {
    user_id: Uuid,
    username: String,
    role: String,
    created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct AddCollaboratorRequest {
    username: String,
    role: String,
}

#[derive(Deserialize)]
struct SetCollaboratorRoleRequest {
    role: String,
}

async fn list(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
) -> Result<Json<Vec<CollaboratorResponse>>, ApiError> {
    // Not `require_role_by_id`: its public-repository bypass would show the collaborator list to any visitor.
    let repo =
        require_explicit_role_by_id(&state, user_id, repository_id, CollaboratorRole::Reader)
            .await?;
    let collaborators = state
        .repository_collaborators
        .list_for_repository(repo.id)
        .await?;
    Ok(Json(
        collaborators
            .into_iter()
            .map(|c| CollaboratorResponse {
                user_id: c.user_id,
                username: c.username,
                role: c.role.as_str().to_string(),
                created_at: c.created_at,
            })
            .collect(),
    ))
}

async fn add(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
    Json(req): Json<AddCollaboratorRequest>,
) -> Result<StatusCode, ApiError> {
    let repo =
        require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    let role = CollaboratorRole::parse(&req.role)?;
    let use_case = AddCollaboratorUseCase::new(
        state.repository_collaborators.clone(),
        state.users.clone(),
        state.repositories.clone(),
        state.notifications.clone(),
        state.webhooks.clone(),
        state.groups.clone(),
        state.group_membership.clone(),
    );
    use_case
        .execute(repo.id, user_id, &req.username, role)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_role(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, username)): Path<(Uuid, String)>,
    Json(req): Json<SetCollaboratorRoleRequest>,
) -> Result<StatusCode, ApiError> {
    let repo =
        require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    let role = CollaboratorRole::parse(&req.role)?;
    let use_case = SetCollaboratorRoleUseCase::new(
        state.repository_collaborators.clone(),
        state.users.clone(),
        state.repositories.clone(),
        state.notifications.clone(),
        state.webhooks.clone(),
        state.groups.clone(),
        state.group_membership.clone(),
    );
    use_case.execute(repo.id, user_id, &username, role).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, username)): Path<(Uuid, String)>,
) -> Result<StatusCode, ApiError> {
    let repo =
        require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    let target = state
        .users
        .find_by_username(&username)
        .await?
        .ok_or_else(|| DomainError::Validation("no such user".to_string()))?;
    let use_case = RemoveCollaboratorUseCase::new(
        state.repository_collaborators.clone(),
        state.repositories.clone(),
        state.users.clone(),
        state.notifications.clone(),
        state.webhooks.clone(),
        state.groups.clone(),
        state.group_membership.clone(),
    );
    use_case.execute(repo.id, user_id, target.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/repositories/{repository_id}/collaborators",
            get(list).post(add),
        )
        .route(
            "/repositories/{repository_id}/collaborators/{username}",
            axum::routing::patch(set_role).delete(remove),
        )
}
