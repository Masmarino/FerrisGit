use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::use_cases::add_group_member::AddGroupMemberUseCase;
use ferrisgit_application::use_cases::create_group::CreateGroupUseCase;
use ferrisgit_application::use_cases::delete_group::DeleteGroupUseCase;
use ferrisgit_application::use_cases::remove_group_member::RemoveGroupMemberUseCase;
use ferrisgit_application::use_cases::set_group_member_role::SetGroupMemberRoleUseCase;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::authz::{effective_role_in_group_chain, require_group_role_by_id};
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
struct CreateGroupRequest {
    name: String,
    #[serde(default)]
    description: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupResponse {
    id: Uuid,
    parent_group_id: Option<Uuid>,
    name: String,
    description: String,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WritableGroupResponse {
    id: Uuid,
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupMembershipResponse {
    id: Uuid,
    path: String,
    role: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupMemberResponse {
    user_id: Uuid,
    username: String,
    role: String,
    created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct AddGroupMemberRequest {
    username: String,
    role: String,
}

#[derive(Deserialize)]
struct SetGroupMemberRoleRequest {
    role: String,
}

async fn create_root(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateGroupRequest>,
) -> Result<Json<GroupResponse>, ApiError> {
    let use_case = CreateGroupUseCase::new(
        state.groups.clone(),
        state.group_membership.clone(),
        state.users.clone(),
    );
    let group = use_case
        .execute(user_id, None, req.name, req.description)
        .await?;
    Ok(Json(GroupResponse {
        id: group.id,
        parent_group_id: group.parent_group_id,
        name: group.name,
        description: group.description,
        created_at: group.created_at,
    }))
}

async fn create_subgroup(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(parent_id): Path<Uuid>,
    Json(req): Json<CreateGroupRequest>,
) -> Result<Json<GroupResponse>, ApiError> {
    let use_case = CreateGroupUseCase::new(
        state.groups.clone(),
        state.group_membership.clone(),
        state.users.clone(),
    );
    let group = use_case
        .execute(user_id, Some(parent_id), req.name, req.description)
        .await?;
    Ok(Json(GroupResponse {
        id: group.id,
        parent_group_id: group.parent_group_id,
        name: group.name,
        description: group.description,
        created_at: group.created_at,
    }))
}

async fn writable(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<WritableGroupResponse>>, ApiError> {
    let groups = state.groups.list_writable_groups(user_id).await?;
    Ok(Json(
        groups
            .into_iter()
            .map(|g| WritableGroupResponse {
                id: g.group.id,
                path: g.path,
            })
            .collect(),
    ))
}

/// Unlike `writable`, not restricted to groups the caller can create things under. One `ancestor_chain` call
/// per group builds its display path.
async fn member(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<GroupMembershipResponse>>, ApiError> {
    let ids = state.groups.list_member_group_ids(user_id).await?;
    let mut result = Vec::with_capacity(ids.len());
    for id in ids {
        let chain = state.groups.ancestor_chain(id).await?;
        let path = chain
            .iter()
            .map(|g| g.name.as_str())
            .collect::<Vec<_>>()
            .join("/");
        // Always `Some` in practice. The fallback only avoids a panic.
        let role = effective_role_in_group_chain(state.group_membership.as_ref(), &chain, user_id)
            .await?
            .map_or_else(|| "reader".to_string(), |r| r.as_str().to_string());
        result.push(GroupMembershipResponse { id, path, role });
    }
    Ok(Json(result))
}

async fn delete_group(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    require_group_role_by_id(&state, user_id, id, CollaboratorRole::Maintainer).await?;
    let use_case = DeleteGroupUseCase::new(state.groups.clone(), state.repositories.clone());
    use_case.execute(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_members(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(group_id): Path<Uuid>,
) -> Result<Json<Vec<GroupMemberResponse>>, ApiError> {
    let chain = state.groups.ancestor_chain(group_id).await?;
    if chain.is_empty() {
        return Err(DomainError::NotFound("group".to_string()).into());
    }
    let role =
        effective_role_in_group_chain(state.group_membership.as_ref(), &chain, user_id).await?;
    if role.is_none_or(|r| r < CollaboratorRole::Reader) {
        return Err(DomainError::NotFound("group".to_string()).into());
    }
    let members = state.group_membership.list_members(group_id).await?;
    Ok(Json(
        members
            .into_iter()
            .map(|m| GroupMemberResponse {
                user_id: m.user_id,
                username: m.username,
                role: m.role.as_str().to_string(),
                created_at: m.created_at,
            })
            .collect(),
    ))
}

async fn add_member(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(group_id): Path<Uuid>,
    Json(req): Json<AddGroupMemberRequest>,
) -> Result<(), ApiError> {
    let role = CollaboratorRole::parse(&req.role)?;
    let use_case = AddGroupMemberUseCase::new(
        state.groups.clone(),
        state.group_membership.clone(),
        state.users.clone(),
    );
    use_case
        .execute(group_id, user_id, &req.username, role)
        .await?;
    Ok(())
}

async fn set_member_role(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((group_id, username)): Path<(Uuid, String)>,
    Json(req): Json<SetGroupMemberRoleRequest>,
) -> Result<(), ApiError> {
    let role = CollaboratorRole::parse(&req.role)?;
    let use_case = SetGroupMemberRoleUseCase::new(
        state.groups.clone(),
        state.group_membership.clone(),
        state.users.clone(),
    );
    use_case.execute(group_id, user_id, &username, role).await?;
    Ok(())
}

async fn remove_member(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((group_id, username)): Path<(Uuid, String)>,
) -> Result<(), ApiError> {
    let use_case = RemoveGroupMemberUseCase::new(
        state.groups.clone(),
        state.group_membership.clone(),
        state.users.clone(),
    );
    use_case.execute(group_id, user_id, &username).await?;
    Ok(())
}

async fn list_children(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(group_id): Path<Uuid>,
) -> Result<Json<Vec<GroupResponse>>, ApiError> {
    let chain = state.groups.ancestor_chain(group_id).await?;
    if chain.is_empty() {
        return Err(DomainError::NotFound("group".to_string()).into());
    }
    let role =
        effective_role_in_group_chain(state.group_membership.as_ref(), &chain, user_id).await?;
    if role.is_none_or(|r| r < CollaboratorRole::Reader) {
        return Err(DomainError::NotFound("group".to_string()).into());
    }
    let children = state.groups.list_children(Some(group_id)).await?;
    Ok(Json(
        children
            .into_iter()
            .map(|g| GroupResponse {
                id: g.id,
                parent_group_id: g.parent_group_id,
                name: g.name,
                description: g.description,
                created_at: g.created_at,
            })
            .collect(),
    ))
}

async fn list_repositories(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(group_id): Path<Uuid>,
) -> Result<Json<Vec<crate::routes::repositories::RepositoryResponse>>, ApiError> {
    let chain = state.groups.ancestor_chain(group_id).await?;
    if chain.is_empty() {
        return Err(DomainError::NotFound("group".to_string()).into());
    }
    let role =
        effective_role_in_group_chain(state.group_membership.as_ref(), &chain, user_id).await?;
    if role.is_none_or(|r| r < CollaboratorRole::Reader) {
        return Err(DomainError::NotFound("group".to_string()).into());
    }
    // Every repository here shares this group's ancestor chain and the caller's role, both already resolved above.
    let group_path: Vec<String> = chain.iter().map(|g| g.name.clone()).collect();
    let role = role.map_or_else(|| "reader".to_string(), |r| r.as_str().to_string());
    let repos = state.repositories.list_for_group(group_id).await?;
    let mut result = Vec::with_capacity(repos.len());
    for repo in repos {
        let owner = state
            .users
            .find_by_id(repo.owner_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("user".to_string()))?
            .username;
        let mut path = group_path.clone();
        path.push(repo.name.clone());
        result.push(
            crate::routes::repositories::RepositoryResponse::for_group_listing(
                &repo,
                owner,
                role.clone(),
                path,
            ),
        );
    }
    Ok(Json(result))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/groups", post(create_root))
        .route("/groups/writable", get(writable))
        .route("/groups/member", get(member))
        .route("/groups/{id}", axum::routing::delete(delete_group))
        .route("/groups/{id}/subgroups", post(create_subgroup))
        .route("/groups/{id}/members", get(list_members).post(add_member))
        .route(
            "/groups/{id}/members/{username}",
            axum::routing::patch(set_member_role).delete(remove_member),
        )
        .route("/groups/{id}/children", get(list_children))
        .route("/groups/{id}/repositories", get(list_repositories))
}
