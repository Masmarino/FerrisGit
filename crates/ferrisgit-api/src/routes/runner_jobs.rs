use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use ferrisgit_application::use_cases::claim_next_job::ClaimNextJobUseCase;
use ferrisgit_application::use_cases::job_environment::JobEnvironmentSource;
use ferrisgit_application::use_cases::report_job_result::{
    AppendJobLogsUseCase, ReportJobResultUseCase,
};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::{Job, JobStatus};
use ferrisgit_domain::job_containers::DOCKER_DAEMON_IMAGE;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::auth_middleware::RunnerAuth;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClaimedJobResponse {
    id: Uuid,
    stage: String,
    name: String,
    image: String,
    script: Vec<String>,
    variables: BTreeMap<String, String>,
    /// What comes before the name in the clone URL: the owner, or the group's path (`acme/team`).
    repository_owner: String,
    repository_name: String,
    commit_sha: String,
    ci_variables: BTreeMap<String, String>,
    // The runner redacts these from every log line.
    masked_values: Vec<String>,
    /// The Docker daemon to start beside the job, when the admin gives jobs Docker. A runner too old to know it ignores
    /// it, and the job runs without.
    #[serde(skip_serializing_if = "Option::is_none")]
    docker_image: Option<&'static str>,
}

async fn claim(
    RunnerAuth { runner_id, tags }: RunnerAuth,
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    let use_case = ClaimNextJobUseCase::new(
        state.jobs.clone(),
        state.pipelines.clone(),
        state.pipeline_events.clone(),
        state.system_settings.clone(),
    );
    let Some(job) = use_case.execute(runner_id, &tags).await? else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };

    let environment = state.job_environments.environment(&job).await?;
    let (repository_owner, repository_name) = environment
        .repository_path
        .rsplit_once('/')
        .map(|(owner, name)| (owner.to_string(), name.to_string()))
        .ok_or_else(|| DomainError::Infrastructure("repository path without owner".to_string()))?;

    Ok(Json(ClaimedJobResponse {
        id: job.id,
        stage: job.stage,
        name: job.name,
        image: job.image,
        script: job.script,
        variables: job.variables,
        repository_owner,
        repository_name,
        commit_sha: environment.commit_sha,
        ci_variables: environment.ci_variables,
        masked_values: environment.masked_values,
        docker_image: environment.docker.then_some(DOCKER_DAEMON_IMAGE),
    })
    .into_response())
}

#[derive(Deserialize)]
struct AppendLogsRequest {
    chunk: String,
}

/// A runner token is instance-wide, so only the runner that claimed a job may append logs or report a result. Any
/// other runner gets the same `NotFound` as for a missing job, so job ids can't be probed.
async fn ensure_job_is_claimed_by(
    state: &AppState,
    runner_id: Uuid,
    job_id: Uuid,
) -> Result<Job, ApiError> {
    let job = state
        .jobs
        .find_by_id(job_id)
        .await?
        .ok_or_else(|| ApiError(DomainError::NotFound("job".to_string())))?;
    if job.runner_id != Some(runner_id) {
        return Err(ApiError(DomainError::NotFound("job".to_string())));
    }
    Ok(job)
}

async fn append_logs(
    RunnerAuth { runner_id, .. }: RunnerAuth,
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
    Json(req): Json<AppendLogsRequest>,
) -> Result<StatusCode, ApiError> {
    ensure_job_is_claimed_by(&state, runner_id, job_id).await?;
    let use_case = AppendJobLogsUseCase::new(state.jobs.clone());
    use_case.execute(job_id, &req.chunk).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum ReportedStatus {
    Success,
    Failed,
    Canceled,
}

impl From<ReportedStatus> for JobStatus {
    fn from(value: ReportedStatus) -> Self {
        match value {
            ReportedStatus::Success => JobStatus::Success,
            ReportedStatus::Failed => JobStatus::Failed,
            ReportedStatus::Canceled => JobStatus::Canceled,
        }
    }
}

#[derive(Deserialize)]
struct ReportResultRequest {
    status: ReportedStatus,
}

async fn report_result(
    RunnerAuth { runner_id, .. }: RunnerAuth,
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
    Json(req): Json<ReportResultRequest>,
) -> Result<StatusCode, ApiError> {
    let job = ensure_job_is_claimed_by(&state, runner_id, job_id).await?;
    let use_case = ReportJobResultUseCase::new(
        state.jobs.clone(),
        state.pipelines.clone(),
        state.pipeline_events.clone(),
        state.job_execution.clone(),
    );
    let notify = use_case.execute(job_id, req.status.into()).await?;
    ferrisgit_application::use_cases::report_job_result::notify_pipeline_failure(
        notify,
        job.pipeline_id,
        &state.pipelines,
        &state.repositories,
        &state.users,
        &state.notifications,
        &state.webhooks,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/runner/jobs/claim", post(claim))
        .route("/runner/jobs/{id}/logs", post(append_logs))
        .route("/runner/jobs/{id}/result", post(report_result))
}
