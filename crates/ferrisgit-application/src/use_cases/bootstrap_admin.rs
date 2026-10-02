use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::user::{NewUser, PasswordHasherPort, UserRepositoryPort};

use crate::account_rules::MIN_PASSWORD_LEN;

pub struct BootstrapAdminUseCase {
    users: Arc<dyn UserRepositoryPort>,
    hasher: Arc<dyn PasswordHasherPort>,
}

impl BootstrapAdminUseCase {
    pub fn new(users: Arc<dyn UserRepositoryPort>, hasher: Arc<dyn PasswordHasherPort>) -> Self {
        Self { users, hasher }
    }

    pub async fn execute(
        &self,
        username: Option<String>,
        password: Option<String>,
    ) -> Result<(), DomainError> {
        let (Some(username), Some(password)) = (username, password) else {
            return Ok(());
        };

        if self.users.count().await? > 0 {
            return Ok(());
        }
        if password.len() < MIN_PASSWORD_LEN {
            tracing::warn!(
                "bootstrap admin password is shorter than {MIN_PASSWORD_LEN} characters, skipping bootstrap"
            );
            return Ok(());
        }

        let password_hash = self.hasher.hash(&password)?;
        self.users
            .create(NewUser {
                username: username.clone(),
                email: format!("{username}@localhost"),
                password_hash,
                is_admin: true,
            })
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeHasher, FakeUsers};
    use crate::use_cases::fixtures::user;
    use ferrisgit_domain::user::User;

    fn use_case(existing: Vec<User>) -> (BootstrapAdminUseCase, Arc<FakeUsers>) {
        let users = Arc::new(FakeUsers::new(existing));
        let use_case = BootstrapAdminUseCase::new(users.clone(), Arc::new(FakeHasher));
        (use_case, users)
    }

    #[tokio::test]
    async fn creates_the_admin_when_the_users_table_is_empty_and_credentials_are_set() {
        let (use_case, users) = use_case(vec![]);

        use_case
            .execute(
                Some("admin".to_string()),
                Some("longenoughpassword".to_string()),
            )
            .await
            .unwrap();

        assert_eq!(users.snapshot().len(), 1);
        assert!(users.snapshot()[0].is_admin);
    }

    #[tokio::test]
    async fn does_nothing_when_a_user_already_exists() {
        let (use_case, users) = use_case(vec![user("someone")]);

        use_case
            .execute(
                Some("admin".to_string()),
                Some("longenoughpassword".to_string()),
            )
            .await
            .unwrap();

        assert_eq!(users.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn does_nothing_when_credentials_are_not_set() {
        let (use_case, users) = use_case(vec![]);

        use_case.execute(None, None).await.unwrap();

        assert!(users.snapshot().is_empty());
    }

    #[tokio::test]
    async fn skips_bootstrap_when_the_password_is_too_short() {
        let (use_case, users) = use_case(vec![]);

        use_case
            .execute(Some("admin".to_string()), Some("short".to_string()))
            .await
            .unwrap();

        assert!(users.snapshot().is_empty());
    }
}
