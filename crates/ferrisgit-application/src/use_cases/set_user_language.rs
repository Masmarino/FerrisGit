use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::user::{UserRepositoryPort, normalize_language};
use uuid::Uuid;

pub struct SetUserLanguageUseCase {
    users: Arc<dyn UserRepositoryPort>,
}

impl SetUserLanguageUseCase {
    pub fn new(users: Arc<dyn UserRepositoryPort>) -> Self {
        Self { users }
    }

    /// Saves the interface language on the account and returns it as stored. `None` clears it: the interface follows
    /// the browser again.
    pub async fn execute(
        &self,
        user_id: Uuid,
        language: Option<&str>,
    ) -> Result<Option<String>, DomainError> {
        let language = language.map(normalize_language).transpose()?;
        self.users
            .set_language(user_id, language.as_deref())
            .await?;
        Ok(language)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeUsers;
    use crate::use_cases::fixtures::user;

    #[tokio::test]
    async fn saves_a_language_then_clears_it() {
        let seed = user("florian");
        let users = Arc::new(FakeUsers::new(vec![seed.clone()]));
        let use_case = SetUserLanguageUseCase::new(users.clone());

        let saved = use_case.execute(seed.id, Some("DE")).await.unwrap();
        assert_eq!(saved.as_deref(), Some("de"));
        assert_eq!(
            users.language(seed.id).await.unwrap().as_deref(),
            Some("de")
        );

        assert_eq!(use_case.execute(seed.id, None).await.unwrap(), None);
        assert_eq!(users.language(seed.id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn refuses_a_language_the_interface_is_not_translated_into() {
        let seed = user("florian");
        let users = Arc::new(FakeUsers::new(vec![seed.clone()]));
        users.set_language(seed.id, Some("it")).await.unwrap();
        let use_case = SetUserLanguageUseCase::new(users.clone());

        let result = use_case.execute(seed.id, Some("pt")).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
        assert_eq!(
            users.language(seed.id).await.unwrap().as_deref(),
            Some("it")
        );
    }
}
