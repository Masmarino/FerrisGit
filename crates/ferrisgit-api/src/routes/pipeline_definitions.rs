use std::collections::BTreeMap;

use axum::extract::{DefaultBodyLimit, Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use ferrisgit_application::use_cases::pipeline_definition_builder::{
    ReadPipelineDefinitionUseCase, RenderPipelineDefinitionUseCase,
};
use ferrisgit_application::use_cases::pipeline_definition_proposal::{
    PipelineProposal, ProposePipelineDefinitionUseCase, ReadRepositoryPipelineFileUseCase,
};
use ferrisgit_application::use_cases::repository_profile::DetectRepositoryProfileUseCase;
use ferrisgit_domain::pipeline_definition::{
    JobDefinition, PipelineDefinition, PipelineDefinitionError, PipelineDefinitionWarning,
};
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use ferrisgit_domain::repository_profile::RepositoryProfile;
use serde::{Deserialize, Serialize};

use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::authz::require_role_by_id;
use crate::error::ApiError;
use crate::state::AppState;

/// A pipeline file is a few KiB; this leaves room for a large one and nothing more.
const BODY_LIMIT_BYTES: usize = 256 * 1024;

/// A job as the builder holds it. Every field but the stage may be missing or empty, because a draft is not finished:
/// what is wrong with it is the answer, not a refusal.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct JobDto {
    stage: String,
    image: String,
    script: Vec<String>,
    variables: BTreeMap<String, String>,
    needs: Vec<String>,
    tags: Vec<String>,
    cache: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct DefinitionDto {
    stages: Vec<String>,
    jobs: BTreeMap<String, JobDto>,
}

impl From<PipelineDefinition> for DefinitionDto {
    fn from(definition: PipelineDefinition) -> Self {
        Self {
            stages: definition.stages,
            jobs: definition
                .jobs
                .into_iter()
                .map(|(name, job)| {
                    (
                        name,
                        JobDto {
                            stage: job.stage,
                            image: job.image,
                            script: job.script,
                            variables: job.variables,
                            needs: job.needs,
                            tags: job.tags,
                            cache: job.cache,
                        },
                    )
                })
                .collect(),
        }
    }
}

impl From<DefinitionDto> for PipelineDefinition {
    fn from(dto: DefinitionDto) -> Self {
        Self {
            stages: dto.stages,
            jobs: dto
                .jobs
                .into_iter()
                .map(|(name, job)| {
                    (
                        name,
                        JobDefinition {
                            stage: job.stage,
                            image: job.image,
                            script: job.script,
                            variables: job.variables,
                            needs: job.needs,
                            tags: job.tags,
                            cache: job.cache,
                        },
                    )
                })
                .collect(),
        }
    }
}

/// A stable `code` for the interface to translate, the English `message` as the parser words it, and the names it is
/// about.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProblemDto {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    job: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dependency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    jobs: Option<Vec<String>>,
}

impl ProblemDto {
    fn new(code: &'static str, error: &PipelineDefinitionError) -> Self {
        Self {
            code,
            message: error.to_string(),
            job: None,
            stage: None,
            dependency: None,
            key: None,
            jobs: None,
        }
    }
}

impl From<&PipelineDefinitionError> for ProblemDto {
    fn from(error: &PipelineDefinitionError) -> Self {
        match error {
            PipelineDefinitionError::InvalidYaml(_) => Self::new("invalid_yaml", error),
            PipelineDefinitionError::UnknownStage { job, stage } => Self {
                job: Some(job.clone()),
                stage: Some(stage.clone()),
                ..Self::new("unknown_stage", error)
            },
            PipelineDefinitionError::UnknownDependency { job, dependency } => Self {
                job: Some(job.clone()),
                dependency: Some(dependency.clone()),
                ..Self::new("unknown_dependency", error)
            },
            PipelineDefinitionError::NeedsMustPrecedeOwnStage { job, dependency } => Self {
                job: Some(job.clone()),
                dependency: Some(dependency.clone()),
                ..Self::new("needs_must_precede_own_stage", error)
            },
            PipelineDefinitionError::NeedsCycle { jobs } => Self {
                jobs: Some(jobs.clone()),
                ..Self::new("needs_cycle", error)
            },
            PipelineDefinitionError::InvalidCacheKey { job, key } => Self {
                job: Some(job.clone()),
                key: Some(key.clone()),
                ..Self::new("invalid_cache_key", error)
            },
        }
    }
}

#[derive(Serialize)]
struct WarningDto {
    code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    job: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stage: Option<String>,
}

impl From<&PipelineDefinitionWarning> for WarningDto {
    fn from(warning: &PipelineDefinitionWarning) -> Self {
        let (code, job, stage) = match warning {
            PipelineDefinitionWarning::EmptyImage { job } => ("empty_image", Some(job), None),
            PipelineDefinitionWarning::EmptyScript { job } => ("empty_script", Some(job), None),
            PipelineDefinitionWarning::DuplicateStage { stage } => {
                ("duplicate_stage", None, Some(stage))
            }
        };
        Self {
            code,
            job: job.cloned(),
            stage: stage.cloned(),
        }
    }
}

#[derive(Deserialize)]
struct ParseRequest {
    yaml: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ParseResponse {
    definition: Option<DefinitionDto>,
    problems: Vec<ProblemDto>,
    warnings: Vec<WarningDto>,
    ignored_fields: Vec<String>,
    has_comments: bool,
}

#[derive(Deserialize)]
struct RenderRequest {
    definition: DefinitionDto,
}

#[derive(Serialize)]
struct RenderResponse {
    yaml: String,
    problems: Vec<ProblemDto>,
    warnings: Vec<WarningDto>,
}

/// Reads a pipeline file with the server's own parser. Anyone signed in can ask: it touches no repository.
async fn parse(
    AuthUser(_user_id): AuthUser,
    Json(req): Json<ParseRequest>,
) -> Result<Json<ParseResponse>, ApiError> {
    let report = ReadPipelineDefinitionUseCase.execute(&req.yaml);
    Ok(Json(ParseResponse {
        definition: report.definition.map(DefinitionDto::from),
        problems: report.problems.iter().map(ProblemDto::from).collect(),
        warnings: report.warnings.iter().map(WarningDto::from).collect(),
        ignored_fields: report.ignored_fields,
        has_comments: report.has_comments,
    }))
}

/// Writes a definition as a pipeline file and checks it with the same rules as the parser.
async fn render(
    AuthUser(_user_id): AuthUser,
    Json(req): Json<RenderRequest>,
) -> Result<Json<RenderResponse>, ApiError> {
    let definition = PipelineDefinition::from(req.definition);
    let rendered = RenderPipelineDefinitionUseCase.execute(&definition)?;
    Ok(Json(RenderResponse {
        yaml: rendered.yaml,
        problems: rendered.problems.iter().map(ProblemDto::from).collect(),
        warnings: rendered.warnings.iter().map(WarningDto::from).collect(),
    }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryFileResponse {
    path: String,
    branch: Option<String>,
    base_sha: Option<String>,
    yaml: Option<String>,
}

/// The pipeline file as the default branch has it, from where the repository keeps it, for the editor to start from.
async fn repository_file(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
) -> Result<Json<RepositoryFileResponse>, ApiError> {
    let repo = require_role_by_id(
        &state,
        user_id,
        repository_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    let file = ReadRepositoryPipelineFileUseCase::new(
        state.repository_settings.clone(),
        state.branch_reader.clone(),
        state.pipeline_file_reader.clone(),
    )
    .execute(repo.id, &repo.disk_path)
    .await?;
    Ok(Json(RepositoryFileResponse {
        path: file.path,
        branch: file.branch,
        base_sha: file.base_sha,
        yaml: file.yaml,
    }))
}

/// What the default branch is made of (its projects, their versions and tools, its Dockerfiles and charts), for the
/// editor to propose a pipeline that fits the repository.
async fn repository_profile(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
) -> Result<Json<RepositoryProfile>, ApiError> {
    let repo = require_role_by_id(
        &state,
        user_id,
        repository_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    let profile = DetectRepositoryProfileUseCase::new(
        state.branch_reader.clone(),
        state.repository_file_lister.clone(),
        state.pipeline_file_reader.clone(),
    )
    .execute(&repo.disk_path)
    .await?;
    Ok(Json(profile))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProposalRequest {
    yaml: String,
    base_sha: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProposalResponse {
    branch: String,
    commit_sha: String,
    merge_request_id: Uuid,
}

/// Saves the file on a new branch and opens a merge request for it: the default branch is never written to directly.
async fn propose(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
    Json(req): Json<ProposalRequest>,
) -> Result<Json<ProposalResponse>, ApiError> {
    let repo = require_role_by_id(
        &state,
        user_id,
        repository_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    let proposed = ProposePipelineDefinitionUseCase::new(
        state.repository_settings.clone(),
        state.users.clone(),
        state.branch_reader.clone(),
        state.pipeline_file_reader.clone(),
        state.branch_file_writer.clone(),
        state.merge_requests.clone(),
    )
    .execute(PipelineProposal {
        repository_id: repo.id,
        repository_disk_path: repo.disk_path.clone(),
        author_id: user_id,
        yaml: req.yaml,
        base_sha: req.base_sha,
        title: req.title,
        description: req.description,
    })
    .await?;
    // Best effort, as for any merge request: remember the branch tip so a later push is journaled as new commits.
    state
        .merge_request_events
        .set_head_sha(proposed.merge_request.id, &proposed.commit_sha)
        .await
        .ok();
    Ok(Json(ProposalResponse {
        branch: proposed.branch,
        commit_sha: proposed.commit_sha,
        merge_request_id: proposed.merge_request.id,
    }))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/pipeline-definitions/parse", post(parse))
        .route("/pipeline-definitions/render", post(render))
        .route(
            "/repositories/{repository_id}/pipeline-definition",
            get(repository_file),
        )
        .route(
            "/repositories/{repository_id}/pipeline-definition/proposal",
            post(propose),
        )
        .route(
            "/repositories/{repository_id}/pipeline-definition/profile",
            get(repository_profile),
        )
        .layer(DefaultBodyLimit::max(BODY_LIMIT_BYTES))
}
