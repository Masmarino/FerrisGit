use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::DomainError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
}

pub struct NewUser {
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub is_admin: bool,
}

#[async_trait]
pub trait UserRepositoryPort: Send + Sync {
    async fn find_by_username(&self, username: &str) -> Result<Option<User>, DomainError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, DomainError>;
    async fn create(&self, new_user: NewUser) -> Result<User, DomainError>;
    async fn count(&self) -> Result<i64, DomainError>;
    async fn update_email(&self, user_id: Uuid, email: String) -> Result<User, DomainError>;
    async fn update_password_hash(
        &self,
        user_id: Uuid,
        password_hash: String,
    ) -> Result<(), DomainError>;
    /// Active means the flag is set and no invitation is pending (an invited admin who never activated does not count).
    /// `SetAdminUseCase` uses it for an early, clear refusal of a demotion that would leave no usable admin.
    async fn count_admins(&self) -> Result<i64, DomainError>;
    /// Sets or clears the admin flag. `NotFound` for an unknown user. The store refuses, atomically and with
    /// `Conflict`, to clear it on the last active admin: `count_admins` followed by this write is check-then-act, and
    /// two admins demoting each other at the same time would both pass. There is no default implementation on purpose:
    /// a store that skipped the floor would be worse than none.
    async fn set_admin(&self, user_id: Uuid, is_admin: bool) -> Result<(), DomainError>;
    /// Deletes the account and its personal repositories in one transaction and returns them (the caller removes git
    /// storage and release assets once the rows are gone). The store cascades the rest of what the user owns
    /// and turns what they wrote elsewhere into content by a deleted user (the author field becomes `None`).
    /// `NotFound` for an unknown user.
    /// Refused with `Conflict`, atomically and before anything is written, for the last active admin (same floor as
    /// `set_admin`) and for the last Maintainer of a group hierarchy (same rule as `group_maintainer_guard`).
    /// A group repository the user created (`owner_id` means "created by", never an access grant) survives: its
    /// `owner_id` passes to `heir_id` (the acting admin), because `repositories.owner_id` cascades.
    /// `heir_id == user_id` is a `Validation` error. No default implementation, like `set_admin`.
    async fn delete(
        &self,
        user_id: Uuid,
        heir_id: Uuid,
    ) -> Result<Vec<crate::repository::Repository>, DomainError>;
    /// Username only, never scoped: any authenticated user can find any other by name.
    async fn search(&self, query: &str, limit: i64) -> Result<Vec<User>, DomainError>;
    /// Keep new accounts unique regardless of casing (the table's `UNIQUE` constraints are case-sensitive). No
    /// default implementations: a store that forgot them would let duplicates through.
    async fn find_by_username_ignore_case(
        &self,
        username: &str,
    ) -> Result<Option<User>, DomainError>;
    async fn find_by_email_ignore_case(&self, email: &str) -> Result<Option<User>, DomainError>;
    /// Ordered by creation time then id, at most `limit`.
    async fn list(&self, _limit: i64) -> Result<Vec<User>, DomainError> {
        Ok(vec![])
    }
    /// Epoch embedded in every JWT at issue time (see `bump_token_epoch`). Defaults to `Ok(0)` rather than
    /// `unimplemented!()` because it is on every authenticated request's hot path.
    async fn get_token_epoch(&self, _user_id: Uuid) -> Result<i32, DomainError> {
        Ok(0)
    }
    /// Invalidates every token issued before this call: a JWT carrying the old epoch fails `AuthUser`'s check even if
    /// unexpired. Called on password change and admin password/MFA reset. No-op by default, paired with
    /// `get_token_epoch`'s constant default.
    async fn bump_token_epoch(&self, _user_id: Uuid) -> Result<(), DomainError> {
        Ok(())
    }
}

pub trait PasswordHasherPort: Send + Sync {
    fn hash(&self, plain: &str) -> Result<String, DomainError>;
    fn verify(&self, plain: &str, hash: &str) -> Result<bool, DomainError>;
}

pub trait TokenIssuerPort: Send + Sync {
    /// `token_epoch` is embedded in the claims (see `UserRepositoryPort::bump_token_epoch`).
    fn issue(&self, user_id: Uuid, token_epoch: i32) -> Result<String, DomainError>;
    /// Returns the subject id and the token's `token_epoch`. `AuthUser` compares it with the user's current epoch to
    /// detect revocation.
    fn verify(&self, token: &str) -> Result<(Uuid, i32), DomainError>;
    /// Affects subsequent `issue` calls immediately; already-issued tokens keep their `exp`.
    fn set_ttl_hours(&self, hours: i64);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn new_user_carries_the_fields_it_was_built_with() {
        let new_user = NewUser {
            username: "florian".to_string(),
            email: "florian@example.com".to_string(),
            password_hash: "hash".to_string(),
            is_admin: true,
        };
        assert_eq!(new_user.username, "florian");
        assert!(new_user.is_admin);
    }

    #[test]
    fn serializing_a_user_never_includes_the_password_hash() {
        let user = User {
            id: Uuid::new_v4(),
            username: "florian".to_string(),
            email: "f@example.com".to_string(),
            password_hash: "super-secret-hash".to_string(),
            is_admin: false,
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&user).unwrap();
        assert!(!json.contains("super-secret-hash"));
        assert!(!json.contains("password_hash"));
    }
}
