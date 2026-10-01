#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("rate limited: {0}")]
    RateLimited(String),
    #[error("infrastructure error: {0}")]
    Infrastructure(String),
    /// A feature that is switched off by the deployment's configuration (for example passkeys with an IP-literal public
    /// URL). Not a fault, and not the caller's doing. The API answers 503.
    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_formats_with_its_message() {
        let error = DomainError::NotFound("user".to_string());
        assert_eq!(error.to_string(), "not found: user");
    }

    #[test]
    fn service_unavailable_formats_with_its_message_and_is_distinct_from_infrastructure() {
        let error = DomainError::ServiceUnavailable("passkeys".to_string());
        assert_eq!(error.to_string(), "service unavailable: passkeys");
        assert!(matches!(error, DomainError::ServiceUnavailable(_)));
        assert!(!matches!(error, DomainError::Infrastructure(_)));
    }
}
