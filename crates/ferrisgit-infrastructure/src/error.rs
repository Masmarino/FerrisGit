use ferrisgit_domain::error::DomainError;
use std::fmt::Display;

/// Any lower-level failure (sqlx, gix, I/O...) as the domain's opaque infrastructure error.
pub(crate) fn infra(error: impl Display) -> DomainError {
    DomainError::Infrastructure(error.to_string())
}

/// A unique violation becomes `Conflict`, anything else a plain infrastructure error.
pub(crate) fn conflict_on_duplicate(
    message: impl FnOnce() -> String,
) -> impl FnOnce(sqlx::Error) -> DomainError {
    move |error| match &error {
        sqlx::Error::Database(db_error) if db_error.is_unique_violation() => {
            DomainError::Conflict(message())
        }
        _ => infra(error),
    }
}

/// Runs blocking work off the async executor; a failed join and the work's own error both become infra errors.
pub(crate) async fn blocking<T, E>(
    work: impl FnOnce() -> Result<T, E> + Send + 'static,
) -> Result<T, DomainError>
where
    T: Send + 'static,
    E: Display + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(infra)?
        .map_err(infra)
}
