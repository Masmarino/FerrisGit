use axum::extract::{Path, State};
use axum::routing::{delete, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::use_cases::create_api_token::CreateApiTokenUseCase;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
struct CreateTokenRequest {
    name: String,
}

#[derive(Serialize)]
struct CreateTokenResponse {
    id: Uuid,
    name: String,
    token: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TokenSummary {
    id: Uuid,
    name: String,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
}

async fn create(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateTokenRequest>,
) -> Result<Json<CreateTokenResponse>, ApiError> {
    let use_case = CreateApiTokenUseCase::new(state.api_tokens.clone());
    let (token, plain) = use_case.execute(user_id, req.name).await?;
    Ok(Json(CreateTokenResponse {
        id: token.id,
        name: token.name,
        token: plain,
    }))
}

async fn list(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<TokenSummary>>, ApiError> {
    let tokens = state.api_tokens.list_for_user(user_id).await?;
    Ok(Json(
        tokens
            .into_iter()
            .map(|t| TokenSummary {
                id: t.id,
                name: t.name,
                created_at: t.created_at,
                last_used_at: t.last_used_at,
            })
            .collect(),
    ))
}

async fn revoke(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<(), ApiError> {
    state.api_tokens.revoke(id, user_id).await?;
    Ok(())
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tokens", post(create).get(list))
        .route("/tokens/{id}", delete(revoke))
}
