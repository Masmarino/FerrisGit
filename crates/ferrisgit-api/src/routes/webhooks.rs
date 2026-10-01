use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::use_cases::create_webhook::CreateWebhookUseCase;
use ferrisgit_application::use_cases::update_webhook::{
    DeleteWebhookUseCase, UpdateWebhookUseCase,
};
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use ferrisgit_domain::webhook::WebhookUpdate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::authz::require_role_by_id;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WebhookResponse {
    id: Uuid,
    url: String,
    events: Vec<String>,
    active: bool,
    created_at: DateTime<Utc>,
}

impl From<ferrisgit_domain::webhook::Webhook> for WebhookResponse {
    fn from(w: ferrisgit_domain::webhook::Webhook) -> Self {
        Self {
            id: w.id,
            url: w.url,
            events: w.events,
            active: w.active,
            created_at: w.created_at,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WebhookDeliveryResponse {
    id: Uuid,
    event_kind: String,
    http_status: Option<i32>,
    success: bool,
    error_message: Option<String>,
    created_at: DateTime<Utc>,
}

impl From<ferrisgit_domain::webhook::WebhookDelivery> for WebhookDeliveryResponse {
    fn from(d: ferrisgit_domain::webhook::WebhookDelivery) -> Self {
        Self {
            id: d.id,
            event_kind: d.event_kind,
            http_status: d.http_status,
            success: d.success,
            error_message: d.error_message,
            created_at: d.created_at,
        }
    }
}

#[derive(Deserialize)]
struct CreateWebhookRequest {
    url: String,
    secret: String,
    events: Vec<String>,
}

#[derive(Deserialize)]
struct UpdateWebhookRequest {
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    secret: Option<String>,
    #[serde(default)]
    events: Option<Vec<String>>,
    #[serde(default)]
    active: Option<bool>,
}

async fn list(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
) -> Result<Json<Vec<WebhookResponse>>, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    let webhooks = state
        .webhook_store
        .list_for_repository(repository_id)
        .await?;
    Ok(Json(webhooks.into_iter().map(Into::into).collect()))
}

async fn create(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
    Json(req): Json<CreateWebhookRequest>,
) -> Result<Json<WebhookResponse>, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    let use_case = CreateWebhookUseCase::new(state.webhook_store.clone());
    let webhook = use_case
        .execute(repository_id, req.url, req.secret, req.events)
        .await?;
    Ok(Json(webhook.into()))
}

async fn update(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateWebhookRequest>,
) -> Result<Json<WebhookResponse>, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    let use_case = UpdateWebhookUseCase::new(state.webhook_store.clone());
    let webhook = use_case
        .execute(
            id,
            repository_id,
            WebhookUpdate {
                url: req.url,
                secret_plaintext: req.secret,
                events: req.events,
                active: req.active,
            },
        )
        .await?;
    Ok(Json(webhook.into()))
}

async fn delete(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    let use_case = DeleteWebhookUseCase::new(state.webhook_store.clone());
    use_case.execute(id, repository_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn deliveries(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<WebhookDeliveryResponse>>, ApiError> {
    require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Maintainer).await?;
    let deliveries = state
        .webhook_store
        .list_deliveries(id, repository_id, 20)
        .await?;
    Ok(Json(deliveries.into_iter().map(Into::into).collect()))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/repositories/{repository_id}/webhooks",
            get(list).post(create),
        )
        .route(
            "/repositories/{repository_id}/webhooks/{id}",
            axum::routing::patch(update).delete(delete),
        )
        .route(
            "/repositories/{repository_id}/webhooks/{id}/deliveries",
            get(deliveries),
        )
}
