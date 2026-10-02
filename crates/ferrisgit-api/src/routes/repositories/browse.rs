//! Read-only views of a repository's git content: commits, tree, blob, README, contributors and languages.

use std::path::{Path as FsPath, PathBuf};

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::repository::Repository;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use ferrisgit_infrastructure::gix_reader::{
    CommitInfo, ContributorInfo, GitReadError, GixRepositoryReader, LanguageStat, TreeEntryInfo,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::authz::{require_role, require_role_by_id};
use crate::error::ApiError;
use crate::state::AppState;

/// Runs a git read on a blocking thread, gix does synchronous disk I/O.
async fn read_git<T: Send + 'static>(
    state: &AppState,
    disk_path: &str,
    read: impl FnOnce(&GixRepositoryReader, &FsPath) -> Result<T, GitReadError> + Send + 'static,
) -> Result<T, DomainError> {
    let full_path = PathBuf::from(&state.config.storage_root).join(disk_path);
    let git_reader = state.git_reader.clone();
    tokio::task::spawn_blocking(move || read(&git_reader, &full_path))
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?
        .map_err(|e| DomainError::Infrastructure(e.to_string()))
}

#[derive(Deserialize)]
pub(crate) struct CommitsQuery {
    #[serde(rename = "ref")]
    pub(crate) r#ref: Option<String>,
}

const COMMITS_PAGE_SIZE: usize = 20;

/// A `ref` that doesn't resolve (unknown, or a blob/tree) is a 404. `HEAD` of an empty repository is an empty list.
pub(crate) async fn commits_response(
    state: &AppState,
    disk_path: &str,
    r#ref: Option<String>,
) -> Result<Json<Vec<CommitInfo>>, ApiError> {
    // HEAD is the default, and asking for it by name must still give an empty list, not a 404, on an empty repository.
    let explicit_ref = r#ref.as_deref().is_some_and(|r| r != "HEAD");
    let revision = r#ref.unwrap_or_else(|| "HEAD".to_string());
    let commits = read_git(state, disk_path, move |git, path| {
        if explicit_ref && git.resolve_commit(path, &revision)?.is_none() {
            return Ok(None);
        }
        git.list_commits_at(path, &revision, COMMITS_PAGE_SIZE)
            .map(Some)
    })
    .await?;
    let commits =
        commits.ok_or_else(|| DomainError::NotFound("this ref does not exist".to_string()))?;
    Ok(Json(commits))
}

async fn commits(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((owner, name)): Path<(String, String)>,
    Query(query): Query<CommitsQuery>,
) -> Result<Json<Vec<CommitInfo>>, ApiError> {
    let repo = require_role(&state, user_id, &owner, &name, CollaboratorRole::Reader).await?;
    commits_response(&state, &repo.disk_path, query.r#ref).await
}

async fn commits_by_id(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<CommitsQuery>,
) -> Result<Json<Vec<CommitInfo>>, ApiError> {
    let repo = require_role_by_id(&state, user_id, id, CollaboratorRole::Reader).await?;
    commits_response(&state, &repo.disk_path, query.r#ref).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TreeQuery {
    last_commit: Option<bool>,
}

pub(crate) async fn tree_response(
    state: &AppState,
    repo: &Repository,
    r#ref: &str,
    path: &str,
    query: TreeQuery,
) -> Result<Json<Vec<TreeEntryInfo>>, ApiError> {
    let revision = r#ref.to_string();
    let tree_path = path.to_string();
    let with_last_commits = query.last_commit.unwrap_or(true);
    let entries = read_git(state, &repo.disk_path, move |git, full_path| {
        let Some(commit_id) = git.resolve_revision(full_path, &revision)? else {
            return Ok(None);
        };
        if with_last_commits {
            git.list_tree_at_revision(full_path, &commit_id.to_string(), &tree_path)
        } else {
            git.list_tree_names_at_revision(full_path, &commit_id.to_string(), &tree_path)
        }
    })
    .await?;
    let entries = entries
        .ok_or_else(|| DomainError::NotFound("this ref or path does not exist".to_string()))?;
    Ok(Json(entries))
}

async fn tree_by_id_root(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((id, r#ref)): Path<(Uuid, String)>,
    Query(query): Query<TreeQuery>,
) -> Result<Json<Vec<TreeEntryInfo>>, ApiError> {
    let repo = require_role_by_id(&state, user_id, id, CollaboratorRole::Reader).await?;
    tree_response(&state, &repo, &r#ref, "", query).await
}

async fn tree_by_id_path(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((id, r#ref, path)): Path<(Uuid, String, String)>,
    Query(query): Query<TreeQuery>,
) -> Result<Json<Vec<TreeEntryInfo>>, ApiError> {
    let repo = require_role_by_id(&state, user_id, id, CollaboratorRole::Reader).await?;
    tree_response(&state, &repo, &r#ref, &path, query).await
}

const MAX_PREVIEWABLE_FILE_SIZE: usize = 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BlobResponse {
    sha: String,
    size: usize,
    is_binary: bool,
    content: Option<String>,
}

type BlobAtRevision = (String, u64, Option<Vec<u8>>);

async fn blob_by_id(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((id, r#ref, path)): Path<(Uuid, String, String)>,
) -> Result<Json<BlobResponse>, ApiError> {
    let repo = require_role_by_id(&state, user_id, id, CollaboratorRole::Reader).await?;
    blob_response(&state, &repo, r#ref, path).await
}

pub(crate) async fn blob_response(
    state: &AppState,
    repo: &Repository,
    r#ref: String,
    path: String,
) -> Result<Json<BlobResponse>, ApiError> {
    let (sha, size, bytes) = read_git(
        state,
        &repo.disk_path,
        move |git, full_path| -> Result<Option<BlobAtRevision>, GitReadError> {
            let Some(commit_id) = git.resolve_revision(full_path, &r#ref)? else {
                return Ok(None);
            };
            let sha = commit_id.to_string();
            let Some(size) = git.blob_size_at_revision(full_path, &sha, &path)? else {
                return Ok(None);
            };
            if size as usize > MAX_PREVIEWABLE_FILE_SIZE {
                return Ok(Some((sha, size, None)));
            }
            let bytes = git.read_file_at_revision(full_path, &sha, &path)?;
            Ok(bytes.map(|b| (sha, size, Some(b))))
        },
    )
    .await?
    .ok_or_else(|| DomainError::NotFound("this ref or path does not exist".to_string()))?;

    let is_binary = bytes
        .as_deref()
        .is_some_and(|b| b.iter().take(8000).any(|&byte| byte == 0));
    let content = if is_binary {
        None
    } else {
        bytes.map(|b| String::from_utf8_lossy(&b).into_owned())
    };

    Ok(Json(BlobResponse {
        sha,
        size: size as usize,
        is_binary,
        content,
    }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReadmeResponse {
    content: Option<String>,
}

async fn readme_by_id(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((id, r#ref)): Path<(Uuid, String)>,
) -> Result<Json<ReadmeResponse>, ApiError> {
    let repo = require_role_by_id(&state, user_id, id, CollaboratorRole::Reader).await?;
    readme_response(&state, &repo, r#ref).await
}

pub(crate) async fn readme_response(
    state: &AppState,
    repo: &Repository,
    r#ref: String,
) -> Result<Json<ReadmeResponse>, ApiError> {
    let content = read_git(state, &repo.disk_path, move |git, full_path| {
        let Some(commit_id) = git.resolve_revision(full_path, &r#ref)? else {
            return Ok(None);
        };
        let sha = commit_id.to_string();
        let Some(entries) = git.list_tree_at_revision_with_window(full_path, &sha, "", 0)? else {
            return Ok(None);
        };
        let Some(readme_entry) = entries
            .iter()
            .find(|e| !e.is_dir && e.name.eq_ignore_ascii_case("README.md"))
        else {
            return Ok(None);
        };
        git.read_file_at_revision(full_path, &sha, &readme_entry.name)
    })
    .await?;

    let content = content.map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
    Ok(Json(ReadmeResponse { content }))
}

async fn contributors_by_id(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((id, r#ref)): Path<(Uuid, String)>,
) -> Result<Json<Vec<ContributorInfo>>, ApiError> {
    let repo = require_role_by_id(&state, user_id, id, CollaboratorRole::Reader).await?;
    contributors_response(&state, &repo, r#ref).await
}

pub(crate) async fn contributors_response(
    state: &AppState,
    repo: &Repository,
    r#ref: String,
) -> Result<Json<Vec<ContributorInfo>>, ApiError> {
    let contributors = read_git(state, &repo.disk_path, move |git, full_path| {
        let Some(commit_id) = git.resolve_revision(full_path, &r#ref)? else {
            return Ok(Vec::new());
        };
        Ok(git
            .list_contributors(full_path, &commit_id.to_string())?
            .unwrap_or_default())
    })
    .await?;
    Ok(Json(contributors))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LanguagesResponse {
    languages: Vec<LanguageStat>,
}

async fn languages_by_id(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((id, r#ref)): Path<(Uuid, String)>,
) -> Result<Json<LanguagesResponse>, ApiError> {
    let repo = require_role_by_id(&state, user_id, id, CollaboratorRole::Reader).await?;
    languages_response(&state, &repo, r#ref).await
}

pub(crate) async fn languages_response(
    state: &AppState,
    repo: &Repository,
    r#ref: String,
) -> Result<Json<LanguagesResponse>, ApiError> {
    let languages = read_git(state, &repo.disk_path, move |git, full_path| {
        let Some(commit_id) = git.resolve_revision(full_path, &r#ref)? else {
            return Ok(Vec::new());
        };
        Ok(git
            .compute_language_stats(full_path, &commit_id.to_string())?
            .unwrap_or_default())
    })
    .await?;
    Ok(Json(LanguagesResponse { languages }))
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/repositories/{owner}/{name}/commits", get(commits))
        .route("/repositories/by-id/{id}/tree/{ref}", get(tree_by_id_root))
        .route(
            "/repositories/by-id/{id}/tree/{ref}/{*path}",
            get(tree_by_id_path),
        )
        .route(
            "/repositories/by-id/{id}/blob/{ref}/{*path}",
            get(blob_by_id),
        )
        .route("/repositories/by-id/{id}/readme/{ref}", get(readme_by_id))
        .route(
            "/repositories/by-id/{id}/contributors/{ref}",
            get(contributors_by_id),
        )
        .route(
            "/repositories/by-id/{id}/languages/{ref}",
            get(languages_by_id),
        )
        .route("/repositories/by-id/{id}/commits", get(commits_by_id))
}
