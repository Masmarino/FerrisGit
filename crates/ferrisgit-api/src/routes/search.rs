use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::use_cases::search::SearchUseCase;
use ferrisgit_domain::error::DomainError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::error::ApiError;
use crate::routes::repositories::repository_path;
use crate::state::AppState;

#[derive(Deserialize)]
struct SearchQuery {
    #[serde(default)]
    q: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SearchRepositoryRef {
    id: Uuid,
    name: String,
    path: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchRepositoryResponse {
    id: Uuid,
    name: String,
    description: String,
    path: Vec<String>,
    visibility: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchIssueResponse {
    id: Uuid,
    number: i32,
    title: String,
    status: String,
    kind: String,
    created_at: DateTime<Utc>,
    repository: SearchRepositoryRef,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchMergeRequestResponse {
    id: Uuid,
    title: String,
    status: String,
    source_branch: String,
    target_branch: String,
    created_at: DateTime<Utc>,
    repository: SearchRepositoryRef,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchUserResponse {
    id: Uuid,
    username: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchResponse {
    repositories: Vec<SearchRepositoryResponse>,
    issues: Vec<SearchIssueResponse>,
    merge_requests: Vec<SearchMergeRequestResponse>,
    users: Vec<SearchUserResponse>,
}

/// Cached by `repository_id`: results often share a repository and each resolution costs lookups.
pub(crate) async fn repository_ref(
    state: &AppState,
    repository_id: Uuid,
    cache: &mut HashMap<Uuid, SearchRepositoryRef>,
) -> Result<SearchRepositoryRef, ApiError> {
    if let Some(cached) = cache.get(&repository_id) {
        return Ok(cached.clone());
    }
    let repo = state
        .repositories
        .find_by_id(repository_id)
        .await?
        .ok_or_else(|| DomainError::NotFound("repository".to_string()))?;
    let owner = state
        .users
        .find_by_id(repo.owner_id)
        .await?
        .ok_or_else(|| DomainError::NotFound("user".to_string()))?;
    let path = repository_path(state, &repo, &owner.username).await?;
    let result = SearchRepositoryRef {
        id: repo.id,
        name: repo.name,
        path,
    };
    cache.insert(repository_id, result.clone());
    Ok(result)
}

async fn search(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchResponse>, ApiError> {
    let use_case = SearchUseCase::new(
        state.repositories.clone(),
        state.repository_collaborators.clone(),
        state.groups.clone(),
        state.issues.clone(),
        state.merge_requests.clone(),
        state.users.clone(),
    );
    let results = use_case.execute(user_id, &query.q).await?;

    let mut repositories = Vec::with_capacity(results.repositories.len());
    for repo in results.repositories {
        let owner = state
            .users
            .find_by_id(repo.owner_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("user".to_string()))?;
        let path = repository_path(&state, &repo, &owner.username).await?;
        repositories.push(SearchRepositoryResponse {
            id: repo.id,
            name: repo.name,
            description: repo.description,
            path,
            visibility: repo.visibility.as_str().to_string(),
        });
    }

    let mut repo_cache: HashMap<Uuid, SearchRepositoryRef> = HashMap::new();

    let mut issues = Vec::with_capacity(results.issues.len());
    for issue in results.issues {
        let repository = repository_ref(&state, issue.repository_id, &mut repo_cache).await?;
        issues.push(SearchIssueResponse {
            id: issue.id,
            number: issue.number,
            title: issue.title,
            status: issue.status.as_str().to_string(),
            kind: issue.kind.as_str().to_string(),
            created_at: issue.created_at,
            repository,
        });
    }

    let mut merge_requests = Vec::with_capacity(results.merge_requests.len());
    for mr in results.merge_requests {
        let repository = repository_ref(&state, mr.repository_id, &mut repo_cache).await?;
        merge_requests.push(SearchMergeRequestResponse {
            id: mr.id,
            title: mr.title,
            status: mr.status.as_str().to_string(),
            source_branch: mr.source_branch,
            target_branch: mr.target_branch,
            created_at: mr.created_at,
            repository,
        });
    }

    let users = results
        .users
        .into_iter()
        .map(|u| SearchUserResponse {
            id: u.id,
            username: u.username,
        })
        .collect();

    Ok(Json(SearchResponse {
        repositories,
        issues,
        merge_requests,
        users,
    }))
}

pub fn router() -> Router<AppState> {
    Router::new().route("/search", get(search))
}
