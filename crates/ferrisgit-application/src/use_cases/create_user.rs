use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::user::{NewUser, PasswordHasherPort, User, UserRepositoryPort};

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
        if password.len() < 8 {
            return Err(DomainError::Validation(
                "password must be at least 8 characters".to_string(),
            ));
        }
        if username.trim().is_empty()
            || !username
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            // Same charset check as `create_repository.rs`/`create_group.rs`: a `.git`-ending segment such as `x.git`
            // would make the git-vs-SPA fallback misroute the user's own pages to the git handler.
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
    use chrono::Utc;
    use ferrisgit_domain::group::Group;
    use uuid::Uuid;

    #[tokio::test]
    async fn creates_a_new_non_admin_user() {
        let use_case = CreateUserUseCase::new(
            Arc::new(FakeUsers::new(vec![])),
            Arc::new(FakeHasher),
            Arc::new(FakeGroups::empty()),
        );

        let created = use_case
            .execute(
                "alice".to_string(),
                "alice@example.com".to_string(),
                "password12345".to_string(),
            )
            .await
            .unwrap();

        assert_eq!(created.username, "alice");
        assert!(!created.is_admin);
    }

    #[tokio::test]
    async fn rejects_a_duplicate_username() {
        let existing = User {
            id: Uuid::new_v4(),
            username: "alice".to_string(),
            email: "a@a.com".to_string(),
            password_hash: "h".to_string(),
            is_admin: false,
            created_at: Utc::now(),
        };
        let use_case = CreateUserUseCase::new(
            Arc::new(FakeUsers::new(vec![existing])),
            Arc::new(FakeHasher),
            Arc::new(FakeGroups::empty()),
        );

        let result = use_case
            .execute(
                "alice".to_string(),
                "alice2@example.com".to_string(),
                "password12345".to_string(),
            )
            .await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
    }

    #[tokio::test]
    async fn rejects_a_password_shorter_than_8_characters() {
        let use_case = CreateUserUseCase::new(
            Arc::new(FakeUsers::new(vec![])),
            Arc::new(FakeHasher),
            Arc::new(FakeGroups::empty()),
        );

        let result = use_case
            .execute(
                "alice".to_string(),
                "alice@example.com".to_string(),
                "short".to_string(),
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn rejects_a_username_with_an_invalid_charset() {
        let use_case = CreateUserUseCase::new(
            Arc::new(FakeUsers::new(vec![])),
            Arc::new(FakeHasher),
            Arc::new(FakeGroups::empty()),
        );

        let result = use_case
            .execute(
                "x.git".to_string(),
                "x@example.com".to_string(),
                "password12345".to_string(),
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn rejects_a_username_colliding_with_an_existing_root_group() {
        let existing_group = Group {
            id: Uuid::new_v4(),
            parent_group_id: None,
            name: "acme".to_string(),
            description: String::new(),
            created_by: Some(Uuid::new_v4()),
            created_at: Utc::now(),
        };
        let use_case = CreateUserUseCase::new(
            Arc::new(FakeUsers::new(vec![])),
            Arc::new(FakeHasher),
            Arc::new(FakeGroups::new(vec![existing_group])),
        );

        let result = use_case
            .execute(
                "acme".to_string(),
                "acme@example.com".to_string(),
                "password12345".to_string(),
            )
            .await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
    }
}
