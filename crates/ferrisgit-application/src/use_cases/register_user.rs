use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::registration::RegistrationSettingsPort;
use ferrisgit_domain::user::{NewUser, PasswordHasherPort, User, UserRepositoryPort};

use crate::account_rules::{
    ensure_account_available, hash_blocking, normalize_email, normalize_username, validate_password,
};

/// Self-registration, only while an admin has it switched on. Always a plain non-admin account.
pub struct RegisterUserUseCase {
    users: Arc<dyn UserRepositoryPort>,
    hasher: Arc<dyn PasswordHasherPort>,
    groups: Arc<dyn GroupStorePort>,
    registration: Arc<dyn RegistrationSettingsPort>,
}

impl RegisterUserUseCase {
    pub fn new(
        users: Arc<dyn UserRepositoryPort>,
        hasher: Arc<dyn PasswordHasherPort>,
        groups: Arc<dyn GroupStorePort>,
        registration: Arc<dyn RegistrationSettingsPort>,
    ) -> Self {
        Self {
            users,
            hasher,
            groups,
            registration,
        }
    }

    pub async fn execute(
        &self,
        username: String,
        email: String,
        password: String,
    ) -> Result<User, DomainError> {
        // First, so a disabled instance answers the same whatever is submitted and nobody can probe usernames.
        if !self.registration.is_enabled().await? {
            return Err(DomainError::Validation(
                "registration is disabled".to_string(),
            ));
        }
        validate_password(&password)?;
        let username = normalize_username(&username)?;
        let email = normalize_email(&email)?;
        ensure_account_available(&self.users, &self.groups, &username, &email).await?;
        let password_hash = hash_blocking(&self.hasher, password).await?;
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
    use crate::test_support::{FakeGroups, FakeHasher, FakeRegistration, FakeUsers};
    use chrono::Utc;
    use ferrisgit_domain::group::Group;
    use uuid::Uuid;

    struct Fixture {
        users: Arc<FakeUsers>,
        use_case: RegisterUserUseCase,
    }

    fn fixture(enabled: bool, existing: Vec<User>, groups: Vec<Group>) -> Fixture {
        let users = Arc::new(FakeUsers::new(existing));
        let use_case = RegisterUserUseCase::new(
            users.clone(),
            Arc::new(FakeHasher),
            Arc::new(FakeGroups::new(groups)),
            Arc::new(FakeRegistration::new(enabled)),
        );
        Fixture { users, use_case }
    }

    fn existing(username: &str, email: &str) -> User {
        User {
            id: Uuid::new_v4(),
            username: username.to_string(),
            email: email.to_string(),
            password_hash: "h".to_string(),
            is_admin: false,
            created_at: Utc::now(),
        }
    }

    async fn register(
        f: &Fixture,
        username: &str,
        email: &str,
        password: &str,
    ) -> Result<User, DomainError> {
        f.use_case
            .execute(
                username.to_string(),
                email.to_string(),
                password.to_string(),
            )
            .await
    }

    #[tokio::test]
    async fn refuses_while_registration_is_disabled_and_creates_nothing() {
        let f = fixture(false, vec![], vec![]);

        let result = register(&f, "alice", "alice@example.com", "password12345").await;

        assert!(
            matches!(&result, Err(DomainError::Validation(m)) if m == "registration is disabled"),
            "{result:?}"
        );
        assert!(f.users.snapshot().is_empty());
    }

    #[tokio::test]
    async fn creates_a_non_admin_user_with_a_hashed_password_when_enabled() {
        let f = fixture(true, vec![], vec![]);

        let created = register(&f, "  Alice ", " Alice@Example.com ", "password12345")
            .await
            .unwrap();

        assert_eq!(created.username, "alice");
        assert_eq!(created.email, "Alice@Example.com");
        assert!(!created.is_admin);
        assert_eq!(created.password_hash, "hashed:password12345");
        assert_eq!(f.users.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn refuses_a_username_taken_in_another_casing() {
        let f = fixture(true, vec![existing("alice", "a@example.com")], vec![]);

        let result = register(&f, "ALICE", "other@example.com", "password12345").await;

        assert!(
            matches!(&result, Err(DomainError::Conflict(m)) if m == "username already taken"),
            "{result:?}"
        );
        assert_eq!(f.users.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn refuses_an_e_mail_in_use_in_another_casing() {
        let f = fixture(true, vec![existing("alice", "Alice@Example.com")], vec![]);

        let result = register(&f, "bob", "alice@example.COM", "password12345").await;

        assert!(
            matches!(&result, Err(DomainError::Conflict(m)) if m == "email already in use"),
            "{result:?}"
        );
        assert_eq!(f.users.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn refuses_a_username_colliding_with_a_root_group() {
        let group = Group {
            id: Uuid::new_v4(),
            parent_group_id: None,
            name: "acme".to_string(),
            description: String::new(),
            created_by: Some(Uuid::new_v4()),
            created_at: Utc::now(),
        };
        let f = fixture(true, vec![], vec![group]);

        let result = register(&f, "Acme", "acme@example.com", "password12345").await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
        assert!(f.users.snapshot().is_empty());
    }

    #[tokio::test]
    async fn invalid_input_is_a_validation_error_and_creates_nothing() {
        let f = fixture(true, vec![], vec![]);

        for (username, email, password) in [
            ("ab", "a@example.com", "password12345"),
            ("admin", "a@example.com", "password12345"),
            ("x.git", "a@example.com", "password12345"),
            ("alice", "not-an-email", "password12345"),
            ("alice", "alice@localhost", "password12345"),
            ("alice", "a@example.com", "short"),
        ] {
            let result = register(&f, username, email, password).await;
            assert!(
                matches!(result, Err(DomainError::Validation(_))),
                "{username}/{email}/{password}: {result:?}"
            );
        }
        assert!(f.users.snapshot().is_empty());
    }

    #[tokio::test]
    async fn the_disabled_check_comes_before_any_other_validation() {
        let f = fixture(false, vec![existing("alice", "a@example.com")], vec![]);

        // With the other rules first this would be a conflict, and a disabled instance would leak usernames.
        let result = register(&f, "alice", "a@example.com", "x").await;

        assert!(
            matches!(&result, Err(DomainError::Validation(m)) if m == "registration is disabled"),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn the_password_is_hashed_off_the_async_thread() {
        let hasher = Arc::new(crate::test_support::ThreadRecordingHasher::default());
        let users = Arc::new(FakeUsers::new(vec![]));
        let use_case = RegisterUserUseCase::new(
            users,
            hasher.clone(),
            Arc::new(FakeGroups::new(vec![])),
            Arc::new(FakeRegistration::new(true)),
        );

        use_case
            .execute(
                "alice".to_string(),
                "alice@example.com".to_string(),
                "password12345".to_string(),
            )
            .await
            .unwrap();

        let threads = hasher.hash_threads();
        assert_eq!(threads.len(), 1);
        assert_ne!(threads[0], std::thread::current().id());
    }
}
