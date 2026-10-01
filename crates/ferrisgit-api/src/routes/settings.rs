use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get};
use axum::{Json, Router};
use ferrisgit_application::use_cases::update_system_settings::UpdateSystemSettingsUseCase;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::public_pages::{PublicPagesSettings, PublicPagesSettingsUpdate};
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use ferrisgit_domain::settings::{
    CiVariable, ExecutionEngine, NewCiVariable, RepositorySettings, RepositorySettingsUpdate,
    SystemSettings, SystemSettingsUpdate,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::{AdminUser, AuthUser};
use crate::authz::require_role_by_id;
use crate::error::ApiError;
use crate::state::AppState;

fn deserialize_some<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemSettingsResponse {
    execution_engine: String,
    k8s_namespace: Option<String>,
    k8s_cache_storage_class: Option<String>,
    // Only says whether a token is configured. It is stored hashed and never round-trips.
    runner_registration_token_configured: bool,
    log_retention_days: Option<i32>,
    max_concurrent_jobs: Option<i32>,
    jwt_ttl_hours: i32,
    max_push_size_mb: i32,
    registration_enabled: bool,
    public_pages_enabled: bool,
    seo_indexing_enabled: bool,
    // Not persisted: detected once at boot so the admin UI can pre-fill the k8s settings.
    detected_k8s_namespace: Option<String>,
    detected_k8s_default_storage_class: Option<String>,
}

impl SystemSettingsResponse {
    fn from_settings(
        s: SystemSettings,
        registration_enabled: bool,
        public_pages: PublicPagesSettings,
        state: &AppState,
    ) -> Self {
        Self {
            execution_engine: s.execution_engine.as_str().to_string(),
            k8s_namespace: s.k8s_namespace,
            k8s_cache_storage_class: s.k8s_cache_storage_class,
            runner_registration_token_configured: s.runner_registration_token.is_some(),
            log_retention_days: s.log_retention_days,
            max_concurrent_jobs: s.max_concurrent_jobs,
            jwt_ttl_hours: s.jwt_ttl_hours,
            max_push_size_mb: s.max_push_size_mb,
            registration_enabled,
            public_pages_enabled: public_pages.public_pages_enabled,
            seo_indexing_enabled: public_pages.seo_indexing_enabled,
            detected_k8s_namespace: state.detected_k8s_namespace.clone(),
            detected_k8s_default_storage_class: state.detected_k8s_default_storage_class.clone(),
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct SystemSettingsUpdateRequest {
    #[serde(default)]
    execution_engine: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    k8s_namespace: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    k8s_cache_storage_class: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    runner_registration_token: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    log_retention_days: Option<Option<i32>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    max_concurrent_jobs: Option<Option<i32>>,
    #[serde(default)]
    jwt_ttl_hours: Option<i32>,
    #[serde(default)]
    max_push_size_mb: Option<i32>,
    #[serde(default)]
    registration_enabled: Option<bool>,
    #[serde(default)]
    public_pages_enabled: Option<bool>,
    #[serde(default)]
    seo_indexing_enabled: Option<bool>,
}

impl SystemSettingsUpdateRequest {
    fn into_domain(self) -> Result<SystemSettingsUpdate, DomainError> {
        Ok(SystemSettingsUpdate {
            execution_engine: self
                .execution_engine
                .map(|s| ExecutionEngine::parse(&s))
                .transpose()?,
            k8s_namespace: self.k8s_namespace,
            k8s_cache_storage_class: self.k8s_cache_storage_class,
            runner_registration_token: self.runner_registration_token,
            log_retention_days: self.log_retention_days,
            max_concurrent_jobs: self.max_concurrent_jobs,
            jwt_ttl_hours: self.jwt_ttl_hours,
            max_push_size_mb: self.max_push_size_mb,
        })
    }
}

async fn get_admin_settings(
    AdminUser(_admin_id): AdminUser,
    State(state): State<AppState>,
) -> Result<Json<SystemSettingsResponse>, ApiError> {
    let settings = state.system_settings.get().await?;
    let registration_enabled = state.registration_settings.is_enabled().await?;
    let public_pages = state.public_pages_settings.get().await?;
    Ok(Json(SystemSettingsResponse::from_settings(
        settings,
        registration_enabled,
        public_pages,
        &state,
    )))
}

async fn update_admin_settings(
    AdminUser(_admin_id): AdminUser,
    State(state): State<AppState>,
    Json(req): Json<SystemSettingsUpdateRequest>,
) -> Result<Json<SystemSettingsResponse>, ApiError> {
    if let Some(hours) = req.jwt_ttl_hours
        && !(1..=720).contains(&hours)
    {
        return Err(
            DomainError::Validation("jwt_ttl_hours must be between 1 and 720".to_string()).into(),
        );
    }
    let registration_enabled = req.registration_enabled;
    let public_pages_update = PublicPagesSettingsUpdate {
        public_pages_enabled: req.public_pages_enabled,
        seo_indexing_enabled: req.seo_indexing_enabled,
    };
    let update = req.into_domain()?;
    let use_case =
        UpdateSystemSettingsUseCase::new(state.system_settings.clone(), state.token_issuer.clone());
    let settings = use_case.execute(update).await?;
    if let Some(enabled) = registration_enabled {
        state.registration_settings.set_enabled(enabled).await?;
    }
    let registration_enabled = state.registration_settings.is_enabled().await?;
    let public_pages = state
        .public_pages_settings
        .update(public_pages_update)
        .await?;
    Ok(Json(SystemSettingsResponse::from_settings(
        settings,
        registration_enabled,
        public_pages,
        &state,
    )))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PublicSettingsResponse {
    execution_engine: String,
}

async fn get_public_settings(
    AuthUser(_user_id): AuthUser,
    State(state): State<AppState>,
) -> Result<Json<PublicSettingsResponse>, ApiError> {
    let settings = state.system_settings.get().await?;
    Ok(Json(PublicSettingsResponse {
        execution_engine: settings.execution_engine.as_str().to_string(),
    }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RepositorySettingsResponse {
    pipeline_file_path: String,
    ci_enabled: bool,
    required_approvals: i32,
}

impl From<RepositorySettings> for RepositorySettingsResponse {
    fn from(s: RepositorySettings) -> Self {
        Self {
            pipeline_file_path: s.pipeline_file_path,
            ci_enabled: s.ci_enabled,
            required_approvals: s.required_approvals,
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RepositorySettingsUpdateRequest {
    #[serde(default)]
    pipeline_file_path: Option<String>,
    #[serde(default)]
    ci_enabled: Option<bool>,
    #[serde(default)]
    required_approvals: Option<i32>,
}

async fn get_repository_settings(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
) -> Result<Json<RepositorySettingsResponse>, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    Ok(Json(
        state
            .repository_settings
            .get_or_create_default(repository_id)
            .await?
            .into(),
    ))
}

async fn update_repository_settings(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
    Json(req): Json<RepositorySettingsUpdateRequest>,
) -> Result<Json<RepositorySettingsResponse>, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    if let Some(n) = req.required_approvals
        && n < 0
    {
        return Err(
            DomainError::Validation("required_approvals must not be negative".to_string()).into(),
        );
    }
    let update = RepositorySettingsUpdate {
        pipeline_file_path: req.pipeline_file_path,
        ci_enabled: req.ci_enabled,
        required_approvals: req.required_approvals,
    };
    Ok(Json(
        state
            .repository_settings
            .update(repository_id, update)
            .await?
            .into(),
    ))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CiVariableResponse {
    id: Uuid,
    key: String,
    masked: bool,
}

impl From<CiVariable> for CiVariableResponse {
    fn from(v: CiVariable) -> Self {
        Self {
            id: v.id,
            key: v.key,
            masked: v.masked,
        }
    }
}

async fn list_ci_variables(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
) -> Result<Json<Vec<CiVariableResponse>>, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    let vars = state
        .repository_settings
        .list_ci_variables(repository_id)
        .await?;
    Ok(Json(vars.into_iter().map(Into::into).collect()))
}

#[derive(Deserialize)]
struct SetCiVariableRequest {
    key: String,
    value: String,
    #[serde(default = "default_masked")]
    masked: bool,
}

fn default_masked() -> bool {
    true
}

async fn set_ci_variable(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
    Json(req): Json<SetCiVariableRequest>,
) -> Result<Json<CiVariableResponse>, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    let variable = state
        .repository_settings
        .set_ci_variable(NewCiVariable {
            repository_id,
            key: req.key,
            plaintext_value: req.value,
            masked: req.masked,
        })
        .await?;
    Ok(Json(variable.into()))
}

async fn delete_ci_variable(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    state
        .repository_settings
        .delete_ci_variable(id, repository_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/settings",
            get(get_admin_settings).put(update_admin_settings),
        )
        .route("/settings/public", get(get_public_settings))
        .route(
            "/repositories/{repository_id}/settings",
            get(get_repository_settings).put(update_repository_settings),
        )
        .route(
            "/repositories/{repository_id}/ci-variables",
            get(list_ci_variables).post(set_ci_variable),
        )
        .route(
            "/repositories/{repository_id}/ci-variables/{id}",
            delete(delete_ci_variable),
        )
}
