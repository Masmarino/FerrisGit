use axum::Router;

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
pub mod user_ref;
pub mod users;
pub mod webhooks;
pub mod wikis;

pub async fn health() -> &'static str {
    "ok"
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
