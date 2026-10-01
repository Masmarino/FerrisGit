use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::use_cases::create_repository::CreateRepositoryUseCase;
use ferrisgit_application::use_cases::delete_repository::DeleteRepositoryUseCase;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::repository::{Repository, RepositoryVisibility};
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use ferrisgit_domain::settings::RepositorySettingsUpdate;
use ferrisgit_infrastructure::git_backend::GitBackend;
use ferrisgit_infrastructure::gix_reader::{
    CommitInfo, ContributorInfo, GitReadError, LanguageStat, TreeEntryInfo,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::authz::{effective_role_in_group_chain, require_role, require_role_by_id};
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateRepositoryRequest {
    name: String,
    visibility: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    ci_enabled: Option<bool>,
    #[serde(default)]
    required_approvals: Option<i32>,
    #[serde(default)]
    pipeline_file_path: Option<String>,
    #[serde(default)]
    group_path: Option<String>,
}

#[derive(Deserialize)]
struct ListRepositoriesQuery {
    #[serde(default)]
    starred: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RepositoryResponse {
    id: Uuid,
    name: String,
    description: String,
    owner: String,
    /// The caller's role; `null` for an anonymous visitor of a public repository.
    role: Option<String>,
    visibility: String,
    created_at: DateTime<Utc>,
    /// Group repositories cannot be reached through `{owner}/{name}` links, so listings must link through
    /// this field.
    path: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    star_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_starred: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_bytes: Option<u64>,
}

impl RepositoryResponse {
    /// `role` is the caller's resolved role in the group. Resolve it once per group, not per repository.
    pub(crate) fn for_group_listing(
        repo: &ferrisgit_domain::repository::Repository,
        owner: String,
        role: String,
        path: Vec<String>,
    ) -> Self {
        Self {
            id: repo.id,
            name: repo.name.clone(),
            description: repo.description.clone(),
            owner,
            role: Some(role),
            visibility: repo.visibility.as_str().to_string(),
            created_at: repo.created_at,
            path,
            star_count: None,
            is_starred: None,
            size_bytes: None,
        }
    }
}

/// Callers that already hold the group's `ancestor_chain` should build the path from it instead.
pub(crate) async fn repository_path(
    state: &AppState,
    repo: &ferrisgit_domain::repository::Repository,
    owner_username: &str,
) -> Result<Vec<String>, DomainError> {
    match repo.group_id {
        None => Ok(vec![owner_username.to_string(), repo.name.clone()]),
        Some(group_id) => {
            let chain = state.groups.ancestor_chain(group_id).await?;
            let mut path: Vec<String> = chain.into_iter().map(|g| g.name).collect();
            path.push(repo.name.clone());
            Ok(path)
        }
    }
}

async fn create(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateRepositoryRequest>,
) -> Result<Json<RepositoryResponse>, ApiError> {
    let visibility = RepositoryVisibility::parse(&req.visibility)?;
    let settings_update = RepositorySettingsUpdate {
        pipeline_file_path: req.pipeline_file_path,
        ci_enabled: req.ci_enabled,
        required_approvals: req.required_approvals,
    };

    let (group_id, group_path_names): (Option<Uuid>, Option<Vec<String>>) = match &req.group_path {
        None => (None, None),
        Some(path) => {
            let mut parent_id: Option<Uuid> = None;
            let mut group = None;
            for segment in path.split('/').filter(|s| !s.is_empty()) {
                let found = state
                    .groups
                    .find_child_by_name(parent_id, segment)
                    .await?
                    .ok_or_else(|| DomainError::NotFound("group".to_string()))?;
                parent_id = Some(found.id);
                group = Some(found);
            }
            let group = group.ok_or_else(|| {
                DomainError::Validation("groupPath must not be empty".to_string())
            })?;
            let chain = state.groups.ancestor_chain(group.id).await?;
            let role = crate::authz::effective_role_in_group_chain(
                state.group_membership.as_ref(),
                &chain,
                user_id,
            )
            .await?;
            if role.is_none_or(|r| r < CollaboratorRole::Maintainer) {
                return Err(DomainError::NotFound("group".to_string()).into());
            }
            let names: Vec<String> = chain.into_iter().map(|g| g.name).collect();
            (Some(group.id), Some(names))
        }
    };

    let use_case = CreateRepositoryUseCase::new(
        state.repositories.clone(),
        state.repository_settings.clone(),
    );
    let git_backend = state.git_backend.clone();

    let repo = use_case
        .execute(
            user_id,
            req.name,
            req.description,
            group_id,
            visibility,
            settings_update,
            |disk_path| {
                let git_backend = git_backend.clone();
                let disk_path = disk_path.to_string();
                // `block_in_place` panics on a current-thread runtime, which `#[sqlx::test]` uses.
                // Fall back to a direct call there.
                if tokio::runtime::Handle::current().runtime_flavor()
                    == tokio::runtime::RuntimeFlavor::MultiThread
                {
                    tokio::task::block_in_place(|| git_backend.init_bare_repo(&disk_path))
                } else {
                    git_backend.init_bare_repo(&disk_path)
                }
            },
        )
        .await?;

    let caller = state
        .users
        .find_by_id(user_id)
        .await?
        .ok_or_else(|| DomainError::NotFound("user".to_string()))?;
    let path = match group_path_names {
        Some(mut names) => {
            names.push(repo.name.clone());
            names
        }
        None => vec![caller.username.clone(), repo.name.clone()],
    };
    Ok(Json(RepositoryResponse {
        id: repo.id,
        name: repo.name,
        description: repo.description,
        owner: caller.username,
        role: Some("owner".to_string()),
        visibility: repo.visibility.as_str().to_string(),
        created_at: repo.created_at,
        path,
        star_count: None,
        is_starred: None,
        size_bytes: None,
    }))
}

async fn list(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Query(query): Query<ListRepositoriesQuery>,
) -> Result<Json<Vec<RepositoryResponse>>, ApiError> {
    let caller = state
        .users
        .find_by_id(user_id)
        .await?
        .ok_or_else(|| DomainError::NotFound("user".to_string()))?;

    let mut responses: Vec<RepositoryResponse> = state
        .repositories
        .list_for_owner(user_id)
        .await?
        .into_iter()
        .map(|r| {
            let path = vec![caller.username.clone(), r.name.clone()];
            RepositoryResponse {
                id: r.id,
                name: r.name,
                description: r.description.clone(),
                owner: caller.username.clone(),
                role: Some("owner".to_string()),
                visibility: r.visibility.as_str().to_string(),
                created_at: r.created_at,
                path,
                star_count: None,
                is_starred: None,
                size_bytes: None,
            }
        })
        .collect();

    for collaboration in state
        .repository_collaborators
        .list_collaborations_for_user(user_id)
        .await?
    {
        let repo = collaboration.repository;
        let path = repository_path(&state, &repo, &collaboration.owner_username).await?;
        responses.push(RepositoryResponse {
            id: repo.id,
            name: repo.name,
            description: repo.description.clone(),
            owner: collaboration.owner_username,
            role: Some(collaboration.role.as_str().to_string()),
            visibility: repo.visibility.as_str().to_string(),
            created_at: repo.created_at,
            path,
            star_count: None,
            is_starred: None,
            size_bytes: None,
        });
    }

    // Also list repositories reached only through group membership, or group-inherited access would list nothing.
    let mut owner_usernames: std::collections::HashMap<Uuid, String> =
        std::collections::HashMap::new();
    let member_group_ids = state.groups.list_member_group_ids(user_id).await?;
    for group_id in member_group_ids {
        let repos = state.repositories.list_for_group(group_id).await?;
        if repos.is_empty() {
            continue;
        }
        // Every repository of a group shares the ancestor chain and caller role, so resolve them once per group.
        let chain = state.groups.ancestor_chain(group_id).await?;
        let group_path: Vec<String> = chain.iter().map(|g| g.name.clone()).collect();
        // Always `Some` in practice. The fallback only avoids a panic.
        let role = effective_role_in_group_chain(state.group_membership.as_ref(), &chain, user_id)
            .await?
            .map_or_else(|| "reader".to_string(), |r| r.as_str().to_string());
        for repo in repos {
            let owner_username = if let Some(name) = owner_usernames.get(&repo.owner_id) {
                name.clone()
            } else {
                let owner = state
                    .users
                    .find_by_id(repo.owner_id)
                    .await?
                    .ok_or_else(|| DomainError::NotFound("user".to_string()))?;
                owner_usernames.insert(repo.owner_id, owner.username.clone());
                owner.username
            };
            let mut path = group_path.clone();
            path.push(repo.name.clone());
            responses.push(RepositoryResponse::for_group_listing(
                &repo,
                owner_username,
                role.clone(),
                path,
            ));
        }
    }

    // The same repository can be reached both as a collaborator and through a group. Dedup by id and keep the
    // first one, since the collaborator branch has the more precise role.
    let mut seen_ids = std::collections::HashSet::new();
    responses.retain(|r| seen_ids.insert(r.id));

    if query.starred {
        let starred_ids: std::collections::HashSet<Uuid> = state
            .repository_stars
            .list_starred_for_user(user_id)
            .await?
            .into_iter()
            .collect();
        responses.retain(|r| starred_ids.contains(&r.id));
    }

    Ok(Json(responses))
}

async fn get_one(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((owner, name)): Path<(String, String)>,
) -> Result<Json<RepositoryResponse>, ApiError> {
    let repo = require_role(&state, user_id, &owner, &name, CollaboratorRole::Reader).await?;
    let role = if repo.owner_id == user_id {
        "owner".to_string()
    } else {
        state
            .repository_collaborators
            .get_role(repo.id, user_id)
            .await?
            .map_or_else(|| "reader".to_string(), |r| r.as_str().to_string())
    };
    let path = vec![owner.clone(), repo.name.clone()];

    let star_count = state.repository_stars.count_for_repository(repo.id).await?;
    let is_starred = state.repository_stars.is_starred(repo.id, user_id).await?;
    let git_backend = state.git_backend.clone();
    let disk_path = repo.disk_path.clone();
    let size_bytes = tokio::task::spawn_blocking(move || git_backend.directory_size(&disk_path))
        .await
        .ok()
        .and_then(|r| r.ok());

    Ok(Json(RepositoryResponse {
        id: repo.id,
        name: repo.name,
        description: repo.description.clone(),
        owner,
        role: Some(role),
        visibility: repo.visibility.as_str().to_string(),
        created_at: repo.created_at,
        path,
        star_count: Some(star_count),
        is_starred: Some(is_starred),
        size_bytes,
    }))
}

async fn get_by_id(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<RepositoryResponse>, ApiError> {
    let repo = require_role_by_id(&state, user_id, id, CollaboratorRole::Reader).await?;
    Ok(Json(repository_summary(&state, repo, Some(user_id)).await?))
}

/// `viewer` is `None` for an anonymous visitor of a public repository, who has no role and has starred nothing.
pub(crate) async fn repository_summary(
    state: &AppState,
    repo: Repository,
    viewer: Option<Uuid>,
) -> Result<RepositoryResponse, ApiError> {
    let owner = state
        .users
        .find_by_id(repo.owner_id)
        .await?
        .ok_or_else(|| DomainError::NotFound("user".to_string()))?
        .username;
    let (role, path) = match repo.group_id {
        None => {
            let role = match viewer {
                None => None,
                Some(user_id) if repo.owner_id == user_id => Some("owner".to_string()),
                Some(user_id) => Some(
                    state
                        .repository_collaborators
                        .get_role(repo.id, user_id)
                        .await?
                        .map_or_else(|| "reader".to_string(), |r| r.as_str().to_string()),
                ),
            };
            (role, vec![owner.clone(), repo.name.clone()])
        }
        Some(group_id) => {
            let chain = state.groups.ancestor_chain(group_id).await?;
            let role = match viewer {
                None => None,
                Some(user_id) => {
                    let group_role = effective_role_in_group_chain(
                        state.group_membership.as_ref(),
                        &chain,
                        user_id,
                    )
                    .await?;
                    let direct_role = state
                        .repository_collaborators
                        .get_role(repo.id, user_id)
                        .await?;
                    let best = match (group_role, direct_role) {
                        (Some(a), Some(b)) => a.max(b),
                        (Some(a), None) | (None, Some(a)) => a,
                        // Public repositories bypass the role check, so (None, None) is expected and means Reader.
                        (None, None) => CollaboratorRole::Reader,
                    };
                    Some(best.as_str().to_string())
                }
            };
            let mut path: Vec<String> = chain.into_iter().map(|g| g.name).collect();
            path.push(repo.name.clone());
            (role, path)
        }
    };

    let star_count = state.repository_stars.count_for_repository(repo.id).await?;
    let is_starred = match viewer {
        Some(user_id) => state.repository_stars.is_starred(repo.id, user_id).await?,
        None => false,
    };
    let git_backend = state.git_backend.clone();
    let disk_path = repo.disk_path.clone();
    let size_bytes = tokio::task::spawn_blocking(move || git_backend.directory_size(&disk_path))
        .await
        .ok()
        .and_then(|r| r.ok());

    Ok(RepositoryResponse {
        id: repo.id,
        name: repo.name,
        description: repo.description,
        owner,
        role,
        visibility: repo.visibility.as_str().to_string(),
        created_at: repo.created_at,
        path,
        star_count: Some(star_count),
        is_starred: Some(is_starred),
        size_bytes,
    })
}

#[derive(Deserialize)]
pub(crate) struct CommitsQuery {
    #[serde(rename = "ref")]
    pub(crate) r#ref: Option<String>,
}

const COMMITS_PAGE_SIZE: usize = 20;

/// An unresolvable `ref` (unknown, or a blob/tree) is a 404; `HEAD` of an empty repository is an empty list.
pub(crate) async fn commits_response(
    state: &AppState,
    disk_path: &str,
    r#ref: Option<String>,
) -> Result<Json<Vec<CommitInfo>>, ApiError> {
    let full_path = PathBuf::from(&state.config.storage_root).join(disk_path);
    let git_reader = state.git_reader.clone();
    // `HEAD` is the default. When asked for by name it must still list nothing (not 404) on an empty repository.
    let explicit_ref = r#ref.as_deref().is_some_and(|r| r != "HEAD");
    let revision = r#ref.unwrap_or_else(|| "HEAD".to_string());
    let commits = tokio::task::spawn_blocking(move || {
        if explicit_ref && git_reader.resolve_commit(&full_path, &revision)?.is_none() {
            return Ok(None);
        }
        git_reader
            .list_commits_at(&full_path, &revision, COMMITS_PAGE_SIZE)
            .map(Some)
    })
    .await
    .map_err(|e| DomainError::Infrastructure(e.to_string()))?
    .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
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
    let full_path = PathBuf::from(&state.config.storage_root).join(&repo.disk_path);
    let git_reader = state.git_reader.clone();
    let revision = r#ref.to_string();
    let path = path.to_string();
    let with_last_commits = query.last_commit.unwrap_or(true);
    let entries = tokio::task::spawn_blocking(move || {
        let Some(commit_id) = git_reader.resolve_revision(&full_path, &revision)? else {
            return Ok(None);
        };
        if with_last_commits {
            git_reader.list_tree_at_revision(&full_path, &commit_id.to_string(), &path)
        } else {
            git_reader.list_tree_names_at_revision(&full_path, &commit_id.to_string(), &path)
        }
    })
    .await
    .map_err(|e| DomainError::Infrastructure(e.to_string()))?
    .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
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
    let full_path = PathBuf::from(&state.config.storage_root).join(&repo.disk_path);
    let git_reader = state.git_reader.clone();
    let (sha, size, bytes) =
        tokio::task::spawn_blocking(move || -> Result<Option<BlobAtRevision>, GitReadError> {
            let Some(commit_id) = git_reader.resolve_revision(&full_path, &r#ref)? else {
                return Ok(None);
            };
            let sha = commit_id.to_string();
            let Some(size) = git_reader.blob_size_at_revision(&full_path, &sha, &path)? else {
                return Ok(None);
            };
            if size as usize > MAX_PREVIEWABLE_FILE_SIZE {
                return Ok(Some((sha, size, None)));
            }
            let bytes = git_reader.read_file_at_revision(&full_path, &sha, &path)?;
            Ok(bytes.map(|b| (sha, size, Some(b))))
        })
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?
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
    let full_path = PathBuf::from(&state.config.storage_root).join(&repo.disk_path);
    let git_reader = state.git_reader.clone();
    let content = tokio::task::spawn_blocking(move || -> Result<Option<Vec<u8>>, GitReadError> {
        let Some(commit_id) = git_reader.resolve_revision(&full_path, &r#ref)? else {
            return Ok(None);
        };
        let sha = commit_id.to_string();
        let Some(entries) =
            git_reader.list_tree_at_revision_with_window(&full_path, &sha, "", 0)?
        else {
            return Ok(None);
        };
        let Some(readme_entry) = entries
            .iter()
            .find(|e| !e.is_dir && e.name.eq_ignore_ascii_case("README.md"))
        else {
            return Ok(None);
        };
        git_reader.read_file_at_revision(&full_path, &sha, &readme_entry.name)
    })
    .await
    .map_err(|e| DomainError::Infrastructure(e.to_string()))?
    .map_err(|e| DomainError::Infrastructure(e.to_string()))?;

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
    let full_path = PathBuf::from(&state.config.storage_root).join(&repo.disk_path);
    let git_reader = state.git_reader.clone();
    let contributors =
        tokio::task::spawn_blocking(move || -> Result<Vec<ContributorInfo>, GitReadError> {
            let Some(commit_id) = git_reader.resolve_revision(&full_path, &r#ref)? else {
                return Ok(Vec::new());
            };
            Ok(git_reader
                .list_contributors(&full_path, &commit_id.to_string())?
                .unwrap_or_default())
        })
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
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
    let full_path = PathBuf::from(&state.config.storage_root).join(&repo.disk_path);
    let git_reader = state.git_reader.clone();
    let languages =
        tokio::task::spawn_blocking(move || -> Result<Vec<LanguageStat>, GitReadError> {
            let Some(commit_id) = git_reader.resolve_revision(&full_path, &r#ref)? else {
                return Ok(Vec::new());
            };
            Ok(git_reader
                .compute_language_stats(&full_path, &commit_id.to_string())?
                .unwrap_or_default())
        })
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
    Ok(Json(LanguagesResponse { languages }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StarResponse {
    star_count: i64,
    is_starred: bool,
}

async fn star_by_id(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<StarResponse>, ApiError> {
    require_role_by_id(&state, user_id, id, CollaboratorRole::Reader).await?;
    state.repository_stars.add(id, user_id).await?;
    let star_count = state.repository_stars.count_for_repository(id).await?;
    Ok(Json(StarResponse {
        star_count,
        is_starred: true,
    }))
}

async fn unstar_by_id(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<StarResponse>, ApiError> {
    require_role_by_id(&state, user_id, id, CollaboratorRole::Reader).await?;
    state.repository_stars.remove(id, user_id).await?;
    let star_count = state.repository_stars.count_for_repository(id).await?;
    Ok(Json(StarResponse {
        star_count,
        is_starred: false,
    }))
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

async fn delete_by_id(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    require_role_by_id(&state, user_id, id, CollaboratorRole::Maintainer).await?;

    let use_case = DeleteRepositoryUseCase::new(
        state.repositories.clone(),
        state.release_asset_storage.clone(),
    );
    use_case
        .execute(id, |repo| remove_git_storage(&state.git_backend, repo))
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// A filesystem failure is logged, never returned: the deletion already happened. Also used by the admin
/// deletion of a user.
pub(crate) fn remove_git_storage(git_backend: &Arc<GitBackend>, repo: &Repository) {
    let git_backend = git_backend.clone();
    let disk_path = repo.disk_path.clone();
    let wiki_disk_path = ferrisgit_domain::wiki::wiki_disk_path_for(&disk_path);
    let repository_id = repo.id;
    let run = move || {
        if let Err(e) = git_backend.remove_bare_repo(&disk_path) {
            tracing::warn!(error = %e, %repository_id, "failed to remove repository's bare git directory");
        }
        if let Err(e) = git_backend.remove_bare_repo(&wiki_disk_path) {
            tracing::warn!(error = %e, %repository_id, "failed to remove repository's wiki git directory");
        }
    };
    // `block_in_place` panics on a current-thread runtime, which `#[sqlx::test]` uses.
    if tokio::runtime::Handle::current().runtime_flavor()
        == tokio::runtime::RuntimeFlavor::MultiThread
    {
        tokio::task::block_in_place(run);
    } else {
        run();
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/repositories", post(create).get(list))
        .route("/repositories/{owner}/{name}", get(get_one))
        .route(
            "/repositories/by-id/{id}",
            get(get_by_id).delete(delete_by_id),
        )
        .route(
            "/repositories/by-id/{id}/star",
            post(star_by_id).delete(unstar_by_id),
        )
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
