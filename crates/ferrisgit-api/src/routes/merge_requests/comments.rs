use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::merge_request_timeline::is_outdated;
use ferrisgit_application::use_cases::add_merge_request_comment::AddMergeRequestCommentUseCase;
use ferrisgit_application::use_cases::resolve_merge_request_comment::ResolveMergeRequestCommentUseCase;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::merge_request_comment::MergeRequestComment;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::error::ApiError;
use crate::routes::user_ref::{UserRef, load_user_refs};
use crate::state::AppState;

use super::diff::live_diffs;
use super::find_accessible_merge_request;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CommentResponse {
    id: Uuid,
    author_id: Option<Uuid>,
    body: String,
    created_at: DateTime<Utc>,
    reply_to_id: Option<Uuid>,
    file_path: Option<String>,
    line_number: Option<i32>,
    end_line: Option<i32>,
    side: Option<String>,
    outdated: bool,
    resolved: bool,
    suggested_content: Option<String>,
    applied_at: Option<DateTime<Utc>>,
    applied_commit_sha: Option<String>,
    author: Option<UserRef>,
}

impl CommentResponse {
    pub(super) fn from_comment(
        comment: MergeRequestComment,
        outdated: bool,
        author: Option<UserRef>,
    ) -> Self {
        CommentResponse {
            id: comment.id,
            author_id: comment.author_id,
            body: comment.body,
            created_at: comment.created_at,
            reply_to_id: comment.reply_to_id,
            file_path: comment.file_path,
            line_number: comment.line_number,
            end_line: comment.end_line,
            side: comment.side.map(|s| s.as_str().to_string()),
            outdated,
            resolved: comment.resolved,
            suggested_content: comment.suggested_content,
            applied_at: comment.applied_at,
            applied_commit_sha: comment.applied_commit_sha,
            author,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddCommentRequest {
    body: String,
    #[serde(default)]
    reply_to_id: Option<Uuid>,
    #[serde(default)]
    file_path: Option<String>,
    #[serde(default)]
    line_number: Option<i32>,
    #[serde(default)]
    end_line: Option<i32>,
    #[serde(default)]
    side: Option<String>,
    #[serde(default)]
    suggested_content: Option<String>,
}

async fn list_comments(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(merge_request_id): Path<Uuid>,
) -> Result<Json<Vec<CommentResponse>>, ApiError> {
    let (mr, repo) =
        find_accessible_merge_request(&state, user_id, merge_request_id, CollaboratorRole::Reader)
            .await?;
    let comments = state
        .merge_request_comments
        .list_comments(merge_request_id)
        .await?;
    let diffs = live_diffs(&state, &mr, &repo).await?;
    let authors = load_user_refs(&state, comments.iter().filter_map(|c| c.author_id)).await;

    Ok(Json(
        comments
            .into_iter()
            .map(|c| {
                let outdated = diffs.as_deref().is_some_and(|diffs| is_outdated(diffs, &c));
                let author = c.author_id.and_then(|id| authors.get(&id).cloned());
                CommentResponse::from_comment(c, outdated, author)
            })
            .collect(),
    ))
}

async fn add_comment(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(merge_request_id): Path<Uuid>,
    Json(req): Json<AddCommentRequest>,
) -> Result<Json<CommentResponse>, ApiError> {
    find_accessible_merge_request(
        &state,
        user_id,
        merge_request_id,
        CollaboratorRole::Contributor,
    )
    .await?;

    let anchor = match (req.file_path, req.line_number, req.side) {
        (None, None, None) => {
            if req.end_line.is_some() || req.suggested_content.is_some() {
                return Err(DomainError::Validation(
                    "endLine and suggestedContent require filePath, lineNumber and side"
                        .to_string(),
                )
                .into());
            }
            None
        }
        (Some(file_path), Some(line_number), Some(side)) => {
            let side = ferrisgit_domain::diff::DiffSide::parse(&side)?;
            Some(
                ferrisgit_application::use_cases::add_merge_request_comment::PostedAnchor {
                    file_path,
                    line_number,
                    end_line: req.end_line,
                    side,
                },
            )
        }
        _ => {
            return Err(DomainError::Validation(
                "filePath, lineNumber and side must all be present, or all absent".to_string(),
            )
            .into());
        }
    };

    let use_case = AddMergeRequestCommentUseCase::new(
        state.merge_requests.clone(),
        state.merge_request_comments.clone(),
        state.repositories.clone(),
        state.users.clone(),
        state.notifications.clone(),
        state.diff_reader.clone(),
        state.webhooks.clone(),
    );
    let comment = use_case
        .execute(
            merge_request_id,
            user_id,
            req.body,
            req.reply_to_id,
            anchor,
            req.suggested_content,
        )
        .await?;
    let author = load_user_refs(&state, comment.author_id)
        .await
        .into_values()
        .next();
    Ok(Json(CommentResponse::from_comment(comment, false, author)))
}

async fn apply_suggestion(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((merge_request_id, comment_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<CommentResponse>, ApiError> {
    find_accessible_merge_request(
        &state,
        user_id,
        merge_request_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    let use_case = ferrisgit_application::use_cases::apply_suggestion_comment::ApplySuggestionCommentUseCase::new(
        state.merge_requests.clone(),
        state.merge_request_comments.clone(),
        state.repositories.clone(),
        state.users.clone(),
        state.diff_reader.clone(),
        state.branch_reader.clone(),
        state.suggestion_executor.clone(),
    );
    let comment = use_case
        .execute(merge_request_id, comment_id, user_id)
        .await?;
    let author = load_user_refs(&state, comment.author_id)
        .await
        .into_values()
        .next();
    Ok(Json(CommentResponse::from_comment(comment, false, author)))
}

async fn resolve_comment(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((merge_request_id, comment_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    find_accessible_merge_request(
        &state,
        user_id,
        merge_request_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    let use_case = ResolveMergeRequestCommentUseCase::new(state.merge_request_comments.clone());
    use_case.execute(merge_request_id, comment_id, true).await?;
    state
        .merge_request_activity
        .thread_resolved(merge_request_id, user_id, comment_id, true)
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn unresolve_comment(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((merge_request_id, comment_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    find_accessible_merge_request(
        &state,
        user_id,
        merge_request_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    let use_case = ResolveMergeRequestCommentUseCase::new(state.merge_request_comments.clone());
    use_case
        .execute(merge_request_id, comment_id, false)
        .await?;
    state
        .merge_request_activity
        .thread_resolved(merge_request_id, user_id, comment_id, false)
        .await;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/merge-requests/{id}/comments",
            get(list_comments).post(add_comment),
        )
        .route(
            "/merge-requests/{id}/comments/{comment_id}/resolve",
            post(resolve_comment),
        )
        .route(
            "/merge-requests/{id}/comments/{comment_id}/unresolve",
            post(unresolve_comment),
        )
        .route(
            "/merge-requests/{id}/comments/{comment_id}/apply-suggestion",
            post(apply_suggestion),
        )
}
