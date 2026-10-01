use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use ferrisgit_application::use_cases::resolve_path::{ResolvePathUseCase, ResolvedPath};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::Group;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use serde::Serialize;
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::authz::{effective_role_in_group_chain, require_role_by_id};
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GroupChainEntry {
    id: Uuid,
    name: String,
}

fn chain_response(chain: &[Group]) -> Vec<GroupChainEntry> {
    chain
        .iter()
        .map(|g| GroupChainEntry {
            id: g.id,
            name: g.name.clone(),
        })
        .collect()
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum ResolveResponse {
    #[serde(rename_all = "camelCase")]
    PersonalRepository { repository_id: Uuid },
    #[serde(rename_all = "camelCase")]
    Group {
        group_id: Uuid,
        chain: Vec<GroupChainEntry>,
        role: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    GroupRepository {
        repository_id: Uuid,
        chain: Vec<GroupChainEntry>,
    },
}

impl ResolveResponse {
    /// `chain` is the group chain of a group repository, `None` for a personal one.
    pub(crate) fn repository(repository_id: Uuid, chain: Option<&[Group]>) -> Self {
        match chain {
            None => Self::PersonalRepository { repository_id },
            Some(chain) => Self::GroupRepository {
                repository_id,
                chain: chain_response(chain),
            },
        }
    }
}

/// Every failure to find the path, including an empty one, is `NotFound("path")`.
pub(crate) async fn resolve_path(state: &AppState, path: &str) -> Result<ResolvedPath, ApiError> {
    let segments: Vec<String> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if segments.is_empty() {
        return Err(DomainError::NotFound("path".to_string()).into());
    }
    let use_case = ResolvePathUseCase::new(
        state.users.clone(),
        state.groups.clone(),
        state.repositories.clone(),
    );
    Ok(use_case.execute(&segments).await?)
}

async fn resolve(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<Json<ResolveResponse>, ApiError> {
    let resolved = resolve_path(&state, &path).await?;
    Ok(Json(match resolved {
        ResolvedPath::PersonalRepository(repo) => {
            require_role_by_id(&state, user_id, repo.id, CollaboratorRole::Reader)
                .await
                .map_err(|e| match e {
                    DomainError::NotFound(_) | DomainError::Unauthorized(_) => {
                        DomainError::NotFound("path".to_string())
                    }
                    other => other,
                })?;
            ResolveResponse::repository(repo.id, None)
        }
        ResolvedPath::Group { chain } => {
            let group_id = chain
                .last()
                .expect("resolved Group chains are never empty")
                .id;
            let role =
                effective_role_in_group_chain(state.group_membership.as_ref(), &chain, user_id)
                    .await?;
            if role.is_none_or(|r| r < CollaboratorRole::Reader) {
                return Err(DomainError::NotFound("path".to_string()).into());
            }
            ResolveResponse::Group {
                group_id,
                chain: chain_response(&chain),
                role: role.map(|r| r.as_str().to_string()),
            }
        }
        ResolvedPath::GroupRepository { chain, repository } => {
            require_role_by_id(&state, user_id, repository.id, CollaboratorRole::Reader)
                .await
                .map_err(|e| match e {
                    DomainError::NotFound(_) | DomainError::Unauthorized(_) => {
                        DomainError::NotFound("path".to_string())
                    }
                    other => other,
                })?;
            ResolveResponse::repository(repository.id, Some(&chain))
        }
    }))
}

pub fn router() -> Router<AppState> {
    Router::new().route("/resolve/{*path}", get(resolve))
}
