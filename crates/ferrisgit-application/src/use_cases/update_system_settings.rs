use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::settings::{SystemSettings, SystemSettingsStorePort, SystemSettingsUpdate};
use ferrisgit_domain::user::TokenIssuerPort;

use crate::token_hash::hash_token;

pub struct UpdateSystemSettingsUseCase {
    system_settings: Arc<dyn SystemSettingsStorePort>,
    token_issuer: Arc<dyn TokenIssuerPort>,
}

impl UpdateSystemSettingsUseCase {
    pub fn new(
        system_settings: Arc<dyn SystemSettingsStorePort>,
        token_issuer: Arc<dyn TokenIssuerPort>,
    ) -> Self {
        Self {
            system_settings,
            token_issuer,
        }
    }

    pub async fn execute(
        &self,
        mut update: SystemSettingsUpdate,
    ) -> Result<SystemSettings, DomainError> {
        // Stored like API and runner tokens (a SHA-256 hash, never the plaintext). Anyone who has this value can
        // register a runner, which in turn can claim jobs and read every repository's decrypted CI variables.
        update.runner_registration_token = update
            .runner_registration_token
            .map(|token| token.map(|t| hash_token(&t)));
        let settings = self.system_settings.update(update).await?;
        self.token_issuer
            .set_ttl_hours(settings.jwt_ttl_hours as i64);
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeSystemSettings;
    use ferrisgit_domain::settings::ExecutionEngine;
    use std::sync::Mutex;
    use uuid::Uuid;

    #[derive(Default)]
    struct FakeTokenIssuer(Mutex<Option<i64>>);
    impl TokenIssuerPort for FakeTokenIssuer {
        fn issue(&self, _user_id: Uuid, _token_epoch: i32) -> Result<String, DomainError> {
            unimplemented!()
        }
        fn verify(&self, _token: &str) -> Result<(Uuid, i32), DomainError> {
            unimplemented!()
        }
        fn set_ttl_hours(&self, hours: i64) {
            *self.0.lock().unwrap() = Some(hours);
        }
    }

    fn default_settings() -> SystemSettings {
        SystemSettings {
            execution_engine: ExecutionEngine::DockerRunners,
            k8s_namespace: None,
            k8s_cache_storage_class: None,
            runner_registration_token: None,
            log_retention_days: None,
            max_concurrent_jobs: None,
            jwt_ttl_hours: 12,
            max_push_size_mb: 500,
        }
    }

    #[tokio::test]
    async fn updating_jwt_ttl_hours_immediately_pushes_the_new_value_to_the_token_issuer() {
        let system_settings = Arc::new(FakeSystemSettings::new(default_settings()));
        let token_issuer = Arc::new(FakeTokenIssuer::default());
        let use_case = UpdateSystemSettingsUseCase::new(system_settings, token_issuer.clone());

        use_case
            .execute(SystemSettingsUpdate {
                jwt_ttl_hours: Some(6),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(*token_issuer.0.lock().unwrap(), Some(6));
    }

    #[tokio::test]
    async fn updating_an_unrelated_field_still_pushes_the_current_ttl_so_the_issuer_never_drifts() {
        let system_settings = Arc::new(FakeSystemSettings::new(default_settings()));
        let token_issuer = Arc::new(FakeTokenIssuer::default());
        let use_case = UpdateSystemSettingsUseCase::new(system_settings, token_issuer.clone());

        use_case
            .execute(SystemSettingsUpdate {
                execution_engine: Some(ExecutionEngine::Kubernetes),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(
            *token_issuer.0.lock().unwrap(),
            Some(12),
            "even an unrelated update must re-push the current ttl_hours value"
        );
    }

    #[tokio::test]
    async fn the_runner_registration_token_is_stored_hashed_not_in_plaintext() {
        let system_settings = Arc::new(FakeSystemSettings::new(default_settings()));
        let token_issuer = Arc::new(FakeTokenIssuer::default());
        let use_case = UpdateSystemSettingsUseCase::new(system_settings, token_issuer);

        let settings = use_case
            .execute(SystemSettingsUpdate {
                runner_registration_token: Some(Some("shared-secret-123".to_string())),
                ..Default::default()
            })
            .await
            .unwrap();

        let stored = settings.runner_registration_token.unwrap();
        assert_ne!(
            stored, "shared-secret-123",
            "the plaintext registration token must never be persisted"
        );
        assert_eq!(stored, hash_token("shared-secret-123"));
    }
}
