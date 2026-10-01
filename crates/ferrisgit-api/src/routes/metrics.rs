use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::use_cases::get_admin_stats::{AdminStats, GetAdminStatsUseCase};
use ferrisgit_application::use_cases::get_health_status::{GetHealthStatusUseCase, HealthStatus};
use ferrisgit_application::use_cases::get_metrics_history::GetMetricsHistoryUseCase;
use ferrisgit_domain::health::{DatabaseHealth, StorageHealth};
use ferrisgit_domain::metrics_snapshot::MetricsSnapshot;
use serde::{Deserialize, Serialize};

use crate::auth_middleware::AdminUser;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminStatsResponse {
    total_users: i64,
    total_repositories: i64,
    pipelines_last_7_days: i64,
}

impl From<AdminStats> for AdminStatsResponse {
    fn from(s: AdminStats) -> Self {
        Self {
            total_users: s.total_users,
            total_repositories: s.total_repositories,
            pipelines_last_7_days: s.pipelines_last_7_days,
        }
    }
}

async fn get_admin_stats(
    AdminUser(_admin_id): AdminUser,
    State(state): State<AppState>,
) -> Result<Json<AdminStatsResponse>, ApiError> {
    let use_case = GetAdminStatsUseCase::new(
        state.repositories.clone(),
        state.users.clone(),
        state.pipelines.clone(),
    );
    Ok(Json(use_case.execute().await?.into()))
}

#[derive(Deserialize)]
struct MetricsHistoryParams {
    days: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MetricsSnapshotResponse {
    recorded_at: DateTime<Utc>,
    total_users: i64,
    total_repositories: i64,
    total_storage_bytes: i64,
}

impl From<MetricsSnapshot> for MetricsSnapshotResponse {
    fn from(s: MetricsSnapshot) -> Self {
        Self {
            recorded_at: s.recorded_at,
            total_users: s.total_users,
            total_repositories: s.total_repositories,
            total_storage_bytes: s.total_storage_bytes,
        }
    }
}

async fn get_metrics_history(
    AdminUser(_admin_id): AdminUser,
    State(state): State<AppState>,
    Query(params): Query<MetricsHistoryParams>,
) -> Result<Json<Vec<MetricsSnapshotResponse>>, ApiError> {
    let days = params.days.unwrap_or(30).clamp(1, 365);
    let since = Utc::now() - chrono::Duration::days(days);
    let use_case = GetMetricsHistoryUseCase::new(state.metrics_snapshots.clone());
    let snapshots = use_case.execute(since).await?;
    Ok(Json(snapshots.into_iter().map(Into::into).collect()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DatabaseHealthResponse {
    status: &'static str,
    detail: Option<String>,
    response_time_ms: u64,
    active_connections: u32,
    max_connections: u32,
    server_version: Option<String>,
}

impl From<DatabaseHealth> for DatabaseHealthResponse {
    fn from(h: DatabaseHealth) -> Self {
        Self {
            status: h.status.status_str(),
            detail: h.status.detail(),
            response_time_ms: h.response_time_ms,
            active_connections: h.active_connections,
            max_connections: h.max_connections,
            server_version: h.server_version,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StorageHealthResponse {
    status: &'static str,
    detail: Option<String>,
    used_bytes: u64,
    free_bytes: u64,
    total_bytes: u64,
}

impl From<StorageHealth> for StorageHealthResponse {
    fn from(h: StorageHealth) -> Self {
        Self {
            status: h.status.status_str(),
            detail: h.status.detail(),
            used_bytes: h.used_bytes,
            free_bytes: h.free_bytes,
            total_bytes: h.total_bytes,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthResponse {
    database: DatabaseHealthResponse,
    storage: StorageHealthResponse,
    uptime_seconds: u64,
}

impl From<HealthStatus> for HealthResponse {
    fn from(s: HealthStatus) -> Self {
        Self {
            database: s.database.into(),
            storage: s.storage.into(),
            uptime_seconds: s.uptime_seconds,
        }
    }
}

async fn get_health(
    AdminUser(_admin_id): AdminUser,
    State(state): State<AppState>,
) -> Json<HealthResponse> {
    let use_case = GetHealthStatusUseCase::new(
        state.health_check.clone(),
        state.storage_health.clone(),
        state.started_at,
    );
    Json(use_case.execute().await.into())
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/stats", get(get_admin_stats))
        .route("/admin/metrics/history", get(get_metrics_history))
        .route("/admin/health", get(get_health))
}
