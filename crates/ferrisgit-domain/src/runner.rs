use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::error::DomainError;

#[derive(Debug, Clone, Serialize)]
pub struct Runner {
    pub id: Uuid,
    pub name: String,
    pub token_hash: String,
    pub tags: Vec<String>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

pub struct NewRunner {
    pub name: String,
    pub token_hash: String,
    pub tags: Vec<String>,
}

#[async_trait]
pub trait RunnerRepositoryPort: Send + Sync {
    async fn create(&self, new_runner: NewRunner) -> Result<Runner, DomainError>;
    async fn find_by_token_hash(&self, token_hash: &str) -> Result<Option<Runner>, DomainError>;
    async fn list(&self) -> Result<Vec<Runner>, DomainError>;
    async fn touch_heartbeat(&self, id: Uuid) -> Result<(), DomainError>;
    /// Permanently revokes a runner: its token stops authenticating anything.
    /// Deleting a runner that does not exist is not an error.
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_runner_carries_the_fields_it_was_built_with() {
        let new_runner = NewRunner {
            name: "vps-1".to_string(),
            token_hash: "h".to_string(),
            tags: vec!["docker".to_string()],
        };
        assert_eq!(new_runner.name, "vps-1");
        assert_eq!(new_runner.tags, vec!["docker".to_string()]);
    }
}
