use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::user::{NewUser, PasswordHasherPort, User, UserRepositoryPort};

use super::name_rules::is_valid_path_name;
use crate::account_rules::validate_password;

pub struct CreateUserUseCase {
    users: Arc<dyn UserRepositoryPort>,
    hasher: Arc<dyn PasswordHasherPort>,
    groups: Arc<dyn GroupStorePort>,
}

impl CreateUserUseCase {
    pub fn new(
        users: Arc<dyn UserRepositoryPort>,
        hasher: Arc<dyn PasswordHasherPort>,
        groups: Arc<dyn GroupStorePort>,
    ) -> Self {
        Self {
            users,
            hasher,
            groups,
        }
    }

    pub async fn execute(
        &self,
        username: String,
        email: String,
        password: String,
    ) -> Result<User, DomainError> {
        validate_password(&password)?;
        if !is_valid_path_name(&username) {
            return Err(DomainError::Validation(
                "username must be non-empty and alphanumeric/-/_ only".to_string(),
            ));
        }
        if self.users.find_by_username(&username).await?.is_some() {
            return Err(DomainError::Conflict("username already taken".to_string()));
        }
        if self
            .groups
            .find_child_by_name(None, &username)
            .await?
            .is_some()
        {
            return Err(DomainError::Conflict(
                "username collides with an existing root group".to_string(),
            ));
        }
        let password_hash = self.hasher.hash(&password)?;
        self.users
            .create(NewUser {
                username,
                email,
                password_hash,
                is_admin: false,
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeGroups, FakeHasher, FakeUsers};
    use crate::use_cases::fixtures::{group, user};
    use ferrisgit_domain::group::Group;

    fn use_case(users: Vec<User>, groups: Vec<Group>) -> CreateUserUseCase {
        CreateUserUseCase::new(
            Arc::new(FakeUsers::new(users)),
            Arc::new(FakeHasher),
            Arc::new(FakeGroups::new(groups)),
        )
    }

    async fn create(
        use_case: &CreateUserUseCase,
        username: &str,
        password: &str,
    ) -> Result<User, DomainError> {
        use_case
            .execute(
                username.to_string(),
                format!("{username}@example.com"),
                password.to_string(),
            )
            .await
    }

    #[tokio::test]
    async fn creates_a_new_non_admin_user() {
        let use_case = use_case(vec![], vec![]);

        let created = create(&use_case, "alice", "password12345").await.unwrap();

        assert_eq!(created.username, "alice");
        assert!(!created.is_admin);
    }

    #[tokio::test]
    async fn rejects_a_duplicate_username() {
        let use_case = use_case(vec![user("alice")], vec![]);

        let result = create(&use_case, "alice", "password12345").await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
    }

    #[tokio::test]
    async fn rejects_a_password_shorter_than_8_characters() {
        let use_case = use_case(vec![], vec![]);

        let result = create(&use_case, "alice", "short").await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn rejects_a_username_with_an_invalid_charset() {
        let use_case = use_case(vec![], vec![]);

        let result = create(&use_case, "x.git", "password12345").await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn rejects_a_username_colliding_with_an_existing_root_group() {
        let use_case = use_case(vec![], vec![group(None, "acme")]);

        let result = create(&use_case, "acme", "password12345").await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
    }
}
