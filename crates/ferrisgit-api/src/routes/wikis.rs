use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::use_cases::delete_wiki_page::DeleteWikiPageUseCase;
use ferrisgit_application::use_cases::save_wiki_page::SaveWikiPageUseCase;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use ferrisgit_domain::wiki_page::WikiRevision;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::authz::require_role_by_id;
use crate::error::ApiError;
use crate::routes::user_ref::require_user;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WikiPageSummaryResponse {
    slug: String,
    title: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WikiListResponse {
    head_sha: Option<String>,
    pages: Vec<WikiPageSummaryResponse>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WikiPageDetailResponse {
    content: String,
    head_sha: String,
    title: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveWikiPageRequest {
    content: String,
    base_sha: Option<String>,
    message: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteWikiPageQuery {
    base_sha: String,
}

async fn list(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
) -> Result<Json<WikiListResponse>, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Reader).await?;

    let Some(wiki) = state.wikis.find_by_repository_id(repository_id).await? else {
        // No wiki row yet is normal: answer an empty list, and a read must never create the row.
        return Ok(Json(WikiListResponse {
            head_sha: None,
            pages: Vec::new(),
        }));
    };
    let head_sha = state.wiki_reader.current_head_sha(&wiki.disk_path).await?;
    let pages = state.wiki_reader.list_pages(&wiki.disk_path).await?;
    Ok(Json(WikiListResponse {
        head_sha,
        pages: pages
            .into_iter()
            .map(|p| WikiPageSummaryResponse {
                slug: p.slug,
                title: p.title,
            })
            .collect(),
    }))
}

async fn detail(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, slug)): Path<(Uuid, String)>,
) -> Result<Json<WikiPageDetailResponse>, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Reader).await?;

    let wiki = state
        .wikis
        .find_by_repository_id(repository_id)
        .await?
        .ok_or_else(|| DomainError::NotFound("wiki page".to_string()))?;
    let content = state
        .wiki_reader
        .read_page(&wiki.disk_path, &slug)
        .await?
        .ok_or_else(|| DomainError::NotFound("wiki page".to_string()))?;
    Ok(Json(WikiPageDetailResponse {
        content: content.content,
        head_sha: content.head_sha,
        title: ferrisgit_domain::wiki_page::wiki_page_title_from_slug(&slug),
    }))
}

async fn save(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, slug)): Path<(Uuid, String)>,
    Json(req): Json<SaveWikiPageRequest>,
) -> Result<Json<WikiPageDetailResponse>, ApiError> {
    require_role_by_id(
        &state,
        user_id,
        repository_id,
        CollaboratorRole::Contributor,
    )
    .await?;

    let user = require_user(&state, user_id).await?;
    let use_case = SaveWikiPageUseCase::new(
        state.repositories.clone(),
        state.wikis.clone(),
        state.wiki_writer.clone(),
    );
    let message = req.message.unwrap_or_else(|| format!("Update {slug}"));
    let revision = use_case
        .execute(
            repository_id,
            &slug,
            &req.content,
            req.base_sha.as_deref(),
            &user.username,
            &user.email,
            &message,
        )
        .await?;

    Ok(Json(WikiPageDetailResponse {
        content: req.content,
        head_sha: revision.commit_sha,
        title: ferrisgit_domain::wiki_page::wiki_page_title_from_slug(&slug),
    }))
}

/// `baseSha` goes in the query string because none of our DELETE routes takes a JSON body.
async fn delete(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, slug)): Path<(Uuid, String)>,
    Query(query): Query<DeleteWikiPageQuery>,
) -> Result<StatusCode, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;

    let user = require_user(&state, user_id).await?;
    let use_case = DeleteWikiPageUseCase::new(state.wikis.clone(), state.wiki_writer.clone());
    use_case
        .execute(
            repository_id,
            &slug,
            &query.base_sha,
            &user.username,
            &user.email,
            &format!("Delete {slug}"),
        )
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WikiRevisionResponse {
    commit_sha: String,
    author_name: String,
    author_email: String,
    committed_at: DateTime<Utc>,
    message: String,
}

impl From<WikiRevision> for WikiRevisionResponse {
    fn from(r: WikiRevision) -> Self {
        Self {
            commit_sha: r.commit_sha,
            author_name: r.author_name,
            author_email: r.author_email,
            committed_at: r.committed_at,
            message: r.message,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WikiPageRevisionContentResponse {
    content: String,
}

async fn revisions(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, slug)): Path<(Uuid, String)>,
) -> Result<Json<Vec<WikiRevisionResponse>>, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Reader).await?;
    let wiki = state
        .wikis
        .find_by_repository_id(repository_id)
        .await?
        .ok_or_else(|| DomainError::NotFound("wiki page".to_string()))?;
    let revisions = state
        .wiki_reader
        .list_page_revisions(&wiki.disk_path, &slug)
        .await?;
    Ok(Json(revisions.into_iter().map(Into::into).collect()))
}

async fn revision_content(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, slug, commit_sha)): Path<(Uuid, String, String)>,
) -> Result<Json<WikiPageRevisionContentResponse>, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Reader).await?;
    let wiki = state
        .wikis
        .find_by_repository_id(repository_id)
        .await?
        .ok_or_else(|| DomainError::NotFound("wiki page".to_string()))?;
    let content = state
        .wiki_reader
        .read_page_at_revision(&wiki.disk_path, &slug, &commit_sha)
        .await?
        .ok_or_else(|| DomainError::NotFound("wiki page revision".to_string()))?;
    Ok(Json(WikiPageRevisionContentResponse { content }))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/repositories/{repository_id}/wiki", get(list))
        .route(
            "/repositories/{repository_id}/wiki/pages/{slug}",
            get(detail).put(save).delete(delete),
        )
        .route(
            "/repositories/{repository_id}/wiki/pages/{slug}/revisions",
            get(revisions),
        )
        .route(
            "/repositories/{repository_id}/wiki/pages/{slug}/revisions/{commit_sha}",
            get(revision_content),
        )
}
