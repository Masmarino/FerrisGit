use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use ferrisgit_domain::error::DomainError;

pub struct ApiError(pub DomainError);

impl From<DomainError> for ApiError {
    fn from(e: DomainError) -> Self {
        ApiError(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self.0 {
            DomainError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            DomainError::Conflict(m) => (StatusCode::CONFLICT, m.clone()),
            DomainError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m.clone()),
            DomainError::Validation(m) => (StatusCode::BAD_REQUEST, m.clone()),
            DomainError::RateLimited(m) => (StatusCode::TOO_MANY_REQUESTS, m.clone()),
            DomainError::ServiceUnavailable(m) => (StatusCode::SERVICE_UNAVAILABLE, m.clone()),
            DomainError::Infrastructure(m) => {
                tracing::error!(error = %m, "infrastructure error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal error".to_string(),
                )
            }
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn body_of(error: DomainError) -> (StatusCode, serde_json::Value) {
        let response = ApiError(error).into_response();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn a_service_that_cannot_work_here_is_a_503_that_says_why() {
        let (status, body) = body_of(DomainError::ServiceUnavailable(
            "passkeys are not available on this server".to_string(),
        ))
        .await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            body,
            serde_json::json!({ "error": "passkeys are not available on this server" })
        );
    }

    #[tokio::test]
    async fn an_infrastructure_failure_stays_a_500_that_hides_the_detail() {
        let (status, body) = body_of(DomainError::Infrastructure(
            "connection refused at 10.0.0.3".to_string(),
        ))
        .await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body, serde_json::json!({ "error": "internal error" }));
    }
}
