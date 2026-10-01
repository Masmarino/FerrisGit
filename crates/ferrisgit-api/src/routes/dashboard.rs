use std::collections::HashMap;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::use_cases::dashboard::DashboardUseCase;
use serde::Serialize;
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::error::ApiError;
use crate::routes::notifications::NotificationResponse;
use crate::routes::search::{SearchRepositoryRef, repository_ref};
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardIssueResponse {
    id: Uuid,
    number: i32,
    title: String,
    status: String,
    kind: String,
    created_at: DateTime<Utc>,
    repository: SearchRepositoryRef,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardMergeRequestResponse {
    id: Uuid,
    title: String,
    status: String,
    source_branch: String,
    target_branch: String,
    created_at: DateTime<Utc>,
    repository: SearchRepositoryRef,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardResponse {
    assigned_issues: Vec<DashboardIssueResponse>,
    authored_issues: Vec<DashboardIssueResponse>,
    authored_merge_requests: Vec<DashboardMergeRequestResponse>,
    merge_requests_to_review: Vec<DashboardMergeRequestResponse>,
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

    let mut repo_cache: HashMap<Uuid, SearchRepositoryRef> = HashMap::new();

    let mut assigned_issues = Vec::with_capacity(results.assigned_issues.len());
    for issue in results.assigned_issues {
        let repository = repository_ref(&state, issue.repository_id, &mut repo_cache).await?;
        assigned_issues.push(DashboardIssueResponse {
            id: issue.id,
            number: issue.number,
            title: issue.title,
            status: issue.status.as_str().to_string(),
            kind: issue.kind.as_str().to_string(),
            created_at: issue.created_at,
            repository,
        });
    }

    let mut authored_issues = Vec::with_capacity(results.authored_issues.len());
    for issue in results.authored_issues {
        let repository = repository_ref(&state, issue.repository_id, &mut repo_cache).await?;
        authored_issues.push(DashboardIssueResponse {
            id: issue.id,
            number: issue.number,
            title: issue.title,
            status: issue.status.as_str().to_string(),
            kind: issue.kind.as_str().to_string(),
            created_at: issue.created_at,
            repository,
        });
    }

    let mut authored_merge_requests = Vec::with_capacity(results.authored_merge_requests.len());
    for mr in results.authored_merge_requests {
        let repository = repository_ref(&state, mr.repository_id, &mut repo_cache).await?;
        authored_merge_requests.push(DashboardMergeRequestResponse {
            id: mr.id,
            title: mr.title,
            status: mr.status.as_str().to_string(),
            source_branch: mr.source_branch,
            target_branch: mr.target_branch,
            created_at: mr.created_at,
            repository,
        });
    }

    let mut merge_requests_to_review = Vec::with_capacity(results.merge_requests_to_review.len());
    for mr in results.merge_requests_to_review {
        let repository = repository_ref(&state, mr.repository_id, &mut repo_cache).await?;
        merge_requests_to_review.push(DashboardMergeRequestResponse {
            id: mr.id,
            title: mr.title,
            status: mr.status.as_str().to_string(),
            source_branch: mr.source_branch,
            target_branch: mr.target_branch,
            created_at: mr.created_at,
            repository,
        });
    }

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
