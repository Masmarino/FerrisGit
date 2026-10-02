use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use ferrisgit_application::use_cases::search::SearchUseCase;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::error::ApiError;
use crate::routes::repositories::repository_path;
use crate::routes::summaries::{
    IssueSummary, MergeRequestSummary, RepositoryRefs, issue_summaries, merge_request_summaries,
};
use crate::routes::user_ref::require_user;
use crate::state::AppState;

#[derive(Deserialize)]
struct SearchQuery {
    #[serde(default)]
    q: String,
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
struct SearchUserResponse {
    id: Uuid,
    username: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchResponse {
    repositories: Vec<SearchRepositoryResponse>,
    issues: Vec<IssueSummary>,
    merge_requests: Vec<MergeRequestSummary>,
    users: Vec<SearchUserResponse>,
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
        let owner = require_user(&state, repo.owner_id).await?;
        let path = repository_path(&state, &repo, &owner.username).await?;
        repositories.push(SearchRepositoryResponse {
            id: repo.id,
            name: repo.name,
            description: repo.description,
            path,
            visibility: repo.visibility.as_str().to_string(),
        });
    }

    let mut repo_cache = RepositoryRefs::new();
    let issues = issue_summaries(&state, results.issues, &mut repo_cache).await?;
    let merge_requests =
        merge_request_summaries(&state, results.merge_requests, &mut repo_cache).await?;

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
