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
        // `None` (the field cleared) means "no retention" / "no ceiling"; a value must be a positive count.
        if let Some(Some(days)) = update.log_retention_days
            && days < 1
        {
            return Err(DomainError::Validation(
                "log_retention_days must be at least 1".to_string(),
            ));
        }
        if let Some(Some(jobs)) = update.max_concurrent_jobs
            && jobs < 1
        {
            return Err(DomainError::Validation(
                "max_concurrent_jobs must be at least 1".to_string(),
            ));
        }
        // An empty token would let anyone register a runner by sending nothing.
        if let Some(Some(token)) = &update.runner_registration_token
            && token.trim().is_empty()
        {
            return Err(DomainError::Validation(
                "runner_registration_token must not be empty".to_string(),
            ));
        }
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

    #[tokio::test]
    async fn updating_jwt_ttl_hours_immediately_pushes_the_new_value_to_the_token_issuer() {
        let system_settings = Arc::new(FakeSystemSettings::default());
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
        let system_settings = Arc::new(FakeSystemSettings::default());
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
        let system_settings = Arc::new(FakeSystemSettings::default());
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

    #[tokio::test]
    async fn log_retention_days_and_max_concurrent_jobs_must_be_positive() {
        for update in [
            SystemSettingsUpdate {
                log_retention_days: Some(Some(0)),
                ..Default::default()
            },
            SystemSettingsUpdate {
                log_retention_days: Some(Some(-3)),
                ..Default::default()
            },
            SystemSettingsUpdate {
                max_concurrent_jobs: Some(Some(0)),
                ..Default::default()
            },
        ] {
            let use_case = UpdateSystemSettingsUseCase::new(
                Arc::new(FakeSystemSettings::default()),
                Arc::new(FakeTokenIssuer::default()),
            );
            assert!(matches!(
                use_case.execute(update).await,
                Err(DomainError::Validation(_))
            ));
        }
    }

    #[tokio::test]
    async fn a_positive_value_is_stored_and_none_clears_the_field() {
        let use_case = UpdateSystemSettingsUseCase::new(
            Arc::new(FakeSystemSettings::default()),
            Arc::new(FakeTokenIssuer::default()),
        );

        let set = use_case
            .execute(SystemSettingsUpdate {
                log_retention_days: Some(Some(30)),
                max_concurrent_jobs: Some(Some(4)),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(set.log_retention_days, Some(30));
        assert_eq!(set.max_concurrent_jobs, Some(4));

        let cleared = use_case
            .execute(SystemSettingsUpdate {
                log_retention_days: Some(None),
                max_concurrent_jobs: Some(None),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(cleared.log_retention_days, None);
        assert_eq!(cleared.max_concurrent_jobs, None);
    }

    #[tokio::test]
    async fn an_empty_runner_registration_token_is_refused_but_none_removes_it() {
        let use_case = UpdateSystemSettingsUseCase::new(
            Arc::new(FakeSystemSettings::default()),
            Arc::new(FakeTokenIssuer::default()),
        );
        use_case
            .execute(SystemSettingsUpdate {
                runner_registration_token: Some(Some("a-secret".to_string())),
                ..Default::default()
            })
            .await
            .unwrap();

        for empty in ["", "   "] {
            assert!(matches!(
                use_case
                    .execute(SystemSettingsUpdate {
                        runner_registration_token: Some(Some(empty.to_string())),
                        ..Default::default()
                    })
                    .await,
                Err(DomainError::Validation(_))
            ));
        }

        let removed = use_case
            .execute(SystemSettingsUpdate {
                runner_registration_token: Some(None),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(removed.runner_registration_token, None);
    }
}
