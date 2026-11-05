/// Each variant is an exit code, documented in `docs/ligne-de-commande/scripts-et-agents.md`. Scripts rely on them: they
/// only change with a version.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// Anything else: a Git command that failed, a pipeline that didn't succeed, a file that can't be read.
    #[error("{0}")]
    Other(String),
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    Auth(String),
    #[error("{0}")]
    NotFound(String),
    /// The server understood and said no: scope, validation, conflict, rate limit, merge conditions.
    #[error("{0}")]
    Refused(String),
    /// The server failed or couldn't be reached. Worth retrying, unlike all of the above.
    #[error("{0}")]
    Server(String),
}

impl CliError {
    pub fn exit_code(&self) -> u8 {
        match self {
            CliError::Other(_) => 1,
            CliError::Usage(_) => 2,
            CliError::Auth(_) => 3,
            CliError::NotFound(_) => 4,
            CliError::Refused(_) => 5,
            CliError::Server(_) => 6,
        }
    }
}

pub type Result<T> = std::result::Result<T, CliError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_documented_exit_codes_hold() {
        let code = |e: CliError| e.exit_code();
        assert_eq!(code(CliError::Other(String::new())), 1);
        assert_eq!(code(CliError::Usage(String::new())), 2);
        assert_eq!(code(CliError::Auth(String::new())), 3);
        assert_eq!(code(CliError::NotFound(String::new())), 4);
        assert_eq!(code(CliError::Refused(String::new())), 5);
        assert_eq!(code(CliError::Server(String::new())), 6);
    }
}
