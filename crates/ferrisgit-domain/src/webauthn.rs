use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::DomainError;

/// A registered passkey. `passkey_json` is the serialized `webauthn_rs` `Passkey`, opaque to the domain. It's public
/// material so it isn't encrypted at rest, but it's never logged: `Debug` redacts it.
#[derive(Clone, PartialEq, Eq)]
pub struct StoredPasskey {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub credential_id: Vec<u8>,
    pub passkey_json: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

impl std::fmt::Debug for StoredPasskey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredPasskey")
            .field("id", &self.id)
            .field("user_id", &self.user_id)
            .field("name", &self.name)
            .field("credential_id", &"[redacted]")
            .field("passkey_json", &"[redacted]")
            .field("created_at", &self.created_at)
            .field("last_used_at", &self.last_used_at)
            .finish()
    }
}

#[async_trait]
pub trait WebauthnCredentialPort: Send + Sync {
    /// Oldest first.
    async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<StoredPasskey>, DomainError>;
    async fn count_for_user(&self, user_id: Uuid) -> Result<i64, DomainError>;
    /// `false` if a credential with this `credential_id` already exists, the same authenticator registered twice or another
    /// user's. Nothing is written.
    async fn insert(&self, passkey: &StoredPasskey) -> Result<bool, DomainError>;
    /// Stores the updated passkey (signature counter, backup state) and sets `last_used_at` to now.
    async fn update_after_authentication(
        &self,
        id: Uuid,
        passkey_json: &str,
    ) -> Result<(), DomainError>;
    /// Scoped to `user_id`: `false`, and nothing deleted, if the passkey belongs to someone else.
    async fn delete(&self, id: Uuid, user_id: Uuid) -> Result<bool, DomainError>;
    async fn delete_all_for_user(&self, user_id: Uuid) -> Result<(), DomainError>;
    /// Which of these users have at least one passkey, in one query.
    async fn user_ids_with_passkeys(&self, user_ids: &[Uuid]) -> Result<Vec<Uuid>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_of_a_stored_passkey_never_contains_the_key_material() {
        let passkey = StoredPasskey {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            name: "MacBook".to_string(),
            credential_id: vec![0xde, 0xad, 0xbe, 0xef],
            passkey_json: "{\"cred\":\"PUBLIC-KEY-MATERIAL\"}".to_string(),
            created_at: Utc::now(),
            last_used_at: None,
        };

        let debug = format!("{passkey:?}");
        let wrapped = format!("{:?}", Ok::<_, DomainError>(vec![passkey]));

        for text in [&debug, &wrapped] {
            assert!(!text.contains("PUBLIC-KEY-MATERIAL"), "{text}");
            assert!(!text.contains("222"), "credential id bytes leaked: {text}");
            assert!(text.contains("[redacted]"), "{text}");
            assert!(text.contains("MacBook"), "{text}");
        }
    }
}
