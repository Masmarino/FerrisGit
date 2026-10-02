use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::use_cases::submit_merge_request_review::SubmitMergeRequestReviewUseCase;
use ferrisgit_domain::merge_request::ReviewDecision;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

use super::find_accessible_merge_request;

#[derive(Deserialize)]
struct SubmitReviewRequest {
    decision: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReviewResponse {
    user_id: Uuid,
    username: String,
    decision: String,
    stale: bool,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReviewSummaryResponse {
    reviews: Vec<ReviewResponse>,
    required_approvals: i32,
    live_approval_count: i32,
    blocked: bool,
}

async fn submit_review(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(merge_request_id): Path<Uuid>,
    Json(req): Json<SubmitReviewRequest>,
) -> Result<Json<ReviewResponse>, ApiError> {
    let (mr, repo) = find_accessible_merge_request(
        &state,
        user_id,
        merge_request_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    let decision = ReviewDecision::parse(&req.decision)?;
    let use_case = SubmitMergeRequestReviewUseCase::new(
        state.merge_requests.clone(),
        state.merge_request_reviews.clone(),
        state.branch_reader.clone(),
        state.repositories.clone(),
        state.users.clone(),
        state.notifications.clone(),
        state.webhooks.clone(),
    );
    let review = use_case
        .execute(
            merge_request_id,
            &repo.disk_path,
            &mr.source_branch,
            user_id,
            decision,
        )
        .await?;
    state
        .merge_request_activity
        .review_submitted(merge_request_id, user_id, decision)
        .await;
    Ok(Json(ReviewResponse {
        user_id: review.user_id,
        username: review.username,
        decision: review.decision.as_str().to_string(),
        stale: false,
        created_at: review.created_at,
    }))
}

async fn list_reviews(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(merge_request_id): Path<Uuid>,
) -> Result<Json<ReviewSummaryResponse>, ApiError> {
    let (mr, repo) =
        find_accessible_merge_request(&state, user_id, merge_request_id, CollaboratorRole::Reader)
            .await?;
    let settings = state
        .repository_settings
        .get_or_create_default(repo.id)
        .await?;
    let branches = state.branch_reader.list_branches(&repo.disk_path).await?;
    let tip_sha = branches
        .into_iter()
        .find(|b| b.name == mr.source_branch)
        .map(|b| b.tip_sha);
    let reviews = state
        .merge_request_reviews
        .list_reviews(merge_request_id)
        .await?;

    let review_responses: Vec<ReviewResponse> = reviews
        .into_iter()
        .map(|r| {
            let stale = tip_sha.as_deref() != Some(r.source_sha.as_str());
            ReviewResponse {
                user_id: r.user_id,
                username: r.username,
                decision: r.decision.as_str().to_string(),
                stale,
                created_at: r.created_at,
            }
        })
        .collect();

    let live_approval_count = review_responses
        .iter()
        .filter(|r| !r.stale && r.decision == "approved")
        .count() as i32;
    let blocked_by_changes = review_responses
        .iter()
        .any(|r| !r.stale && r.decision == "changes_requested");
    let blocked = settings.required_approvals > 0
        && (blocked_by_changes || live_approval_count < settings.required_approvals);

    Ok(Json(ReviewSummaryResponse {
        reviews: review_responses,
        required_approvals: settings.required_approvals,
        live_approval_count,
        blocked,
    }))
}

pub(super) fn router() -> Router<AppState> {
    Router::new().route(
        "/merge-requests/{id}/reviews",
        get(list_reviews).post(submit_review),
    )
}
