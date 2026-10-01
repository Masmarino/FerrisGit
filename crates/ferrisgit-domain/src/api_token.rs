use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::error::DomainError;

#[derive(Debug, Clone, Serialize)]
pub struct ApiToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    #[serde(skip_serializing)]
    pub token_hash: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

pub struct NewApiToken {
    pub user_id: Uuid,
    pub name: String,
    pub token_hash: String,
}

#[async_trait]
pub trait ApiTokenRepositoryPort: Send + Sync {
    async fn create(&self, new_token: NewApiToken) -> Result<ApiToken, DomainError>;
    async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<ApiToken>, DomainError>;
    async fn find_by_hash(&self, token_hash: &str) -> Result<Option<ApiToken>, DomainError>;
    async fn revoke(&self, id: Uuid, user_id: Uuid) -> Result<(), DomainError>;
    async fn touch_last_used(&self, id: Uuid) -> Result<(), DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn new_api_token_carries_the_fields_it_was_built_with() {
        let user_id = Uuid::new_v4();
        let new_token = NewApiToken {
            user_id,
            name: "ci".to_string(),
            token_hash: "abc".to_string(),
        };
        assert_eq!(new_token.user_id, user_id);
        assert_eq!(new_token.name, "ci");
    }

    #[test]
    fn serializing_an_api_token_never_includes_the_token_hash() {
        let token = ApiToken {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            name: "ci".to_string(),
            token_hash: "super-secret-hash".to_string(),
            created_at: Utc::now(),
            last_used_at: None,
        };
        let json = serde_json::to_string(&token).unwrap();
        assert!(!json.contains("super-secret-hash"));
        assert!(!json.contains("token_hash"));
    }
}
