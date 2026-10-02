use std::sync::Arc;

use ferrisgit_domain::email::{SmtpSecurity, SmtpSettings, SmtpSettingsPort, is_valid_mailbox};
use ferrisgit_domain::error::DomainError;

use crate::email_templates;
use crate::mailer::Mailer;

pub struct SmtpSettingsInput {
    pub host: String,
    pub port: i32,
    pub security: String,
    pub username: String,
    pub password: Option<String>,
    pub from_address: String,
    pub from_name: String,
}

pub struct UpdateSmtpSettingsUseCase {
    settings: Arc<dyn SmtpSettingsPort>,
}

impl UpdateSmtpSettingsUseCase {
    pub fn new(settings: Arc<dyn SmtpSettingsPort>) -> Self {
        Self { settings }
    }

    pub async fn execute(&self, input: SmtpSettingsInput) -> Result<SmtpSettings, DomainError> {
        let host = input.host.trim().to_string();
        if host.is_empty()
            || host.len() > 253
            || host.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(DomainError::Validation(
                "host must be a hostname or an IP address".to_string(),
            ));
        }
        let port = u16::try_from(input.port)
            .ok()
            .filter(|p| *p >= 1)
            .ok_or_else(|| {
                DomainError::Validation("port must be between 1 and 65535".to_string())
            })?;
        let security = SmtpSecurity::parse(&input.security)?;
        let from_address = input.from_address.trim().to_string();
        if !is_valid_mailbox(&from_address) {
            return Err(DomainError::Validation(
                "from_address must be a valid e-mail address".to_string(),
            ));
        }
        let from_name = match input.from_name.trim() {
            "" => "FerrisGit".to_string(),
            name if name.chars().count() > 100 || name.chars().any(char::is_control) => {
                return Err(DomainError::Validation("from_name is too long".to_string()));
            }
            name => name.to_string(),
        };
        let username = input.username.trim().to_string();
        // Only read the stored row when its password is needed, so an unreadable row can't block a fresh save.
        let password = match input.password {
            _ if username.is_empty() => None,
            Some(p) if !p.is_empty() => Some(p),
            _ => self.settings.get().await?.and_then(|e| e.password),
        };
        if !username.is_empty() && password.is_none() {
            return Err(DomainError::Validation(
                "a password is required when a username is set".to_string(),
            ));
        }
        let settings = SmtpSettings {
            host,
            port,
            security,
            username,
            password,
            from_address,
            from_name,
        };
        self.settings.save(&settings).await?;
        Ok(settings)
    }
}

pub struct SendTestEmailUseCase {
    mailer: Arc<Mailer>,
}

impl SendTestEmailUseCase {
    pub fn new(mailer: Arc<Mailer>) -> Self {
        Self { mailer }
    }

    pub async fn execute(&self, to: &str) -> Result<(), DomainError> {
        let to = to.trim();
        if !is_valid_mailbox(to) {
            return Err(DomainError::Validation(
                "recipient must be a valid e-mail address".to_string(),
            ));
        }
        self.mailer.send(to, email_templates::smtp_test()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use ferrisgit_domain::email::EmailPort;

    #[derive(Default)]
    struct FakeSmtpSettings {
        stored: Mutex<Option<SmtpSettings>>,
    }

    #[async_trait]
    impl SmtpSettingsPort for FakeSmtpSettings {
        async fn get(&self) -> Result<Option<SmtpSettings>, DomainError> {
            Ok(self.stored.lock().unwrap().clone())
        }
        async fn save(&self, settings: &SmtpSettings) -> Result<(), DomainError> {
            *self.stored.lock().unwrap() = Some(settings.clone());
            Ok(())
        }
    }

    /// The stored row can't be read (say it was encrypted under another key): `get` fails, `save` works.
    #[derive(Default)]
    struct UnreadableSmtpSettings {
        saved: Mutex<Option<SmtpSettings>>,
        get_calls: Mutex<u32>,
    }

    #[async_trait]
    impl SmtpSettingsPort for UnreadableSmtpSettings {
        async fn get(&self) -> Result<Option<SmtpSettings>, DomainError> {
            *self.get_calls.lock().unwrap() += 1;
            Err(DomainError::Infrastructure("cannot decrypt".to_string()))
        }
        async fn save(&self, settings: &SmtpSettings) -> Result<(), DomainError> {
            *self.saved.lock().unwrap() = Some(settings.clone());
            Ok(())
        }
    }

    #[derive(Default)]
    struct Recording {
        sent: Mutex<Vec<(String, String)>>,
    }

    #[async_trait]
    impl EmailPort for Recording {
        async fn send(
            &self,
            to: &str,
            subject: &str,
            _text: &str,
            _html: &str,
        ) -> Result<(), DomainError> {
            self.sent
                .lock()
                .unwrap()
                .push((to.to_string(), subject.to_string()));
            Ok(())
        }
    }

    struct Failing;

    #[async_trait]
    impl EmailPort for Failing {
        async fn send(
            &self,
            _to: &str,
            _subject: &str,
            _text: &str,
            _html: &str,
        ) -> Result<(), DomainError> {
            Err(DomainError::Infrastructure(
                "connection refused".to_string(),
            ))
        }
    }

    fn input() -> SmtpSettingsInput {
        SmtpSettingsInput {
            host: "smtp.example.com".into(),
            port: 587,
            security: "starttls".into(),
            username: "mailer".into(),
            password: Some("secret".into()),
            from_address: "noreply@example.com".into(),
            from_name: "Ma forge".into(),
        }
    }

    fn use_case() -> (UpdateSmtpSettingsUseCase, Arc<FakeSmtpSettings>) {
        let store = Arc::new(FakeSmtpSettings::default());
        (UpdateSmtpSettingsUseCase::new(store.clone()), store)
    }

    #[tokio::test]
    async fn valid_input_is_saved_and_returned() {
        let (uc, store) = use_case();
        let saved = uc.execute(input()).await.unwrap();
        assert_eq!(saved.host, "smtp.example.com");
        assert_eq!(saved.port, 587);
        assert_eq!(saved.security, SmtpSecurity::StartTls);
        assert_eq!(saved.password.as_deref(), Some("secret"));
        assert_eq!(store.stored.lock().unwrap().clone().unwrap(), saved);
    }

    #[tokio::test]
    async fn invalid_fields_are_validation_errors() {
        let (uc, store) = use_case();
        let cases: Vec<SmtpSettingsInput> = vec![
            SmtpSettingsInput {
                host: "   ".into(),
                ..input()
            },
            SmtpSettingsInput {
                host: "bad host".into(),
                ..input()
            },
            SmtpSettingsInput { port: 0, ..input() },
            SmtpSettingsInput {
                port: 70000,
                ..input()
            },
            SmtpSettingsInput {
                port: -1,
                ..input()
            },
            SmtpSettingsInput {
                security: "ssl".into(),
                ..input()
            },
            SmtpSettingsInput {
                from_address: "not-an-email".into(),
                ..input()
            },
            SmtpSettingsInput {
                host: "a".repeat(254),
                ..input()
            },
            SmtpSettingsInput {
                from_name: "x".repeat(101),
                ..input()
            },
            SmtpSettingsInput {
                from_name: "bad\nname".into(),
                ..input()
            },
        ];
        for case in cases {
            assert!(matches!(
                uc.execute(case).await,
                Err(DomainError::Validation(_))
            ));
        }
        assert!(
            store.stored.lock().unwrap().is_none(),
            "nothing may be saved on validation failure"
        );
    }

    #[tokio::test]
    async fn a_whitespace_only_username_means_no_authentication() {
        let (uc, _) = use_case();
        let saved = uc
            .execute(SmtpSettingsInput {
                username: "   ".into(),
                password: None,
                ..input()
            })
            .await
            .unwrap();
        assert_eq!(saved.username, "");
        assert_eq!(saved.password, None);
    }

    #[tokio::test]
    async fn an_empty_from_name_defaults_to_ferrisgit() {
        let (uc, _) = use_case();
        let saved = uc
            .execute(SmtpSettingsInput {
                from_name: "  ".into(),
                ..input()
            })
            .await
            .unwrap();
        assert_eq!(saved.from_name, "FerrisGit");
    }

    #[tokio::test]
    async fn a_missing_or_empty_password_keeps_the_stored_one() {
        let (uc, store) = use_case();
        uc.execute(input()).await.unwrap();
        uc.execute(SmtpSettingsInput {
            password: None,
            port: 465,
            ..input()
        })
        .await
        .unwrap();
        assert_eq!(
            store
                .stored
                .lock()
                .unwrap()
                .clone()
                .unwrap()
                .password
                .as_deref(),
            Some("secret")
        );
        uc.execute(SmtpSettingsInput {
            password: Some(String::new()),
            ..input()
        })
        .await
        .unwrap();
        assert_eq!(
            store
                .stored
                .lock()
                .unwrap()
                .clone()
                .unwrap()
                .password
                .as_deref(),
            Some("secret")
        );
    }

    #[tokio::test]
    async fn a_new_password_replaces_the_stored_one() {
        let (uc, store) = use_case();
        uc.execute(input()).await.unwrap();
        uc.execute(SmtpSettingsInput {
            password: Some("new".into()),
            ..input()
        })
        .await
        .unwrap();
        assert_eq!(
            store
                .stored
                .lock()
                .unwrap()
                .clone()
                .unwrap()
                .password
                .as_deref(),
            Some("new")
        );
    }

    #[tokio::test]
    async fn an_empty_username_clears_the_password_and_needs_none() {
        let (uc, store) = use_case();
        uc.execute(input()).await.unwrap();
        let saved = uc
            .execute(SmtpSettingsInput {
                username: String::new(),
                password: None,
                ..input()
            })
            .await
            .unwrap();
        assert_eq!(saved.username, "");
        assert_eq!(saved.password, None);
        assert_eq!(store.stored.lock().unwrap().clone().unwrap().password, None);
    }

    #[tokio::test]
    async fn typing_a_new_password_never_reads_the_stored_row() {
        let store = Arc::new(UnreadableSmtpSettings::default());
        let uc = UpdateSmtpSettingsUseCase::new(store.clone());
        let saved = uc.execute(input()).await.unwrap();
        assert_eq!(saved.password.as_deref(), Some("secret"));
        assert_eq!(*store.get_calls.lock().unwrap(), 0);
        assert_eq!(store.saved.lock().unwrap().clone().unwrap(), saved);
    }

    #[tokio::test]
    async fn an_empty_username_never_reads_the_stored_row() {
        let store = Arc::new(UnreadableSmtpSettings::default());
        let uc = UpdateSmtpSettingsUseCase::new(store.clone());
        let saved = uc
            .execute(SmtpSettingsInput {
                username: String::new(),
                password: None,
                ..input()
            })
            .await
            .unwrap();
        assert_eq!(saved.password, None);
        assert_eq!(*store.get_calls.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn a_stored_row_whose_password_could_not_be_decrypted_asks_for_the_password_again() {
        let (uc, store) = use_case();
        *store.stored.lock().unwrap() = Some(SmtpSettings {
            password: None,
            ..uc.execute(input()).await.unwrap()
        });
        let result = uc
            .execute(SmtpSettingsInput {
                password: None,
                ..input()
            })
            .await;
        assert!(matches!(result, Err(DomainError::Validation(_))));
        let saved = uc
            .execute(SmtpSettingsInput {
                password: Some("fresh".into()),
                ..input()
            })
            .await
            .unwrap();
        assert_eq!(saved.password.as_deref(), Some("fresh"));
    }

    #[tokio::test]
    async fn a_username_without_any_password_is_a_validation_error() {
        let (uc, _) = use_case();
        let result = uc
            .execute(SmtpSettingsInput {
                password: None,
                ..input()
            })
            .await;
        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn the_test_email_rejects_an_invalid_recipient_and_sends_nothing() {
        let email = Arc::new(Recording::default());
        let uc = SendTestEmailUseCase::new(Arc::new(Mailer::new(email.clone())));
        assert!(matches!(
            uc.execute("nope").await,
            Err(DomainError::Validation(_))
        ));
        assert!(email.sent.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_test_email_is_sent_to_a_valid_recipient() {
        let email = Arc::new(Recording::default());
        let uc = SendTestEmailUseCase::new(Arc::new(Mailer::new(email.clone())));
        uc.execute(" admin@example.com ").await.unwrap();
        assert_eq!(
            email.sent.lock().unwrap().as_slice(),
            &[(
                "admin@example.com".to_string(),
                "E-mail de test FerrisGit".to_string()
            )]
        );
    }

    #[tokio::test]
    async fn a_delivery_failure_surfaces_as_the_underlying_error() {
        let uc = SendTestEmailUseCase::new(Arc::new(Mailer::new(Arc::new(Failing))));
        let result = uc.execute("admin@example.com").await;
        assert!(matches!(result, Err(DomainError::Infrastructure(_))));
    }
}
