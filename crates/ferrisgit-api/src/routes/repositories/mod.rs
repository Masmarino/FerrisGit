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
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::authz::{
    effective_role_in_group_chain, require_group_chain_role, require_role, require_role_by_id,
    role_label_or_reader,
};
use crate::error::ApiError;
use crate::routes::user_ref::require_user;
use crate::state::AppState;

mod browse;

pub(crate) use browse::{
    BlobResponse, CommitsQuery, LanguagesResponse, ReadmeResponse, TreeQuery, blob_response,
    commits_response, contributors_response, languages_response, readme_response, tree_response,
};

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

/// Both fields are optional and only the ones sent change. Name and owner can't be edited, they're part of the
/// clone URL.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateRepositoryRequest {
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    visibility: Option<String>,
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
    /// The caller's role, `null` for an anonymous visitor of a public repository.
    role: Option<String>,
    visibility: String,
    created_at: DateTime<Utc>,
    /// Group repositories have no `{owner}/{name}` link, so listings have to link through this.
    path: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    star_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_starred: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_bytes: Option<u64>,
}

impl RepositoryResponse {
    /// A listing entry: no star or size fields in the JSON. For a group's repositories, resolve the caller's `role`
    /// once per group rather than per repository.
    pub(crate) fn new(
        repo: &Repository,
        owner: String,
        role: Option<String>,
        path: Vec<String>,
    ) -> Self {
        Self {
            id: repo.id,
            name: repo.name.clone(),
            description: repo.description.clone(),
            owner,
            role,
            visibility: repo.visibility.as_str().to_string(),
            created_at: repo.created_at,
            path,
            star_count: None,
            is_starred: None,
            size_bytes: None,
        }
    }

    fn with_stats(self, star_count: i64, is_starred: bool, size_bytes: Option<u64>) -> Self {
        Self {
            star_count: Some(star_count),
            is_starred: Some(is_starred),
            size_bytes,
            ..self
        }
    }
}

/// `None` when the size can't be read, a repository page isn't worth failing over it.
async fn directory_size(state: &AppState, repo: &Repository) -> Option<u64> {
    let git_backend = state.git_backend.clone();
    let disk_path = repo.disk_path.clone();
    tokio::task::spawn_blocking(move || git_backend.directory_size(&disk_path))
        .await
        .ok()
        .and_then(|r| r.ok())
}

/// If you already have the group's `ancestor_chain`, build the path from it instead.
pub(crate) async fn repository_path(
    state: &AppState,
    repo: &Repository,
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
            let (chain, _) =
                require_group_chain_role(&state, user_id, group.id, CollaboratorRole::Maintainer)
                    .await?;
            let names: Vec<String> = chain.into_iter().map(|g| g.name).collect();
            (Some(group.id), Some(names))
        }
    };

    let use_case = CreateRepositoryUseCase::new(
        state.repositories.clone(),
        state.repository_settings.clone(),
    );
    let git_backend = &state.git_backend;

    let repo = use_case
        .execute(
            user_id,
            req.name,
            req.description,
            group_id,
            visibility,
            settings_update,
            |disk_path| run_blocking(|| git_backend.init_bare_repo(disk_path)),
        )
        .await?;

    let caller = require_user(&state, user_id).await?;
    let path = match group_path_names {
        Some(mut names) => {
            names.push(repo.name.clone());
            names
        }
        None => vec![caller.username.clone(), repo.name.clone()],
    };
    Ok(Json(RepositoryResponse::new(
        &repo,
        caller.username,
        Some("owner".to_string()),
        path,
    )))
}

async fn list(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Query(query): Query<ListRepositoriesQuery>,
) -> Result<Json<Vec<RepositoryResponse>>, ApiError> {
    let caller = require_user(&state, user_id).await?;

    let mut responses: Vec<RepositoryResponse> = state
        .repositories
        .list_for_owner(user_id)
        .await?
        .into_iter()
        .map(|r| {
            let path = vec![caller.username.clone(), r.name.clone()];
            RepositoryResponse::new(&r, caller.username.clone(), Some("owner".to_string()), path)
        })
        .collect();

    for collaboration in state
        .repository_collaborators
        .list_collaborations_for_user(user_id)
        .await?
    {
        let repo = collaboration.repository;
        let path = repository_path(&state, &repo, &collaboration.owner_username).await?;
        responses.push(RepositoryResponse::new(
            &repo,
            collaboration.owner_username,
            Some(collaboration.role.as_str().to_string()),
            path,
        ));
    }

    // Include repositories reached only through group membership, otherwise inherited access would list nothing.
    let mut owner_usernames: std::collections::HashMap<Uuid, String> =
        std::collections::HashMap::new();
    let member_group_ids = state.groups.list_member_group_ids(user_id).await?;
    for group_id in member_group_ids {
        let repos = state.repositories.list_for_group(group_id).await?;
        if repos.is_empty() {
            continue;
        }
        // A group's repositories share the chain and the caller's role, so resolve them once per group.
        let chain = state.groups.ancestor_chain(group_id).await?;
        let group_path: Vec<String> = chain.iter().map(|g| g.name.clone()).collect();
        let role = role_label_or_reader(
            effective_role_in_group_chain(state.group_membership.as_ref(), &chain, user_id).await?,
        );
        for repo in repos {
            let owner_username = if let Some(name) = owner_usernames.get(&repo.owner_id) {
                name.clone()
            } else {
                let owner = require_user(&state, repo.owner_id).await?;
                owner_usernames.insert(repo.owner_id, owner.username.clone());
                owner.username
            };
            let mut path = group_path.clone();
            path.push(repo.name.clone());
            responses.push(RepositoryResponse::new(
                &repo,
                owner_username,
                Some(role.clone()),
                path,
            ));
        }
    }

    // A repository can show up both as collaborator and through a group. Keep the first, the collaborator one,
    // since its role is more precise.
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
        role_label_or_reader(
            state
                .repository_collaborators
                .get_role(repo.id, user_id)
                .await?,
        )
    };
    let path = vec![owner.clone(), repo.name.clone()];

    let star_count = state.repository_stars.count_for_repository(repo.id).await?;
    let is_starred = state.repository_stars.is_starred(repo.id, user_id).await?;
    let size_bytes = directory_size(&state, &repo).await;

    Ok(Json(
        RepositoryResponse::new(&repo, owner, Some(role), path)
            .with_stats(star_count, is_starred, size_bytes),
    ))
}

async fn get_by_id(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<RepositoryResponse>, ApiError> {
    let repo = require_role_by_id(&state, user_id, id, CollaboratorRole::Reader).await?;
    Ok(Json(repository_summary(&state, repo, Some(user_id)).await?))
}

/// `viewer` is `None` for an anonymous visitor of a public repository: no role, no stars.
pub(crate) async fn repository_summary(
    state: &AppState,
    repo: Repository,
    viewer: Option<Uuid>,
) -> Result<RepositoryResponse, ApiError> {
    let owner = require_user(state, repo.owner_id).await?.username;
    let (role, path) = match repo.group_id {
        None => {
            let role = match viewer {
                None => None,
                Some(user_id) if repo.owner_id == user_id => Some("owner".to_string()),
                Some(user_id) => Some(role_label_or_reader(
                    state
                        .repository_collaborators
                        .get_role(repo.id, user_id)
                        .await?,
                )),
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
                        // Public repositories skip the role check, so no role at all just means Reader.
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
    let size_bytes = directory_size(state, &repo).await;

    Ok(RepositoryResponse::new(&repo, owner, role, path)
        .with_stats(star_count, is_starred, size_bytes))
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

/// Same guard as the other repository settings. Making a repository private drops it from the public catalog and
/// anonymous pages immediately, they read the stored visibility on every request.
async fn update_by_id(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateRepositoryRequest>,
) -> Result<Json<RepositoryResponse>, ApiError> {
    let repo = require_role_by_id(&state, user_id, id, CollaboratorRole::Maintainer).await?;
    let visibility = req
        .visibility
        .as_deref()
        .map(RepositoryVisibility::parse)
        .transpose()?;
    if req.description.is_none() && visibility.is_none() {
        return Ok(Json(repository_summary(&state, repo, Some(user_id)).await?));
    }
    let updated = state
        .repositories
        .update_details(id, req.description, visibility)
        .await?;
    Ok(Json(
        repository_summary(&state, updated, Some(user_id)).await?,
    ))
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

/// A filesystem failure is only logged, the deletion already happened. Also used when an admin deletes a user.
pub(crate) fn remove_git_storage(git_backend: &Arc<GitBackend>, repo: &Repository) {
    let git_backend = git_backend.clone();
    let disk_path = repo.disk_path.clone();
    let wiki_disk_path = ferrisgit_domain::wiki::wiki_disk_path_for(&disk_path);
    let repository_id = repo.id;
    run_blocking(move || {
        if let Err(e) = git_backend.remove_bare_repo(&disk_path) {
            tracing::warn!(error = %e, %repository_id, "failed to remove repository's bare git directory");
        }
        if let Err(e) = git_backend.remove_bare_repo(&wiki_disk_path) {
            tracing::warn!(error = %e, %repository_id, "failed to remove repository's wiki git directory");
        }
    });
}

/// Runs a short blocking filesystem call. `block_in_place` panics on a current-thread runtime, which is what
/// `#[sqlx::test]` uses, so there it runs inline.
fn run_blocking<T>(work: impl FnOnce() -> T) -> T {
    if tokio::runtime::Handle::current().runtime_flavor()
        == tokio::runtime::RuntimeFlavor::MultiThread
    {
        tokio::task::block_in_place(work)
    } else {
        work()
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/repositories", post(create).get(list))
        .route("/repositories/{owner}/{name}", get(get_one))
        .route(
            "/repositories/by-id/{id}",
            get(get_by_id).patch(update_by_id).delete(delete_by_id),
        )
        .route(
            "/repositories/by-id/{id}/star",
            post(star_by_id).delete(unstar_by_id),
        )
        .merge(browse::router())
}
