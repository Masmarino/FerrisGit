use std::sync::Arc;

use ferrisgit_domain::api_token::{ApiToken, ApiTokenRepositoryPort, NewApiToken};
use ferrisgit_domain::error::DomainError;
use rand::Rng;
use uuid::Uuid;

use crate::token_hash::hash_token;

pub struct CreateApiTokenUseCase {
    api_tokens: Arc<dyn ApiTokenRepositoryPort>,
}

impl CreateApiTokenUseCase {
    pub fn new(api_tokens: Arc<dyn ApiTokenRepositoryPort>) -> Self {
        Self { api_tokens }
    }

    /// Returns the stored record with the plaintext token. Only the hash is persisted, so this is the one time the
    /// plaintext is available.
    pub async fn execute(
        &self,
        user_id: Uuid,
        name: String,
    ) -> Result<(ApiToken, String), DomainError> {
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        let plain = format!("fg_{}", hex::encode(bytes));
        let token_hash = hash_token(&plain);

        let token = self
            .api_tokens
            .create(NewApiToken {
                user_id,
                name,
                token_hash,
            })
            .await?;
        Ok((token, plain))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeApiTokens;

    #[tokio::test]
    async fn creating_a_token_stores_only_its_hash_and_returns_the_plaintext_once() {
        let repo = Arc::new(FakeApiTokens::empty());
        let use_case = CreateApiTokenUseCase::new(repo.clone());
        let user_id = Uuid::new_v4();

        let (stored, plain) = use_case.execute(user_id, "ci".to_string()).await.unwrap();

        assert!(plain.starts_with("fg_"));
        assert_ne!(stored.token_hash, plain);
        assert_eq!(hash_token(&plain), stored.token_hash);
    }

    #[tokio::test]
    async fn two_created_tokens_have_different_plaintexts() {
        let repo = Arc::new(FakeApiTokens::empty());
        let use_case = CreateApiTokenUseCase::new(repo);
        let user_id = Uuid::new_v4();

        let (_, plain_a) = use_case.execute(user_id, "a".to_string()).await.unwrap();
        let (_, plain_b) = use_case.execute(user_id, "b".to_string()).await.unwrap();

        assert_ne!(plain_a, plain_b);
    }
}
