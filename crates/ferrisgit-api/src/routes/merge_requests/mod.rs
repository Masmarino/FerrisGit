use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::use_cases::close_merge_request::CloseMergeRequestUseCase;
use ferrisgit_application::use_cases::create_merge_request::CreateMergeRequestUseCase;
use ferrisgit_application::use_cases::merge_merge_request::{
    MergeMergeRequestResult, MergeMergeRequestUseCase,
};
use ferrisgit_application::use_cases::set_merge_request_labels::SetMergeRequestLabelsUseCase;
use ferrisgit_application::use_cases::update_merge_request::UpdateMergeRequestUseCase;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::merge_request::MergeRequest;
use ferrisgit_domain::repository::Repository;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::authz::require_role_by_id;
use crate::error::ApiError;
use crate::routes::labels::{LabelFilterQuery, LabelResponse, SetLabelsRequest};
use crate::routes::user_ref::{UserRef, load_user_refs};
use crate::state::AppState;

mod comments;
mod diff;
mod reviews;
mod timeline;

async fn find_accessible_merge_request(
    state: &AppState,
    user_id: Uuid,
    merge_request_id: Uuid,
    min_role: CollaboratorRole,
) -> Result<(MergeRequest, Repository), DomainError> {
    let mr = state
        .merge_requests
        .find_by_id(merge_request_id)
        .await?
        .ok_or_else(|| DomainError::NotFound("merge request".to_string()))?;
    let repo = require_role_by_id(state, user_id, mr.repository_id, min_role).await?;
    Ok((mr, repo))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BranchResponse {
    name: String,
    tip_sha: String,
    is_default: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MergeRequestResponse {
    id: Uuid,
    source_branch: String,
    target_branch: String,
    title: String,
    description: String,
    status: String,
    merge_commit_sha: Option<String>,
    milestone_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    closed_at: Option<DateTime<Utc>>,
    labels: Vec<LabelResponse>,
    author: Option<UserRef>,
    comment_count: i64,
}

impl From<MergeRequest> for MergeRequestResponse {
    fn from(mr: MergeRequest) -> Self {
        MergeRequestResponse {
            id: mr.id,
            source_branch: mr.source_branch,
            target_branch: mr.target_branch,
            title: mr.title,
            description: mr.description,
            status: mr.status.as_str().to_string(),
            merge_commit_sha: mr.merge_commit_sha,
            milestone_id: mr.milestone_id,
            created_at: mr.created_at,
            closed_at: mr.closed_at,
            labels: Vec::new(),
            author: None,
            comment_count: 0,
        }
    }
}

impl MergeRequestResponse {
    /// Every handler that returns a merge request builds it here. The detail page swaps its state for the update and
    /// merge responses, so they all need the same fields.
    async fn build_many(
        state: &AppState,
        merge_requests: Vec<MergeRequest>,
    ) -> Result<Vec<Self>, DomainError> {
        let ids: Vec<Uuid> = merge_requests.iter().map(|mr| mr.id).collect();
        let labels_by_mr = state.labels.list_for_merge_requests(&ids).await?;
        let comment_counts = state.merge_request_comments.comment_counts(&ids).await?;
        let authors =
            load_user_refs(state, merge_requests.iter().filter_map(|mr| mr.author_id)).await;
        Ok(merge_requests
            .into_iter()
            .map(|mr| {
                let labels = labels_by_mr
                    .iter()
                    .filter(|(id, _)| *id == mr.id)
                    .map(|(_, l)| l.clone().into())
                    .collect();
                let author = mr.author_id.and_then(|id| authors.get(&id).cloned());
                let comment_count = comment_counts.get(&mr.id).copied().unwrap_or(0);
                MergeRequestResponse {
                    labels,
                    author,
                    comment_count,
                    ..mr.into()
                }
            })
            .collect())
    }

    async fn build(state: &AppState, mr: MergeRequest) -> Result<Self, DomainError> {
        let mut built = Self::build_many(state, vec![mr]).await?;
        Ok(built.remove(0))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateMergeRequestRequest {
    source_branch: String,
    target_branch: String,
    title: String,
    description: String,
}

// Built once per merge attempt and serialized right away, so boxing the large variant would only add an allocation.
#[allow(clippy::large_enum_variant)]
#[derive(Serialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
enum MergeAttemptResponse {
    #[serde(rename = "merged")]
    Merged {
        #[serde(flatten)]
        merge_request: MergeRequestResponse,
    },
    #[serde(rename = "conflicting")]
    Conflicting,
}

async fn list_branches(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
) -> Result<Json<Vec<BranchResponse>>, ApiError> {
    let repo = require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Reader).await?;
    branches_response(&state, &repo).await
}

pub(crate) async fn branches_response(
    state: &AppState,
    repo: &Repository,
) -> Result<Json<Vec<BranchResponse>>, ApiError> {
    let branches = state.branch_reader.list_branches(&repo.disk_path).await?;
    Ok(Json(
        branches
            .into_iter()
            .map(|b| BranchResponse {
                name: b.name,
                tip_sha: b.tip_sha,
                is_default: b.is_default,
            })
            .collect(),
    ))
}

async fn list_for_repository(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
    Query(query): Query<LabelFilterQuery>,
) -> Result<Json<Vec<MergeRequestResponse>>, ApiError> {
    let repo = require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Reader).await?;
    let label_ids = query.label_ids()?;
    let merge_requests = state
        .merge_requests
        .list_for_repository_filtered(repo.id, label_ids, query.milestone_id)
        .await?;
    Ok(Json(
        MergeRequestResponse::build_many(&state, merge_requests).await?,
    ))
}

async fn detail(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(merge_request_id): Path<Uuid>,
) -> Result<Json<MergeRequestResponse>, ApiError> {
    let (mr, _repo) =
        find_accessible_merge_request(&state, user_id, merge_request_id, CollaboratorRole::Reader)
            .await?;
    Ok(Json(MergeRequestResponse::build(&state, mr).await?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateMergeRequestRequest {
    title: String,
    description: String,
    /// Double `Option` so an absent field (`None`) differs from an explicit `null` (`Some(None)`, which clears it).
    /// With a plain `Option`, leaving out `milestoneId` would silently clear the milestone.
    #[serde(default, deserialize_with = "crate::routes::deserialize_present")]
    milestone_id: Option<Option<Uuid>>,
}

/// A failed lookup is an error, not "no milestone", so the caller never journals a change it couldn't verify.
async fn milestone_title(
    state: &AppState,
    milestone_id: Option<Uuid>,
) -> Result<Option<String>, DomainError> {
    let Some(milestone_id) = milestone_id else {
        return Ok(None);
    };
    Ok(state
        .milestones
        .find_by_id(milestone_id)
        .await?
        .map(|milestone| milestone.title))
}

async fn update(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(merge_request_id): Path<Uuid>,
    Json(req): Json<UpdateMergeRequestRequest>,
) -> Result<Json<MergeRequestResponse>, ApiError> {
    let (mr, repo) = find_accessible_merge_request(
        &state,
        user_id,
        merge_request_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    let milestone_id = req.milestone_id.ok_or_else(|| {
        DomainError::Validation(
            "milestoneId is required on a merge request update; send null to clear it".to_string(),
        )
    })?;
    let use_case = UpdateMergeRequestUseCase::new(
        state.merge_requests.clone(),
        state.milestones.clone(),
        state.groups.clone(),
    );
    let updated = use_case
        .execute(
            merge_request_id,
            &repo,
            req.title,
            req.description,
            milestone_id,
        )
        .await?;
    let milestones = match (
        milestone_title(&state, mr.milestone_id).await,
        milestone_title(&state, updated.milestone_id).await,
    ) {
        (Ok(before), Ok(after)) => Some((before, after)),
        (before, after) => {
            let err = before.err().or(after.err());
            tracing::warn!(error = ?err, merge_request_id = %merge_request_id, "milestone lookup failed; not recording a milestone change");
            None
        }
    };
    state
        .merge_request_activity
        .edited(merge_request_id, user_id, &mr, &updated, milestones)
        .await;
    Ok(Json(MergeRequestResponse::build(&state, updated).await?))
}

async fn set_labels(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(merge_request_id): Path<Uuid>,
    Json(req): Json<SetLabelsRequest>,
) -> Result<Json<Vec<LabelResponse>>, ApiError> {
    let (_mr, repo) = find_accessible_merge_request(
        &state,
        user_id,
        merge_request_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    // Best effort, the journal shouldn't become a new way for a label update to fail.
    let before = match state.labels.list_for_merge_request(merge_request_id).await {
        Ok(before) => Some(before),
        Err(err) => {
            tracing::warn!(error = %err, merge_request_id = %merge_request_id, "could not read the labels before the update; not recording a label change");
            None
        }
    };
    let use_case = SetMergeRequestLabelsUseCase::new(state.labels.clone(), state.groups.clone());
    let labels = use_case
        .execute(merge_request_id, &repo, req.label_ids)
        .await?;
    if let Some(before) = before {
        state
            .merge_request_activity
            .labels_changed(merge_request_id, user_id, &before, &labels)
            .await;
    }
    Ok(Json(labels.into_iter().map(Into::into).collect()))
}

async fn create(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
    Json(req): Json<CreateMergeRequestRequest>,
) -> Result<Json<MergeRequestResponse>, ApiError> {
    let repo = require_role_by_id(
        &state,
        user_id,
        repository_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    let use_case =
        CreateMergeRequestUseCase::new(state.merge_requests.clone(), state.branch_reader.clone());
    let mr = use_case
        .execute(
            repo.id,
            &repo.disk_path,
            user_id,
            req.source_branch,
            req.target_branch,
            req.title,
            req.description,
        )
        .await?;
    // Best effort: remember the source branch tip, so a later push can be journaled as "commits pushed" from it.
    if let Ok(branches) = state.branch_reader.list_branches(&repo.disk_path).await
        && let Some(tip) = branches.iter().find(|b| b.name == mr.source_branch)
    {
        state
            .merge_request_events
            .set_head_sha(mr.id, &tip.tip_sha)
            .await
            .ok();
    }
    Ok(Json(MergeRequestResponse::build(&state, mr).await?))
}

async fn merge(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(merge_request_id): Path<Uuid>,
) -> Result<Json<MergeAttemptResponse>, ApiError> {
    find_accessible_merge_request(
        &state,
        user_id,
        merge_request_id,
        CollaboratorRole::Maintainer,
    )
    .await?;
    let use_case = MergeMergeRequestUseCase::new(
        state.merge_requests.clone(),
        state.merge_request_reviews.clone(),
        state.merge_executor.clone(),
        state.repositories.clone(),
        state.repository_settings.clone(),
        state.system_settings.clone(),
        state.pipelines.clone(),
        state.jobs.clone(),
        state.pipeline_file_reader.clone(),
        state.pipeline_events.clone(),
        state.job_execution.clone(),
        state.branch_reader.clone(),
        state.users.clone(),
        state.notifications.clone(),
        state.webhooks.clone(),
    );
    match use_case.execute(merge_request_id, user_id).await? {
        MergeMergeRequestResult::Merged(mr) => {
            state
                .merge_request_activity
                .merged(mr.id, user_id, mr.merge_commit_sha.as_deref())
                .await;
            Ok(Json(MergeAttemptResponse::Merged {
                merge_request: MergeRequestResponse::build(&state, *mr).await?,
            }))
        }
        MergeMergeRequestResult::Conflicting => Ok(Json(MergeAttemptResponse::Conflicting)),
    }
}

async fn close(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(merge_request_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    find_accessible_merge_request(
        &state,
        user_id,
        merge_request_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    let use_case = CloseMergeRequestUseCase::new(
        state.merge_requests.clone(),
        state.repositories.clone(),
        state.users.clone(),
        state.notifications.clone(),
        state.webhooks.clone(),
    );
    use_case.execute(merge_request_id, user_id).await?;
    state
        .merge_request_activity
        .closed(merge_request_id, user_id)
        .await;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/repositories/{repository_id}/branches", get(list_branches))
        .route(
            "/repositories/{repository_id}/merge-requests",
            get(list_for_repository).post(create),
        )
        .route("/merge-requests/{id}", get(detail).patch(update))
        .route(
            "/merge-requests/{id}/labels",
            axum::routing::put(set_labels),
        )
        .route("/merge-requests/{id}/merge", post(merge))
        .route("/merge-requests/{id}/close", post(close))
        .merge(diff::router())
        .merge(comments::router())
        .merge(timeline::router())
        .merge(reviews::router())
}
