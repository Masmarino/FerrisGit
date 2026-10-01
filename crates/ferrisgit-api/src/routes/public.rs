//! Anonymous, read-only pages of public repositories. Every route here reads through `require_public_repository`, and
//! answers private, unknown and switched-off alike with the same 404.

use std::time::Duration;

use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderName, HeaderValue, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::use_cases::public_pages::{
    RawPublicCatalogSearch, RequirePublicRepositoryUseCase, SearchPublicRepositoriesUseCase,
};
use ferrisgit_application::use_cases::resolve_path::ResolvedPath;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::repository::Repository;
use ferrisgit_infrastructure::gix_reader::{CommitInfo, ContributorInfo, TreeEntryInfo};
use serde::{Deserialize, Serialize};
use tower_http::set_header::SetResponseHeaderLayer;
use uuid::Uuid;

use crate::error::ApiError;
use crate::routes::auth::{MaybeConnectInfo, client_ip};
use crate::routes::merge_requests::{BranchResponse, branches_response};
use crate::routes::releases::{
    ReleaseDetailResponse, ReleaseSummaryResponse, TagResponse, asset_download_response,
    detail_response, find_release, release_not_found, releases_response, tags_response,
};
use crate::routes::repositories::{
    BlobResponse, CommitsQuery, LanguagesResponse, ReadmeResponse, RepositoryResponse, TreeQuery,
    blob_response, commits_response, contributors_response, languages_response, readme_response,
    repository_summary, tree_response,
};
use crate::routes::resolve::{ResolveResponse, resolve_path};
use crate::state::AppState;

const ROBOTS_DISALLOW_ALL: &str = "User-agent: *\nDisallow: /\n";
const ROBOTS_PUBLIC_PAGES: &str =
    "User-agent: *\nDisallow: /api/\nDisallow: /account\nDisallow: /admin/\n";

/// The only place that decides whether an anonymous visitor may read a repository.
pub async fn require_public_repository(
    state: &AppState,
    repository_id: Uuid,
) -> Result<Repository, ApiError> {
    Ok(RequirePublicRepositoryUseCase::new(
        state.public_pages_settings.clone(),
        state.repositories.clone(),
    )
    .execute(repository_id)
    .await?)
}

/// A settings read that fails counts as "do not index": the safe side.
async fn indexing_allowed(state: &AppState) -> bool {
    state
        .public_pages_settings
        .get()
        .await
        .is_ok_and(|settings| settings.allows_indexing())
}

pub async fn robots_txt(State(state): State<AppState>) -> Response {
    let body = if indexing_allowed(&state).await {
        ROBOTS_PUBLIC_PAGES
    } else {
        ROBOTS_DISALLOW_ALL
    };
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], body).into_response()
}

/// Applied to every response of the application, SPA and API alike.
pub async fn robots_tag(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let (mut response, indexing_allowed) =
        tokio::join!(next.run(request), indexing_allowed(&state));
    if !indexing_allowed {
        response.headers_mut().insert(
            HeaderName::from_static("x-robots-tag"),
            HeaderValue::from_static("noindex, nofollow"),
        );
    }
    response
}

fn retry_after_seconds(wait: Duration) -> u64 {
    (wait.as_secs() + u64::from(wait.subsec_nanos() > 0)).max(1)
}

async fn throttle(
    State(state): State<AppState>,
    MaybeConnectInfo(connect_info): MaybeConnectInfo,
    request: Request,
    next: Next,
) -> Response {
    let ip = client_ip(&state, connect_info, request.headers());
    match state.public_rate_limiter.check_or_retry_after(ip) {
        Ok(()) => next.run(request).await,
        Err(wait) => {
            let mut response = ApiError(DomainError::RateLimited(
                "too many requests, try again later".to_string(),
            ))
            .into_response();
            response.headers_mut().insert(
                header::RETRY_AFTER,
                HeaderValue::from(retry_after_seconds(wait)),
            );
            response
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    q: Option<String>,
    sort: Option<String>,
    page: Option<String>,
    per_page: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PublicRepositoryItem {
    id: Uuid,
    name: String,
    path: Vec<String>,
    owner: String,
    description: String,
    stars: i64,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PublicRepositoryListResponse {
    items: Vec<PublicRepositoryItem>,
    total: i64,
    page: u32,
    per_page: u32,
}

async fn list(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<PublicRepositoryListResponse>, ApiError> {
    let result = SearchPublicRepositoriesUseCase::new(
        state.public_pages_settings.clone(),
        state.public_catalog.clone(),
    )
    .execute(RawPublicCatalogSearch {
        q: query.q,
        sort: query.sort,
        page: query.page,
        per_page: query.per_page,
    })
    .await?;
    Ok(Json(PublicRepositoryListResponse {
        items: result
            .page
            .items
            .into_iter()
            .map(|entry| PublicRepositoryItem {
                id: entry.id,
                name: entry.name,
                path: entry.path,
                owner: entry.owner,
                description: entry.description,
                stars: entry.stars,
                created_at: entry.created_at,
            })
            .collect(),
        total: result.page.total,
        page: result.page_number,
        per_page: result.per_page,
    }))
}

/// Groups are never public: only a public repository's path resolves.
async fn resolve(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<Json<ResolveResponse>, ApiError> {
    let not_found = || ApiError(DomainError::NotFound("path".to_string()));
    let (repository_id, chain) = match resolve_path(&state, &path).await? {
        ResolvedPath::PersonalRepository(repo) => (repo.id, None),
        ResolvedPath::GroupRepository { chain, repository } => (repository.id, Some(chain)),
        ResolvedPath::Group { .. } => return Err(not_found()),
    };
    match require_public_repository(&state, repository_id).await {
        Ok(_) => Ok(Json(ResolveResponse::repository(
            repository_id,
            chain.as_deref(),
        ))),
        Err(ApiError(DomainError::NotFound(_))) => Err(not_found()),
        Err(other) => Err(other),
    }
}

async fn summary(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<RepositoryResponse>, ApiError> {
    let repo = require_public_repository(&state, id).await?;
    Ok(Json(repository_summary(&state, repo, None).await?))
}

async fn tree_root(
    State(state): State<AppState>,
    Path((id, r#ref)): Path<(Uuid, String)>,
    Query(query): Query<TreeQuery>,
) -> Result<Json<Vec<TreeEntryInfo>>, ApiError> {
    let repo = require_public_repository(&state, id).await?;
    tree_response(&state, &repo, &r#ref, "", query).await
}

async fn tree_path(
    State(state): State<AppState>,
    Path((id, r#ref, path)): Path<(Uuid, String, String)>,
    Query(query): Query<TreeQuery>,
) -> Result<Json<Vec<TreeEntryInfo>>, ApiError> {
    let repo = require_public_repository(&state, id).await?;
    tree_response(&state, &repo, &r#ref, &path, query).await
}

async fn blob(
    State(state): State<AppState>,
    Path((id, r#ref, path)): Path<(Uuid, String, String)>,
) -> Result<Json<BlobResponse>, ApiError> {
    let repo = require_public_repository(&state, id).await?;
    blob_response(&state, &repo, r#ref, path).await
}

async fn readme(
    State(state): State<AppState>,
    Path((id, r#ref)): Path<(Uuid, String)>,
) -> Result<Json<ReadmeResponse>, ApiError> {
    let repo = require_public_repository(&state, id).await?;
    readme_response(&state, &repo, r#ref).await
}

async fn contributors(
    State(state): State<AppState>,
    Path((id, r#ref)): Path<(Uuid, String)>,
) -> Result<Json<Vec<ContributorInfo>>, ApiError> {
    let repo = require_public_repository(&state, id).await?;
    contributors_response(&state, &repo, r#ref).await
}

async fn languages(
    State(state): State<AppState>,
    Path((id, r#ref)): Path<(Uuid, String)>,
) -> Result<Json<LanguagesResponse>, ApiError> {
    let repo = require_public_repository(&state, id).await?;
    languages_response(&state, &repo, r#ref).await
}

async fn commits(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<CommitsQuery>,
) -> Result<Json<Vec<CommitInfo>>, ApiError> {
    let repo = require_public_repository(&state, id).await?;
    commits_response(&state, &repo.disk_path, query.r#ref).await
}

async fn branches(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<BranchResponse>>, ApiError> {
    let repo = require_public_repository(&state, id).await?;
    branches_response(&state, &repo).await
}

async fn tags(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<TagResponse>>, ApiError> {
    let repo = require_public_repository(&state, id).await?;
    tags_response(&state, &repo).await
}

async fn releases(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<ReleaseSummaryResponse>>, ApiError> {
    let repo = require_public_repository(&state, id).await?;
    releases_response(&state, &repo, false).await
}

async fn release_detail(
    State(state): State<AppState>,
    Path((id, tag_name)): Path<(Uuid, String)>,
) -> Result<Json<ReleaseDetailResponse>, ApiError> {
    let repo = require_public_repository(&state, id).await?;
    let release = find_release(&state, &repo, &tag_name).await?;
    if release.draft {
        return Err(release_not_found());
    }
    Ok(Json(
        detail_response(&state, &repo.disk_path, release).await?,
    ))
}

async fn release_asset(
    State(state): State<AppState>,
    Path((id, tag_name, asset_id)): Path<(Uuid, String, Uuid)>,
) -> Result<Response, ApiError> {
    let repo = require_public_repository(&state, id).await?;
    let release = find_release(&state, &repo, &tag_name).await?;
    if release.draft {
        return Err(release_not_found());
    }
    asset_download_response(&state, &release, asset_id).await
}

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/public/repositories", get(list))
        .route("/public/resolve/{*path}", get(resolve))
        .route("/public/repositories/by-id/{id}", get(summary))
        .route("/public/repositories/by-id/{id}/tree/{ref}", get(tree_root))
        .route(
            "/public/repositories/by-id/{id}/tree/{ref}/{*path}",
            get(tree_path),
        )
        .route(
            "/public/repositories/by-id/{id}/blob/{ref}/{*path}",
            get(blob),
        )
        .route("/public/repositories/by-id/{id}/readme/{ref}", get(readme))
        .route(
            "/public/repositories/by-id/{id}/contributors/{ref}",
            get(contributors),
        )
        .route(
            "/public/repositories/by-id/{id}/languages/{ref}",
            get(languages),
        )
        .route("/public/repositories/by-id/{id}/commits", get(commits))
        .route("/public/repositories/{id}/branches", get(branches))
        .route("/public/repositories/{id}/tags", get(tags))
        .route("/public/repositories/{id}/releases", get(releases))
        .route(
            "/public/repositories/{id}/releases/{tag_name}",
            get(release_detail),
        )
        .route(
            "/public/repositories/{id}/releases/{tag_name}/assets/{asset_id}",
            get(release_asset),
        )
        .layer(middleware::from_fn_with_state(state, throttle))
        // Outside the throttle, so a 429 is not cached either.
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-cache"),
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_after_rounds_up_to_whole_seconds_and_is_never_zero() {
        assert_eq!(retry_after_seconds(Duration::from_secs(60)), 60);
        assert_eq!(retry_after_seconds(Duration::from_millis(59_001)), 60);
        assert_eq!(retry_after_seconds(Duration::ZERO), 1);
    }
}
