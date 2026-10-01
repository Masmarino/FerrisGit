use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::use_cases::mark_all_notifications_read::MarkAllNotificationsReadUseCase;
use ferrisgit_application::use_cases::mark_notification_read::MarkNotificationReadUseCase;
use serde::Serialize;
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NotificationResponse {
    id: Uuid,
    kind: String,
    repository_owner: String,
    repository_name: String,
    actor_username: Option<String>,
    merge_request_id: Option<Uuid>,
    merge_request_title: Option<String>,
    pipeline_id: Option<Uuid>,
    commit_sha: Option<String>,
    issue_id: Option<Uuid>,
    issue_number: Option<i32>,
    issue_title: Option<String>,
    role: Option<String>,
    read: bool,
    created_at: DateTime<Utc>,
}

impl From<ferrisgit_domain::notification::Notification> for NotificationResponse {
    fn from(n: ferrisgit_domain::notification::Notification) -> Self {
        Self {
            id: n.id,
            kind: n.kind.as_str().to_string(),
            repository_owner: n.repository_owner,
            repository_name: n.repository_name,
            actor_username: n.actor_username,
            merge_request_id: n.merge_request_id,
            merge_request_title: n.merge_request_title,
            pipeline_id: n.pipeline_id,
            commit_sha: n.commit_sha,
            issue_id: n.issue_id,
            issue_number: n.issue_number,
            issue_title: n.issue_title,
            role: n.role,
            read: n.read_at.is_some(),
            created_at: n.created_at,
        }
    }
}

#[derive(Serialize)]
struct UnreadCountResponse {
    count: i64,
}

async fn list(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<NotificationResponse>>, ApiError> {
    let notifications = state.notifications.list_for_recipient(user_id, 50).await?;
    Ok(Json(notifications.into_iter().map(Into::into).collect()))
}

async fn unread_count(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
) -> Result<Json<UnreadCountResponse>, ApiError> {
    let count = state.notifications.unread_count(user_id).await?;
    Ok(Json(UnreadCountResponse { count }))
}

async fn mark_read(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(notification_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let use_case = MarkNotificationReadUseCase::new(state.notifications.clone());
    use_case.execute(notification_id, user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn mark_all_read(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
) -> Result<StatusCode, ApiError> {
    let use_case = MarkAllNotificationsReadUseCase::new(state.notifications.clone());
    use_case.execute(user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/notifications", get(list))
        .route("/notifications/unread-count", get(unread_count))
        .route("/notifications/{id}/read", post(mark_read))
        .route("/notifications/read-all", post(mark_all_read))
}
