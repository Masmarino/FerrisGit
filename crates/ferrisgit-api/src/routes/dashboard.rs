use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use ferrisgit_application::use_cases::dashboard::DashboardUseCase;
use serde::Serialize;

use crate::auth_middleware::AuthUser;
use crate::error::ApiError;
use crate::routes::notifications::NotificationResponse;
use crate::routes::summaries::{
    IssueSummary, MergeRequestSummary, RepositoryRefs, issue_summaries, merge_request_summaries,
};
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardResponse {
    assigned_issues: Vec<IssueSummary>,
    authored_issues: Vec<IssueSummary>,
    authored_merge_requests: Vec<MergeRequestSummary>,
    merge_requests_to_review: Vec<MergeRequestSummary>,
    activity: Vec<NotificationResponse>,
}

async fn dashboard(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
) -> Result<Json<DashboardResponse>, ApiError> {
    let use_case = DashboardUseCase::new(
        state.repositories.clone(),
        state.repository_collaborators.clone(),
        state.groups.clone(),
        state.issues.clone(),
        state.merge_requests.clone(),
        state.notifications.clone(),
    );
    let results = use_case.execute(user_id).await?;

    let mut repo_cache = RepositoryRefs::new();
    let assigned_issues = issue_summaries(&state, results.assigned_issues, &mut repo_cache).await?;
    let authored_issues = issue_summaries(&state, results.authored_issues, &mut repo_cache).await?;
    let authored_merge_requests =
        merge_request_summaries(&state, results.authored_merge_requests, &mut repo_cache).await?;
    let merge_requests_to_review =
        merge_request_summaries(&state, results.merge_requests_to_review, &mut repo_cache).await?;

    let activity = results
        .activity
        .into_iter()
        .map(NotificationResponse::from)
        .collect();

    Ok(Json(DashboardResponse {
        assigned_issues,
        authored_issues,
        authored_merge_requests,
        merge_requests_to_review,
        activity,
    }))
}

pub fn router() -> Router<AppState> {
    Router::new().route("/dashboard", get(dashboard))
}
