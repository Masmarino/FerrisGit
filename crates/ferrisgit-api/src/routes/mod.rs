use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use ferrisgit_domain::health::{ComponentHealth, HealthCheckPort};

use crate::state::AppState;

pub mod api_tokens;
pub mod auth;
pub mod collaborators;
pub mod dashboard;
pub mod git_http;
pub mod groups;
pub mod issues;
pub mod labels;
pub mod merge_requests;
pub mod metrics;
pub mod mfa;
pub mod milestones;
pub mod notifications;
pub mod pipelines;
pub mod public;
pub mod releases;
pub mod repositories;
pub mod resolve;
pub mod runner_jobs;
pub mod runners;
pub mod search;
pub mod settings;
pub mod smtp_settings;
pub mod summaries;
pub mod user_ref;
pub mod users;
pub mod webhooks;
pub mod wikis;

/// Liveness: the process answers. Never touches the database, so a database outage does not get the pod restarted.
/// Served on `/health` and `/healthz`.
pub async fn health() -> &'static str {
    "ok"
}

/// How long `/readyz` waits for the database before it answers 503: a probe must not hang along with it.
pub const READINESS_TIMEOUT: Duration = Duration::from_secs(2);

/// True when the database answers its trivial query within `timeout`. The check is the one behind Admin > Health.
pub async fn database_answers(check: &dyn HealthCheckPort, timeout: Duration) -> bool {
    tokio::time::timeout(timeout, check.check())
        .await
        .is_ok_and(|health| health.status == ComponentHealth::Up)
}

/// Readiness: the database answers. The 503 carries no detail (the reason is for the Admin > Health page).
pub async fn ready(State(state): State<AppState>) -> (StatusCode, &'static str) {
    if database_answers(state.health_check.as_ref(), READINESS_TIMEOUT).await {
        (StatusCode::OK, "ok")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "unavailable")
    }
}

/// For `Option<Option<T>>` fields with `#[serde(default)]`: absent is `None`, `null` is `Some(None)`, a value is
/// `Some(Some(v))`. Plain serde collapses absent and `null`, so an omitted field in a full-replace PATCH would
/// silently clear stored data.
pub fn deserialize_present<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    T::deserialize(deserializer).map(Some)
}

pub fn api_router(state: AppState) -> Router<AppState> {
    Router::new()
        .merge(public::router(state))
        .merge(auth::router())
        .merge(mfa::router())
        .merge(api_tokens::router())
        .merge(repositories::router())
        .merge(collaborators::router())
        .merge(dashboard::router())
        .merge(groups::router())
        .merge(resolve::router())
        .merge(runners::router())
        .merge(runner_jobs::router())
        .merge(pipelines::router())
        .merge(releases::router())
        .merge(merge_requests::router())
        .merge(metrics::router())
        .merge(issues::router())
        .merge(labels::router())
        .merge(milestones::router())
        .merge(notifications::router())
        .merge(settings::router())
        .merge(smtp_settings::router())
        .merge(users::router())
        .merge(search::router())
        .merge(webhooks::router())
        .merge(wikis::router())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrisgit_domain::health::DatabaseHealth;

    struct FakeCheck {
        answer: ComponentHealth,
        delay: Duration,
    }

    #[async_trait::async_trait]
    impl HealthCheckPort for FakeCheck {
        async fn check(&self) -> DatabaseHealth {
            tokio::time::sleep(self.delay).await;
            DatabaseHealth {
                status: self.answer.clone(),
                response_time_ms: 0,
                active_connections: 0,
                max_connections: 10,
                server_version: None,
            }
        }
    }

    fn check(answer: ComponentHealth, delay_ms: u64) -> FakeCheck {
        FakeCheck {
            answer,
            delay: Duration::from_millis(delay_ms),
        }
    }

    #[tokio::test]
    async fn a_database_that_answers_up_in_time_is_ready() {
        let up = check(ComponentHealth::Up, 0);
        assert!(database_answers(&up, Duration::from_secs(1)).await);
    }

    #[tokio::test]
    async fn a_database_reporting_down_is_not_ready() {
        let down = check(ComponentHealth::Down("connection refused".into()), 0);
        assert!(!database_answers(&down, Duration::from_secs(1)).await);
    }

    #[tokio::test]
    async fn a_database_slower_than_the_timeout_is_not_ready() {
        let slow = check(ComponentHealth::Up, 500);
        assert!(!database_answers(&slow, Duration::from_millis(50)).await);
    }
}
