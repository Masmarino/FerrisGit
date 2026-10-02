use std::collections::HashMap;
use std::path::PathBuf;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::use_cases::cancel_pipeline::CancelPipelineUseCase;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::pipeline::Pipeline;
use ferrisgit_domain::repository::Repository;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use serde::Serialize;
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::authz::require_role_by_id;
use crate::error::ApiError;
use crate::routes::user_ref::{UserRef, load_user_refs};
use crate::state::AppState;

/// Commit messages are resolved from git for at most this many (the newest) pipelines: each one opens the
/// repository, so an unbounded list would make the request cost grow with history. Older ones get `null`.
const COMMIT_MESSAGE_LIST_LIMIT: usize = 50;

async fn find_accessible_pipeline(
    state: &AppState,
    user_id: Uuid,
    pipeline_id: Uuid,
    min_role: CollaboratorRole,
) -> Result<(Pipeline, Repository), DomainError> {
    let pipeline = state
        .pipelines
        .find_by_id(pipeline_id)
        .await?
        .ok_or_else(|| DomainError::NotFound("pipeline".to_string()))?;
    let repo = require_role_by_id(state, user_id, pipeline.repository_id, min_role).await?;
    Ok((pipeline, repo))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PipelineResponse {
    id: Uuid,
    commit_sha: String,
    status: String,
    created_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    triggered_by: Option<UserRef>,
    commit_message: Option<String>,
    /// Why the pipeline file could not be used. Only set on a failed pipeline without jobs.
    error: Option<String>,
}

impl PipelineResponse {
    /// Every handler returning pipelines goes through here so they all carry the same fields. A commit that cannot
    /// be resolved never fails the request.
    async fn build_many(
        state: &AppState,
        disk_path: &str,
        pipelines: Vec<Pipeline>,
        resolve_limit: usize,
    ) -> Vec<Self> {
        let users = load_user_refs(state, pipelines.iter().map(|p| p.triggered_by)).await;

        let shas: Vec<String> = pipelines
            .iter()
            .take(resolve_limit)
            .map(|p| p.commit_sha.clone())
            .collect();
        let full_path = PathBuf::from(&state.config.storage_root).join(disk_path);
        let git_reader = state.git_reader.clone();
        let messages: HashMap<String, String> = tokio::task::spawn_blocking(move || {
            let mut messages = HashMap::new();
            for sha in shas {
                if messages.contains_key(&sha) {
                    continue;
                }
                if let Ok(commits) = git_reader.list_commits_at(&full_path, &sha, 1)
                    && let Some(commit) = commits.into_iter().next()
                {
                    messages.insert(sha, commit.message);
                }
            }
            messages
        })
        .await
        .unwrap_or_default();

        pipelines
            .into_iter()
            .enumerate()
            .map(|(index, p)| {
                let commit_message = if index < resolve_limit {
                    messages.get(&p.commit_sha).cloned()
                } else {
                    None
                };
                PipelineResponse {
                    id: p.id,
                    triggered_by: users.get(&p.triggered_by).cloned(),
                    commit_message,
                    commit_sha: p.commit_sha,
                    status: p.status.as_str().to_string(),
                    created_at: p.created_at,
                    finished_at: p.finished_at,
                    error: p.error,
                }
            })
            .collect()
    }

    async fn build(state: &AppState, disk_path: &str, pipeline: Pipeline) -> Self {
        Self::build_many(state, disk_path, vec![pipeline], 1)
            .await
            .remove(0)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JobResponse {
    id: Uuid,
    stage: String,
    name: String,
    status: String,
    needs: Vec<String>,
    tags: Vec<String>,
    logs: String,
    /// Set when the log retention emptied this job's log, so the interface can tell it from a log that was never written.
    logs_purged_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PipelineDetailResponse {
    #[serde(flatten)]
    pipeline: PipelineResponse,
    jobs: Vec<JobResponse>,
}

async fn list_for_repository(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
) -> Result<Json<Vec<PipelineResponse>>, ApiError> {
    let repo = require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Reader).await?;
    let pipelines = state.pipelines.list_for_repository(repo.id).await?;
    Ok(Json(
        PipelineResponse::build_many(
            &state,
            &repo.disk_path,
            pipelines,
            COMMIT_MESSAGE_LIST_LIMIT,
        )
        .await,
    ))
}

async fn detail(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
) -> Result<Json<PipelineDetailResponse>, ApiError> {
    let (pipeline, repo) =
        find_accessible_pipeline(&state, user_id, pipeline_id, CollaboratorRole::Reader).await?;
    let jobs = state.jobs.list_for_pipeline(pipeline.id).await?;
    let logs_purged_at = state.job_log_retention.logs_purged_at(pipeline.id).await?;
    Ok(Json(PipelineDetailResponse {
        pipeline: PipelineResponse::build(&state, &repo.disk_path, pipeline).await,
        jobs: jobs
            .into_iter()
            .map(|j| JobResponse {
                logs_purged_at: logs_purged_at.get(&j.id).copied(),
                id: j.id,
                stage: j.stage,
                name: j.name,
                status: j.status.as_str().to_string(),
                needs: j.needs,
                tags: j.tags,
                logs: j.logs,
                created_at: j.created_at,
                started_at: j.started_at,
                finished_at: j.finished_at,
            })
            .collect(),
    }))
}

async fn cancel(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    find_accessible_pipeline(&state, user_id, pipeline_id, CollaboratorRole::Contributor).await?;
    let use_case = CancelPipelineUseCase::new(
        state.pipelines.clone(),
        state.jobs.clone(),
        state.job_execution.clone(),
        state.pipeline_events.clone(),
    );
    use_case.execute(pipeline_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/repositories/{repository_id}/pipelines",
            get(list_for_repository),
        )
        .route("/pipelines/{id}", get(detail))
        .route("/pipelines/{id}/cancel", post(cancel))
}
