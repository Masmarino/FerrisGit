use axum::extract::{FromRequestParts, MatchedPath};
use axum::http::header;
use axum::http::request::Parts;
use chrono::Utc;
use ferrisgit_application::token_hash::hash_token;
use ferrisgit_domain::access_token::{ACCESS_TOKEN_PREFIX, AccessToken, ScopeArea};
use ferrisgit_domain::error::DomainError;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use crate::token_scopes::{TokenAccess, required_access};

pub struct AuthUser(pub Uuid);

/// `token` is empty for a session JWT, which isn't limited by scopes.
struct Authenticated {
    user_id: Uuid,
    token: Option<AccessToken>,
}

async fn authenticate(parts: &Parts, state: &AppState) -> Result<Authenticated, ApiError> {
    let header_value = parts
        .headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            ApiError(DomainError::Unauthorized(
                "missing Authorization header".to_string(),
            ))
        })?;

    let token = header_value.strip_prefix("Bearer ").ok_or_else(|| {
        ApiError(DomainError::Unauthorized(
            "expected Bearer token".to_string(),
        ))
    })?;

    if token.starts_with(ACCESS_TOKEN_PREFIX) {
        return authenticate_access_token(parts, state, token).await;
    }

    let (user_id, token_epoch) = state.token_issuer.verify(token)?;
    // A password change bumps the epoch, which is what kills older JWTs that haven't expired yet.
    // A deleted account's tokens get a 401 ("signed out"), not the store's 404.
    let current_epoch = match state.users.get_token_epoch(user_id).await {
        Err(DomainError::NotFound(_)) => {
            return Err(ApiError(DomainError::Unauthorized(
                "user no longer exists".to_string(),
            )));
        }
        other => other?,
    };
    if token_epoch != current_epoch {
        return Err(ApiError(DomainError::Unauthorized(
            "token has been revoked".to_string(),
        )));
    }
    Ok(Authenticated {
        user_id,
        token: None,
    })
}

/// Handlers then apply the owner's role as they do for a session, so a token reaches the intersection of its scopes and
/// its owner's rights. No matched route means nothing can say what it needs, so it is refused.
async fn authenticate_access_token(
    parts: &Parts,
    state: &AppState,
    plain: &str,
) -> Result<Authenticated, ApiError> {
    let invalid = || {
        ApiError(DomainError::Unauthorized(
            "invalid or expired token".to_string(),
        ))
    };
    let token = state
        .access_tokens
        .find_by_hash(&hash_token(plain))
        .await?
        .filter(|token| !token.is_expired(Utc::now()))
        .ok_or_else(invalid)?;

    let access = parts
        .extensions
        .get::<MatchedPath>()
        .map_or(TokenAccess::SessionOnly, |matched| {
            required_access(&parts.method, matched.as_str())
        });
    match access {
        TokenAccess::AnyToken => {}
        TokenAccess::Requires(scopes) => {
            if let Some(missing) = scopes.iter().find(|scope| !token.allows(**scope)) {
                return Err(ApiError(DomainError::Forbidden(format!(
                    "this token lacks the scope {missing}"
                ))));
            }
        }
        TokenAccess::SessionOnly => {
            return Err(ApiError(DomainError::Forbidden(
                "this route needs a signed-in session, an API token can't use it".to_string(),
            )));
        }
    }

    state.access_tokens.touch_last_used(token.id).await.ok();
    Ok(Authenticated {
        user_id: token.user_id,
        token: Some(token),
    })
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(AuthUser(authenticate(parts, state).await?.user_id))
    }
}

pub struct AdminUser(pub Uuid);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Authenticated { user_id, token } = authenticate(parts, state).await?;
        let user = state.users.find_by_id(user_id).await?.ok_or_else(|| {
            ApiError(DomainError::Unauthorized(
                "user no longer exists".to_string(),
            ))
        })?;
        if let Some(token) = token {
            // Holds whatever route this extractor ends up on, not only those under /admin.
            if !user.is_admin || !token.has_area(ScopeArea::Admin) {
                return Err(ApiError(DomainError::Forbidden(
                    "this token doesn't give admin access".to_string(),
                )));
            }
        }
        if !user.is_admin {
            return Err(ApiError(DomainError::Unauthorized(
                "admin access required".to_string(),
            )));
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
                ApiError(DomainError::Unauthorized(
                    "missing Authorization header".to_string(),
                ))
            })?;
        let plain_token = header_value.strip_prefix("Bearer ").ok_or_else(|| {
            ApiError(DomainError::Unauthorized(
                "expected Bearer token".to_string(),
            ))
        })?;

        let runner = state
            .runners
            .find_by_token_hash(&hash_token(plain_token))
            .await?
            .ok_or_else(|| {
                ApiError(DomainError::Unauthorized(
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
