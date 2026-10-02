use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::user::{PasswordHasherPort, UserRepositoryPort};
use uuid::Uuid;

use crate::account_rules::validate_password;

pub struct ChangePasswordUseCase {
    users: Arc<dyn UserRepositoryPort>,
    hasher: Arc<dyn PasswordHasherPort>,
}

impl ChangePasswordUseCase {
    pub fn new(users: Arc<dyn UserRepositoryPort>, hasher: Arc<dyn PasswordHasherPort>) -> Self {
        Self { users, hasher }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        current_password: &str,
        new_password: &str,
    ) -> Result<(), DomainError> {
        validate_password(new_password)?;
        let user = self
            .users
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("user".to_string()))?;
        if !self.hasher.verify(current_password, &user.password_hash)? {
            // Validation, not Unauthorized: a 401 would trip the client's auto-logout over a mistyped form value.
            return Err(DomainError::Validation(
                "current password is incorrect".to_string(),
            ));
        }
        let new_hash = self.hasher.hash(new_password)?;
        self.users.update_password_hash(user_id, new_hash).await?;
        // Revokes every JWT issued before this change; a leaked token would otherwise stay valid until expiry.
        self.users.bump_token_epoch(user_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeHasher, FakeUsers};
    use crate::use_cases::fixtures::user;
    use ferrisgit_domain::user::User;

    const OLD_PASSWORD_HASH: &str = "hashed:old-password";

    /// A user whose current password is `old-password`.
    fn fixture() -> (ChangePasswordUseCase, Arc<FakeUsers>, User) {
        let seed = User {
            password_hash: OLD_PASSWORD_HASH.to_string(),
            ..user("florian")
        };
        let users = Arc::new(FakeUsers::new(vec![seed.clone()]));
        let use_case = ChangePasswordUseCase::new(users.clone(), Arc::new(FakeHasher));
        (use_case, users, seed)
    }

    #[tokio::test]
    async fn changes_the_password_when_the_current_one_is_correct() {
        let (use_case, users, seed) = fixture();

        use_case
            .execute(seed.id, "old-password", "new-password123")
            .await
            .unwrap();

        assert_eq!(
            users.get(seed.id).unwrap().password_hash,
            "hashed:new-password123"
        );
    }

    #[tokio::test]
    async fn a_successful_password_change_bumps_the_token_epoch() {
        let (use_case, users, seed) = fixture();

        use_case
            .execute(seed.id, "old-password", "new-password123")
            .await
            .unwrap();

        assert_eq!(
            users.token_epoch_of(seed.id),
            1,
            "a successful password change must revoke every previously-issued token"
        );
    }

    #[tokio::test]
    async fn rejects_an_incorrect_current_password() {
        let (use_case, users, seed) = fixture();

        let result = use_case
            .execute(seed.id, "wrong-password", "new-password123")
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
        assert_eq!(
            users.get(seed.id).unwrap().password_hash,
            OLD_PASSWORD_HASH,
            "the password must not change on a rejected attempt"
        );
        assert_eq!(
            users.token_epoch_of(seed.id),
            0,
            "a rejected attempt must not revoke any existing token"
        );
    }

    #[tokio::test]
    async fn rejects_a_new_password_shorter_than_8_characters() {
        let (use_case, _, seed) = fixture();

        let result = use_case.execute(seed.id, "old-password", "short").await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }
}
