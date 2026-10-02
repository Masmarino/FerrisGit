use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::use_cases::create_release::CreateReleaseUseCase;
use ferrisgit_application::use_cases::delete_release::DeleteReleaseUseCase;
use ferrisgit_application::use_cases::delete_release_asset::DeleteReleaseAssetUseCase;
use ferrisgit_application::use_cases::delete_tag::DeleteTagUseCase;
use ferrisgit_application::use_cases::upload_release_asset::UploadReleaseAssetUseCase;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::release::{Release, ReleaseAsset, ReleaseUpdate};
use ferrisgit_domain::repository::Repository;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::authz::require_role_by_id;
use crate::error::ApiError;
use crate::routes::user_ref::{UserRef, load_user_refs};
use crate::state::AppState;

const NOTES_EXCERPT_CHARS: usize = 240;

fn notes_excerpt(notes: &str) -> String {
    notes.trim().chars().take(NOTES_EXCERPT_CHARS).collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReleaseSummaryResponse {
    id: Uuid,
    tag_name: String,
    title: String,
    draft: bool,
    prerelease: bool,
    /// `null` once the author's account is deleted, and so is `author`.
    author_id: Option<Uuid>,
    author: Option<UserRef>,
    notes_excerpt: String,
    asset_count: i64,
    created_at: DateTime<Utc>,
    published_at: Option<DateTime<Utc>>,
}

impl ReleaseSummaryResponse {
    /// Every handler that returns release summaries builds them here, so the fields stay the same.
    async fn build_many(
        state: &AppState,
        releases: Vec<Release>,
    ) -> Result<Vec<Self>, DomainError> {
        let ids: Vec<Uuid> = releases.iter().map(|r| r.id).collect();
        let asset_counts = state.releases.asset_counts(&ids).await?;
        let users = load_user_refs(state, releases.iter().filter_map(|r| r.author_id)).await;
        Ok(releases
            .into_iter()
            .map(|r| ReleaseSummaryResponse {
                author: r.author_id.and_then(|id| users.get(&id).cloned()),
                notes_excerpt: notes_excerpt(&r.notes),
                asset_count: asset_counts.get(&r.id).copied().unwrap_or(0),
                id: r.id,
                tag_name: r.tag_name,
                title: r.title,
                draft: r.draft,
                prerelease: r.prerelease,
                author_id: r.author_id,
                created_at: r.created_at,
                published_at: r.published_at,
            })
            .collect())
    }

    async fn build(state: &AppState, release: Release) -> Result<Self, DomainError> {
        Ok(Self::build_many(state, vec![release]).await?.remove(0))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReleaseAssetResponse {
    id: Uuid,
    filename: String,
    content_type: String,
    size_bytes: i64,
    /// `null` once the uploader's account is deleted, and so is `uploader`.
    uploaded_by: Option<Uuid>,
    uploader: Option<UserRef>,
    created_at: DateTime<Utc>,
}

impl ReleaseAssetResponse {
    fn from_asset(a: ReleaseAsset, uploader: Option<UserRef>) -> Self {
        Self {
            id: a.id,
            filename: a.filename,
            content_type: a.content_type,
            size_bytes: a.size_bytes,
            uploaded_by: a.uploaded_by,
            uploader,
            created_at: a.created_at,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReleaseDetailResponse {
    id: Uuid,
    tag_name: String,
    title: String,
    notes: String,
    draft: bool,
    prerelease: bool,
    /// `null` once the author's account is deleted, and so is `author`.
    author_id: Option<Uuid>,
    author: Option<UserRef>,
    created_at: DateTime<Utc>,
    published_at: Option<DateTime<Utc>>,
    /// Read live from git. `None` if the tag was deleted after the release.
    target_commit_sha: Option<String>,
    assets: Vec<ReleaseAssetResponse>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TagResponse {
    name: String,
    target_sha: String,
}

/// Feeds the existing-tags dropdown of the "New release" form. Same Reader gate as the branches route.
async fn list_tags(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
) -> Result<Json<Vec<TagResponse>>, ApiError> {
    let repo = require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Reader).await?;
    tags_response(&state, &repo).await
}

pub(crate) async fn tags_response(
    state: &AppState,
    repo: &Repository,
) -> Result<Json<Vec<TagResponse>>, ApiError> {
    let tags = state.tags.list_tags(&repo.disk_path).await?;
    Ok(Json(
        tags.into_iter()
            .map(|t| TagResponse {
                name: t.name,
                target_sha: t.target_sha,
            })
            .collect(),
    ))
}

/// For a tag left without a release, say a draft created against the wrong commit and then deleted
/// (`CreateReleaseUseCase` won't recreate the tag elsewhere). `DeleteTagUseCase` refuses while a release uses it.
async fn delete_tag(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, tag_name)): Path<(Uuid, String)>,
) -> Result<StatusCode, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    let use_case = DeleteTagUseCase::new(
        state.repositories.clone(),
        state.releases.clone(),
        state.tag_creator.clone(),
    );
    use_case.execute(repository_id, tag_name).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
) -> Result<Json<Vec<ReleaseSummaryResponse>>, ApiError> {
    let repo = require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Reader).await?;
    // Separate from the Reader check above: this only decides whether drafts are included, hence `.is_ok()`
    // instead of rejecting a plain Reader.
    let include_drafts =
        require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer)
            .await
            .is_ok();
    releases_response(&state, &repo, include_drafts).await
}

pub(crate) async fn releases_response(
    state: &AppState,
    repo: &Repository,
    include_drafts: bool,
) -> Result<Json<Vec<ReleaseSummaryResponse>>, ApiError> {
    let releases = state
        .releases
        .list_for_repository(repo.id, include_drafts)
        .await?;
    Ok(Json(
        ReleaseSummaryResponse::build_many(state, releases).await?,
    ))
}

/// A missing tag and a draft the caller can't see have to answer identically.
pub(crate) fn release_not_found() -> ApiError {
    DomainError::NotFound("release".to_string()).into()
}

pub(crate) async fn find_release(
    state: &AppState,
    repo: &Repository,
    tag_name: &str,
) -> Result<Release, ApiError> {
    state
        .releases
        .find_by_tag_name(repo.id, tag_name)
        .await?
        .ok_or_else(release_not_found)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateReleaseRequest {
    tag_name: String,
    target_commit_sha: String,
    title: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// Rejects names that can't be one `{tag_name}` path segment or that `git check-ref-format` refuses, so a bad name
/// never reaches `git update-ref`.
fn validate_tag_name(tag_name: &str) -> Result<(), DomainError> {
    if tag_name.is_empty() {
        return Err(DomainError::Validation(
            "tag name must not be empty".to_string(),
        ));
    }
    if tag_name.contains('/') {
        return Err(DomainError::Validation(
            "tag name must not contain '/'".to_string(),
        ));
    }
    if tag_name.starts_with('-') {
        return Err(DomainError::Validation(
            "tag name must not start with '-'".to_string(),
        ));
    }
    if tag_name.contains("..") {
        return Err(DomainError::Validation(
            "tag name must not contain '..'".to_string(),
        ));
    }
    if tag_name.chars().any(|c| c.is_control()) {
        return Err(DomainError::Validation(
            "tag name must not contain control characters".to_string(),
        ));
    }
    if tag_name.ends_with(".lock") {
        return Err(DomainError::Validation(
            "tag name must not end with '.lock'".to_string(),
        ));
    }
    if tag_name.contains("@{") {
        return Err(DomainError::Validation(
            "tag name must not contain '@{'".to_string(),
        ));
    }
    Ok(())
}

async fn create(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
    Json(req): Json<CreateReleaseRequest>,
) -> Result<Json<ReleaseSummaryResponse>, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    validate_tag_name(&req.tag_name)?;
    let use_case = CreateReleaseUseCase::new(
        state.releases.clone(),
        state.repositories.clone(),
        state.tags.clone(),
        state.tag_creator.clone(),
    );
    let release = use_case
        .execute(
            repository_id,
            req.tag_name,
            req.target_commit_sha,
            req.title,
            req.notes,
            req.draft,
            req.prerelease,
            user_id,
        )
        .await?;
    Ok(Json(ReleaseSummaryResponse::build(&state, release).await?))
}

pub(crate) async fn detail_response(
    state: &AppState,
    repo_disk_path: &str,
    release: Release,
) -> Result<ReleaseDetailResponse, ApiError> {
    let tags = state.tags.list_tags(repo_disk_path).await?;
    let target_commit_sha = tags
        .into_iter()
        .find(|t| t.name == release.tag_name)
        .map(|t| t.target_sha);
    let assets = state.releases.list_assets(release.id).await?;
    let users = load_user_refs(
        state,
        release
            .author_id
            .into_iter()
            .chain(assets.iter().filter_map(|a| a.uploaded_by)),
    )
    .await;
    Ok(ReleaseDetailResponse {
        id: release.id,
        tag_name: release.tag_name,
        title: release.title,
        notes: release.notes,
        draft: release.draft,
        prerelease: release.prerelease,
        author_id: release.author_id,
        author: release.author_id.and_then(|id| users.get(&id).cloned()),
        created_at: release.created_at,
        published_at: release.published_at,
        target_commit_sha,
        assets: assets
            .into_iter()
            .map(|a| {
                let uploader = a.uploaded_by.and_then(|id| users.get(&id).cloned());
                ReleaseAssetResponse::from_asset(a, uploader)
            })
            .collect(),
    })
}

async fn detail(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, tag_name)): Path<(Uuid, String)>,
) -> Result<Json<ReleaseDetailResponse>, ApiError> {
    let repo = require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Reader).await?;
    let release = find_release(&state, &repo, &tag_name).await?;
    // To a non-Maintainer a draft has to look exactly like a missing release.
    if release.draft
        && require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer)
            .await
            .is_err()
    {
        return Err(release_not_found());
    }
    Ok(Json(
        detail_response(&state, &repo.disk_path, release).await?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateReleaseRequest {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    prerelease: Option<bool>,
    /// Only `Some(false)` on a draft does anything: it publishes. There's no un-publish.
    #[serde(default)]
    draft: Option<bool>,
}

async fn update(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, tag_name)): Path<(Uuid, String)>,
    Json(req): Json<UpdateReleaseRequest>,
) -> Result<Json<ReleaseDetailResponse>, ApiError> {
    let repo =
        require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    let release = state
        .releases
        .find_by_tag_name(repository_id, &tag_name)
        .await?
        .ok_or_else(|| DomainError::NotFound("release".to_string()))?;

    let mut current = release.clone();
    if req.title.is_some() || req.notes.is_some() || req.prerelease.is_some() {
        current = state
            .releases
            .update(
                release.id,
                repository_id,
                ReleaseUpdate {
                    title: req.title,
                    notes: req.notes,
                    prerelease: req.prerelease,
                },
            )
            .await?;
    }
    if req.draft == Some(false) && current.draft {
        current = state.releases.publish(release.id, repository_id).await?;
    }

    Ok(Json(
        detail_response(&state, &repo.disk_path, current).await?,
    ))
}

async fn delete(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, tag_name)): Path<(Uuid, String)>,
) -> Result<StatusCode, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    let release = state
        .releases
        .find_by_tag_name(repository_id, &tag_name)
        .await?
        .ok_or_else(|| DomainError::NotFound("release".to_string()))?;
    let use_case =
        DeleteReleaseUseCase::new(state.releases.clone(), state.release_asset_storage.clone());
    use_case.execute(release.id, repository_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn upload_asset(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, tag_name)): Path<(Uuid, String)>,
    mut multipart: Multipart,
) -> Result<Json<ReleaseAssetResponse>, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    let release = state
        .releases
        .find_by_tag_name(repository_id, &tag_name)
        .await?
        .ok_or_else(|| DomainError::NotFound("release".to_string()))?;

    let mut filename = None;
    let mut content_type = None;
    let mut data = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| DomainError::Validation(e.to_string()))?
    {
        if field.name() == Some("file") {
            filename = field.file_name().map(|s| s.to_string());
            content_type = field.content_type().map(|s| s.to_string());
            data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| DomainError::Validation(e.to_string()))?,
            );
        }
    }
    let filename = filename.ok_or_else(|| {
        DomainError::Validation(
            "multipart upload is missing a 'file' field with a filename".to_string(),
        )
    })?;
    let data = data.ok_or_else(|| {
        DomainError::Validation("multipart upload is missing a 'file' field".to_string())
    })?;
    let content_type = content_type.unwrap_or_else(|| "application/octet-stream".to_string());

    let use_case =
        UploadReleaseAssetUseCase::new(state.releases.clone(), state.release_asset_storage.clone());
    let asset = use_case
        .execute(
            repository_id,
            release.id,
            filename,
            content_type,
            data,
            user_id,
        )
        .await?;
    let uploader = load_user_refs(&state, asset.uploaded_by)
        .await
        .into_values()
        .next();
    Ok(Json(ReleaseAssetResponse::from_asset(asset, uploader)))
}

async fn download_asset(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, tag_name, asset_id)): Path<(Uuid, String, Uuid)>,
) -> Result<Response, ApiError> {
    let repo = require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Reader).await?;
    let release = find_release(&state, &repo, &tag_name).await?;
    if release.draft
        && require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer)
            .await
            .is_err()
    {
        return Err(release_not_found());
    }
    asset_download_response(&state, &release, asset_id).await
}

/// `release` has to be one the caller may already see.
pub(crate) async fn asset_download_response(
    state: &AppState,
    release: &Release,
    asset_id: Uuid,
) -> Result<Response, ApiError> {
    let asset = state
        .releases
        .find_asset(asset_id, release.id)
        .await?
        .ok_or_else(|| DomainError::NotFound("release asset".to_string()))?;

    let path = state.release_asset_storage.absolute_path(&asset.disk_path);
    // A file that vanished from disk is a 404 for this asset, not a 500.
    let file = tokio::fs::File::open(&path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            DomainError::NotFound("release asset".to_string())
        } else {
            DomainError::Infrastructure(e.to_string())
        }
    })?;
    let body = Body::from_stream(tokio_util::io::ReaderStream::new(file));

    // Escape quotes in the client-supplied filename so it can't smuggle in extra Content-Disposition parameters.
    let escaped_filename = asset.filename.replace('"', "\\\"");
    let headers = [
        (header::CONTENT_TYPE, asset.content_type.clone()),
        (
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{escaped_filename}\""),
        ),
        // Keeps a browser from sniffing the echoed content type into something nastier, like HTML.
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
    ];
    Ok((headers, body).into_response())
}

async fn delete_asset(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, tag_name, asset_id)): Path<(Uuid, String, Uuid)>,
) -> Result<StatusCode, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    let release = state
        .releases
        .find_by_tag_name(repository_id, &tag_name)
        .await?
        .ok_or_else(|| DomainError::NotFound("release".to_string()))?;
    let use_case =
        DeleteReleaseAssetUseCase::new(state.releases.clone(), state.release_asset_storage.clone());
    use_case.execute(asset_id, release.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    let asset_upload = Router::new()
        .route(
            "/repositories/{repository_id}/releases/{tag_name}/assets",
            axum::routing::post(upload_asset),
        )
        .layer(axum::extract::DefaultBodyLimit::max(100 * 1024 * 1024));

    Router::new()
        .route("/repositories/{repository_id}/tags", get(list_tags))
        .route(
            "/repositories/{repository_id}/tags/{tag_name}",
            axum::routing::delete(delete_tag),
        )
        .route(
            "/repositories/{repository_id}/releases",
            get(list).post(create),
        )
        .route(
            "/repositories/{repository_id}/releases/{tag_name}",
            get(detail).patch(update).delete(delete),
        )
        .route(
            "/repositories/{repository_id}/releases/{tag_name}/assets/{asset_id}",
            get(download_asset).delete(delete_asset),
        )
        .merge(asset_upload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_name_containing_a_slash_is_rejected() {
        let result = validate_tag_name("release/1.0");
        assert!(
            matches!(result, Err(DomainError::Validation(_))),
            "a slash breaks routing since {{tag_name}} is a single path segment"
        );
    }

    #[test]
    fn an_empty_tag_name_is_rejected() {
        assert!(matches!(
            validate_tag_name(""),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn tag_names_check_ref_format_would_reject_are_rejected() {
        assert!(matches!(
            validate_tag_name("-leading-dash"),
            Err(DomainError::Validation(_))
        ));
        assert!(matches!(
            validate_tag_name("has..dotdot"),
            Err(DomainError::Validation(_))
        ));
        assert!(matches!(
            validate_tag_name("has\x07control"),
            Err(DomainError::Validation(_))
        ));
        assert!(matches!(
            validate_tag_name("name.lock"),
            Err(DomainError::Validation(_))
        ));
        assert!(matches!(
            validate_tag_name("name@{oops}"),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn ordinary_tag_names_are_accepted() {
        assert!(validate_tag_name("v1.0.0").is_ok());
        assert!(validate_tag_name("release-2026.09").is_ok());
    }
}
