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
    /// Counts admins who are active: flag set and no pending invitation. `SetAdminUseCase` uses it to refuse early, with a
    /// clear message, a demotion that would leave no usable admin.
    async fn count_admins(&self) -> Result<i64, DomainError>;
    /// Sets or clears the admin flag. `NotFound` for an unknown user. Clearing it on the last active admin is refused with
    /// `Conflict`, atomically: `count_admins` then this write is check-then-act, so two admins demoting each other at once
    /// would both pass. No default on purpose, a store that skipped this check would be worse than none.
    async fn set_admin(&self, user_id: Uuid, is_admin: bool) -> Result<(), DomainError>;
    /// Deletes the account and its personal repositories in one transaction and returns them so the caller can remove the
    /// git storage and release assets. Other things the user owns cascade, and what they wrote elsewhere stays with a
    /// `None` author. `NotFound` for an unknown user.
    ///
    /// Refused with `Conflict`, before anything is written, for the last active admin (same rule as `set_admin`) and for the
    /// last Maintainer of a group hierarchy. A group repo the user created survives with `heir_id` (the acting admin) as its
    /// `owner_id`, because that column cascades. `heir_id == user_id` is a `Validation` error. No default, like `set_admin`.
    async fn delete(
        &self,
        user_id: Uuid,
        heir_id: Uuid,
    ) -> Result<Vec<crate::repository::Repository>, DomainError>;
    /// By username only and never scoped: any signed-in user can find anyone by name.
    async fn search(&self, query: &str, limit: i64) -> Result<Vec<User>, DomainError>;
    /// Used to keep usernames and emails unique regardless of case, since the `UNIQUE` constraints are case-sensitive. No
    /// default: a store that forgot them would let duplicates through.
    async fn find_by_username_ignore_case(
        &self,
        username: &str,
    ) -> Result<Option<User>, DomainError>;
    async fn find_by_email_ignore_case(&self, email: &str) -> Result<Option<User>, DomainError>;
    /// An invited account's owner names it and sets its password, in one write. `Conflict` if the name was taken in
    /// the meantime. Unsupported by default: only the stores that serve activation implement it.
    async fn set_username_and_password_hash(
        &self,
        _user_id: Uuid,
        _username: String,
        _password_hash: String,
    ) -> Result<(), DomainError> {
        Err(DomainError::Infrastructure(
            "naming an account is not supported by this store".to_string(),
        ))
    }
    /// Ordered by creation time then id.
    async fn list(&self, _limit: i64) -> Result<Vec<User>, DomainError> {
        Ok(vec![])
    }
    /// The epoch stamped into every JWT. Defaults to 0 instead of `unimplemented!()` since every authenticated request
    /// reads it.
    async fn get_token_epoch(&self, _user_id: Uuid) -> Result<i32, DomainError> {
        Ok(0)
    }
    /// Invalidates every token issued so far: a JWT with the old epoch fails the check even if it hasn't expired. Called on
    /// password change and on admin password or MFA resets. No-op by default, matching the constant `get_token_epoch`.
    async fn bump_token_epoch(&self, _user_id: Uuid) -> Result<(), DomainError> {
        Ok(())
    }
}

pub trait PasswordHasherPort: Send + Sync {
    fn hash(&self, plain: &str) -> Result<String, DomainError>;
    fn verify(&self, plain: &str, hash: &str) -> Result<bool, DomainError>;
}

pub trait TokenIssuerPort: Send + Sync {
    /// The epoch goes into the claims, see `UserRepositoryPort::bump_token_epoch`.
    fn issue(&self, user_id: Uuid, token_epoch: i32) -> Result<String, DomainError>;
    /// Returns the subject id and the token epoch, which `AuthUser` compares to the user's current one to detect revocation.
    fn verify(&self, token: &str) -> Result<(Uuid, i32), DomainError>;
    /// Applies to the next `issue` right away, tokens already issued keep their `exp`.
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
