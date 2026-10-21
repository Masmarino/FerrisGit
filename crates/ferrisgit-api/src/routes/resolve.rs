use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use ferrisgit_application::use_cases::resolve_path::{ResolvePathUseCase, ResolvedPath};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::Group;
use ferrisgit_domain::repository::Repository;
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
    /// `path` is where the repository lives now: it differs from the one asked for when that was an old path, kept as a
    /// redirect after a rename or a transfer, and the interface then moves to it.
    #[serde(rename_all = "camelCase")]
    PersonalRepository {
        repository_id: Uuid,
        path: Vec<String>,
    },
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
        path: Vec<String>,
    },
}

impl ResolveResponse {
    /// `chain` is `None` for a personal repository.
    pub(crate) async fn repository(
        state: &AppState,
        repo: &Repository,
        chain: Option<&[Group]>,
    ) -> Result<Self, DomainError> {
        Ok(match chain {
            None => {
                let owner = state
                    .users
                    .find_by_id(repo.owner_id)
                    .await?
                    .ok_or_else(|| DomainError::NotFound("path".to_string()))?;
                Self::PersonalRepository {
                    repository_id: repo.id,
                    path: vec![owner.username, repo.name.clone()],
                }
            }
            Some(chain) => {
                let mut path: Vec<String> = chain.iter().map(|g| g.name.clone()).collect();
                path.push(repo.name.clone());
                Self::GroupRepository {
                    repository_id: repo.id,
                    chain: chain_response(chain),
                    path,
                }
            }
        })
    }
}

/// Any failure to find the path, an empty one included, is `NotFound("path")`.
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

/// A path the caller can't read looks like one that doesn't exist.
fn unreadable_path_is_missing(error: DomainError) -> DomainError {
    match error {
        DomainError::NotFound(_) | DomainError::Unauthorized(_) => {
            DomainError::NotFound("path".to_string())
        }
        other => other,
    }
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
                .map_err(unreadable_path_is_missing)?;
            ResolveResponse::repository(&state, &repo, None).await?
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
                .map_err(unreadable_path_is_missing)?;
            ResolveResponse::repository(&state, &repository, Some(&chain)).await?
        }
    }))
}

pub fn router() -> Router<AppState> {
    Router::new().route("/resolve/{*path}", get(resolve))
}
