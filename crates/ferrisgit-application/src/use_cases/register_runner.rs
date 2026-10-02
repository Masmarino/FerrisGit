use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::runner::{NewRunner, Runner, RunnerRepositoryPort};
use rand::Rng;

use crate::token_hash::hash_token;

pub struct RegisterRunnerUseCase {
    runners: Arc<dyn RunnerRepositoryPort>,
}

impl RegisterRunnerUseCase {
    pub fn new(runners: Arc<dyn RunnerRepositoryPort>) -> Self {
        Self { runners }
    }

    /// The plaintext token is returned here and never stored, so this is the only time anyone sees it.
    pub async fn execute(
        &self,
        name: String,
        tags: Vec<String>,
    ) -> Result<(Runner, String), DomainError> {
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        let plain = format!("fgr_{}", hex::encode(bytes));
        let token_hash = hash_token(&plain);

        let runner = self
            .runners
            .create(NewRunner {
                name,
                token_hash,
                tags,
            })
            .await?;
        Ok((runner, plain))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeRunners;

    #[tokio::test]
    async fn registering_a_runner_stores_only_its_token_hash_and_returns_the_plaintext_once() {
        let repo = Arc::new(FakeRunners::empty());
        let use_case = RegisterRunnerUseCase::new(repo);

        let (stored, plain) = use_case
            .execute("vps-1".to_string(), vec!["docker".to_string()])
            .await
            .unwrap();

        assert!(plain.starts_with("fgr_"));
        assert_ne!(stored.token_hash, plain);
        assert_eq!(hash_token(&plain), stored.token_hash);
        assert_eq!(stored.tags, vec!["docker".to_string()]);
    }

    #[tokio::test]
    async fn two_registered_runners_have_different_plaintext_tokens() {
        let repo = Arc::new(FakeRunners::empty());
        let use_case = RegisterRunnerUseCase::new(repo);

        let (_, plain_a) = use_case.execute("a".to_string(), vec![]).await.unwrap();
        let (_, plain_b) = use_case.execute("b".to_string(), vec![]).await.unwrap();

        assert_ne!(plain_a, plain_b);
    }
}
