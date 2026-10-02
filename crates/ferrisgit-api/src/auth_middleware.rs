use axum::extract::FromRequestParts;
use axum::http::header;
use axum::http::request::Parts;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

pub struct AuthUser(pub Uuid);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header_value = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                ApiError(ferrisgit_domain::error::DomainError::Unauthorized(
                    "missing Authorization header".to_string(),
                ))
            })?;

        let token = header_value.strip_prefix("Bearer ").ok_or_else(|| {
            ApiError(ferrisgit_domain::error::DomainError::Unauthorized(
                "expected Bearer token".to_string(),
            ))
        })?;

        let (user_id, token_epoch) = state.token_issuer.verify(token)?;
        // A password change bumps the epoch, which is what kills older JWTs that haven't expired yet.
        // A deleted account's tokens get a 401 ("signed out"), not the store's 404.
        let current_epoch = match state.users.get_token_epoch(user_id).await {
            Err(ferrisgit_domain::error::DomainError::NotFound(_)) => {
                return Err(ApiError(
                    ferrisgit_domain::error::DomainError::Unauthorized(
                        "user no longer exists".to_string(),
                    ),
                ));
            }
            other => other?,
        };
        if token_epoch != current_epoch {
            return Err(ApiError(
                ferrisgit_domain::error::DomainError::Unauthorized(
                    "token has been revoked".to_string(),
                ),
            ));
        }
        Ok(AuthUser(user_id))
    }
}

pub struct AdminUser(pub Uuid);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let AuthUser(user_id) = AuthUser::from_request_parts(parts, state).await?;
        let user = state.users.find_by_id(user_id).await?.ok_or_else(|| {
            ApiError(ferrisgit_domain::error::DomainError::Unauthorized(
                "user no longer exists".to_string(),
            ))
        })?;
        if !user.is_admin {
            return Err(ApiError(
                ferrisgit_domain::error::DomainError::Unauthorized(
                    "admin access required".to_string(),
                ),
            ));
        }
        Ok(AdminUser(user_id))
    }
}

pub struct RunnerAuth {
    pub runner_id: Uuid,
    pub tags: Vec<String>,
}

impl FromRequestParts<AppState> for RunnerAuth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header_value = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                ApiError(ferrisgit_domain::error::DomainError::Unauthorized(
                    "missing Authorization header".to_string(),
                ))
            })?;
        let plain_token = header_value.strip_prefix("Bearer ").ok_or_else(|| {
            ApiError(ferrisgit_domain::error::DomainError::Unauthorized(
                "expected Bearer token".to_string(),
            ))
        })?;

        let runner = state
            .runners
            .find_by_token_hash(&ferrisgit_application::token_hash::hash_token(plain_token))
            .await?
            .ok_or_else(|| {
                ApiError(ferrisgit_domain::error::DomainError::Unauthorized(
                    "invalid runner token".to_string(),
                ))
            })?;
        state.runners.touch_heartbeat(runner.id).await.ok();
        Ok(RunnerAuth {
            runner_id: runner.id,
            tags: runner.tags,
        })
    }
}
