use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::user::{User, UserRepositoryPort};
use uuid::Uuid;

pub struct UpdateEmailUseCase {
    users: Arc<dyn UserRepositoryPort>,
}

impl UpdateEmailUseCase {
    pub fn new(users: Arc<dyn UserRepositoryPort>) -> Self {
        Self { users }
    }

    pub async fn execute(&self, user_id: Uuid, email: String) -> Result<User, DomainError> {
        let email = email.trim();
        if email.is_empty() || !email.contains('@') {
            return Err(DomainError::Validation(
                "email must be a valid address".to_string(),
            ));
        }
        self.users.update_email(user_id, email.to_string()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeUsers;
    use crate::use_cases::fixtures::user;

    #[tokio::test]
    async fn updates_the_email() {
        let seed = user("florian");
        let use_case = UpdateEmailUseCase::new(Arc::new(FakeUsers::new(vec![seed.clone()])));

        let updated = use_case
            .execute(seed.id, "new@example.com".to_string())
            .await
            .unwrap();

        assert_eq!(updated.email, "new@example.com");
    }

    #[tokio::test]
    async fn rejects_an_email_with_no_at_sign() {
        let seed = user("florian");
        let use_case = UpdateEmailUseCase::new(Arc::new(FakeUsers::new(vec![seed.clone()])));

        let result = use_case.execute(seed.id, "not-an-email".to_string()).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn rejects_an_empty_email() {
        let seed = user("florian");
        let use_case = UpdateEmailUseCase::new(Arc::new(FakeUsers::new(vec![seed.clone()])));

        let result = use_case.execute(seed.id, "   ".to_string()).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }
}
