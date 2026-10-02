//! Sends mail through whatever `SmtpSettings` are currently configured, read fresh from the port on every send.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use ferrisgit_application::email_templates::LOGO_CID;
use ferrisgit_domain::email::{EmailPort, SmtpSecurity, SmtpSettings, SmtpSettingsPort};
use ferrisgit_domain::error::DomainError;
use lettre::message::{Attachment, Body, Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

/// E-mail-sized copy of the horizontal brand logo (440 px wide, twice the 220 px width the templates display it at).
const LOGO_BYTES: &[u8] = include_bytes!("../assets/email-logo.png");
const LOGO_CONTENT_TYPE: &str = "image/png";
const SMTP_TIMEOUT: Duration = Duration::from_secs(15);

pub struct SmtpEmailSender {
    settings: Arc<dyn SmtpSettingsPort>,
}

impl SmtpEmailSender {
    pub fn new(settings: Arc<dyn SmtpSettingsPort>) -> Self {
        Self { settings }
    }
}

/// Whether the HTML references the logo's CID, which decides if the inline attachment is added.
fn html_references_logo(html_body: &str) -> bool {
    html_body.contains(&format!("cid:{LOGO_CID}"))
}

/// Builds the MIME message (multipart/alternative: plain text + HTML related to the inline logo).
fn build_message(
    from_address: &str,
    from_name: &str,
    to: &str,
    subject: &str,
    text_body: &str,
    html_body: &str,
) -> Result<Message, DomainError> {
    let from_address: Address = from_address.parse().map_err(|_| {
        DomainError::Infrastructure(
            "l'adresse d'expédition SMTP configurée n'est pas une adresse valide".to_string(),
        )
    })?;
    // lettre adds `Date` but not `Message-ID`. Relays that don't add one (a bare in-cluster Postfix) pass the
    // mail on without it, and Gmail and others reject or spam-flag such messages.
    let message_id = format!("<{}@{}>", uuid::Uuid::new_v4(), from_address.domain());
    let from = Mailbox::new(Some(from_name.to_string()), from_address);
    let to: Mailbox = to.parse().map_err(|_| {
        DomainError::Infrastructure(
            "l'adresse du destinataire n'est pas une adresse valide".to_string(),
        )
    })?;

    let mut html_part = MultiPart::related().singlepart(SinglePart::html(html_body.to_string()));
    if html_references_logo(html_body) {
        let logo = Attachment::new_inline(LOGO_CID.to_string()).body(
            Body::new(LOGO_BYTES.to_vec()),
            LOGO_CONTENT_TYPE.parse().map_err(|_| {
                DomainError::Infrastructure("le type de contenu du logo est invalide".to_string())
            })?,
        );
        html_part = html_part.singlepart(logo);
    }
    let body = MultiPart::alternative()
        .singlepart(SinglePart::plain(text_body.to_string()))
        .multipart(html_part);

    Message::builder()
        .from(from)
        .to(to)
        .message_id(Some(message_id))
        .subject(subject)
        .multipart(body)
        .map_err(|e| {
            DomainError::Infrastructure(format!("échec de la construction de l'e-mail : {e}"))
        })
}

/// SMTP authentication is only attempted for a relay that has both a username and a password.
fn credentials_for(settings: &SmtpSettings) -> Option<Credentials> {
    match &settings.password {
        Some(password) if !settings.username.is_empty() => Some(Credentials::new(
            settings.username.clone(),
            password.clone(),
        )),
        _ => None,
    }
}

/// The transport for the configured security mode; nothing connects until `send`.
fn build_transport(
    settings: &SmtpSettings,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, DomainError> {
    let builder = match settings.security {
        SmtpSecurity::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&settings.host),
        SmtpSecurity::StartTls => {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&settings.host)
        }
        SmtpSecurity::None => Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(
            &settings.host,
        )),
    }
    .map_err(|e| {
        DomainError::Infrastructure(format!("échec de la création du transport SMTP : {e}"))
    })?;
    let builder = builder.port(settings.port).timeout(Some(SMTP_TIMEOUT));
    let builder = match credentials_for(settings) {
        Some(credentials) => builder.credentials(credentials),
        None => builder,
    };
    Ok(builder.build())
}

#[async_trait]
impl EmailPort for SmtpEmailSender {
    async fn send(
        &self,
        to: &str,
        subject: &str,
        text_body: &str,
        html_body: &str,
    ) -> Result<(), DomainError> {
        let Some(settings) = self.settings.get().await? else {
            return Err(DomainError::Infrastructure(
                "SMTP non configuré".to_string(),
            ));
        };
        let message = build_message(
            &settings.from_address,
            &settings.from_name,
            to,
            subject,
            text_body,
            html_body,
        )?;
        build_transport(&settings)?
            .send(message)
            .await
            .map_err(|e| {
                DomainError::Infrastructure(format!("échec de l'envoi de l'e-mail : {e}"))
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeSmtpSettings(Option<SmtpSettings>);

    #[async_trait]
    impl SmtpSettingsPort for FakeSmtpSettings {
        async fn get(&self) -> Result<Option<SmtpSettings>, DomainError> {
            Ok(self.0.clone())
        }
        async fn save(&self, _settings: &SmtpSettings) -> Result<(), DomainError> {
            unreachable!("not exercised by these tests")
        }
    }

    fn sample() -> SmtpSettings {
        SmtpSettings {
            host: "smtp.example.com".into(),
            port: 587,
            security: SmtpSecurity::StartTls,
            username: "mailer".into(),
            password: Some("s3cret".into()),
            from_address: "noreply@example.com".into(),
            from_name: "FerrisGit".into(),
        }
    }

    fn logo_html() -> String {
        format!("<img src=\"cid:{LOGO_CID}\">")
    }

    #[tokio::test]
    async fn an_unconfigured_smtp_is_a_clear_error() {
        let sender = SmtpEmailSender::new(Arc::new(FakeSmtpSettings(None)));

        let err = sender
            .send("to@example.com", "subject", "text", "<p>html</p>")
            .await
            .unwrap_err();

        assert!(err.to_string().contains("SMTP non configuré"), "got: {err}");
    }

    #[test]
    fn an_invalid_from_address_is_rejected() {
        let err = build_message(
            "not-an-email",
            "FerrisGit",
            "to@example.com",
            "subject",
            "text",
            "<p>html</p>",
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("adresse d'expédition"),
            "got: {err}"
        );
    }

    #[test]
    fn an_invalid_recipient_is_rejected() {
        let err = build_message(
            "from@example.com",
            "FerrisGit",
            "not-an-email",
            "subject",
            "text",
            "<p>html</p>",
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("adresse du destinataire"),
            "got: {err}"
        );
    }

    #[test]
    fn the_message_carries_both_bodies_and_the_inline_logo() {
        let message = build_message(
            "from@example.com",
            "FerrisGit",
            "to@example.com",
            "subject",
            "plain text",
            &logo_html(),
        )
        .unwrap();

        let formatted = String::from_utf8_lossy(&message.formatted()).to_string();
        assert!(formatted.contains("text/plain"), "got:\n{formatted}");
        assert!(formatted.contains("text/html"), "got:\n{formatted}");
        assert!(formatted.contains("image/png"), "got:\n{formatted}");
        assert!(
            formatted.contains(&format!("Content-ID: <{LOGO_CID}>")),
            "got:\n{formatted}"
        );
        assert!(
            formatted.contains("Content-Disposition: inline"),
            "got:\n{formatted}"
        );
    }

    #[test]
    fn every_message_gets_a_unique_message_id_on_the_senders_domain() {
        let ids: Vec<String> = (0..2)
            .map(|_| {
                let message = build_message(
                    "from@example.com",
                    "FerrisGit",
                    "to@example.com",
                    "subject",
                    "text",
                    "<p>html</p>",
                )
                .unwrap();
                let formatted = String::from_utf8_lossy(&message.formatted()).to_string();
                assert!(formatted.contains("Message-ID: <"), "got:\n{formatted}");
                assert!(formatted.contains("@example.com>"), "got:\n{formatted}");
                formatted
                    .lines()
                    .find(|l| l.starts_with("Message-ID:"))
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_ne!(ids[0], ids[1]);
    }

    #[test]
    fn no_logo_is_attached_when_the_html_does_not_reference_it() {
        let message = build_message(
            "from@example.com",
            "FerrisGit",
            "to@example.com",
            "subject",
            "text",
            "<p>no logo here</p>",
        )
        .unwrap();

        let formatted = String::from_utf8_lossy(&message.formatted()).to_string();
        assert!(formatted.contains("text/html"), "got:\n{formatted}");
        assert!(!formatted.contains("image/png"), "got:\n{formatted}");
        assert!(!formatted.contains(LOGO_CID), "got:\n{formatted}");
    }

    #[test]
    fn the_embedded_logo_is_a_png_of_a_reasonable_size() {
        assert!(LOGO_BYTES.starts_with(&[0x89, b'P', b'N', b'G']));
        assert!(
            LOGO_BYTES.len() < 80 * 1024,
            "logo is {} bytes",
            LOGO_BYTES.len()
        );
    }

    #[tokio::test]
    async fn a_transport_can_be_built_for_every_security_mode() {
        for security in [
            SmtpSecurity::None,
            SmtpSecurity::StartTls,
            SmtpSecurity::Tls,
        ] {
            for (username, password) in [("mailer", Some("s3cret")), ("", None)] {
                let settings = SmtpSettings {
                    security,
                    username: username.into(),
                    password: password.map(str::to_string),
                    ..sample()
                };
                build_transport(&settings)
                    .unwrap_or_else(|e| panic!("{security:?} / user {username:?}: {e}"));
            }
        }
    }

    #[test]
    fn credentials_are_only_used_when_a_username_is_set() {
        assert!(credentials_for(&sample()).is_some());
        assert!(
            credentials_for(&SmtpSettings {
                username: String::new(),
                ..sample()
            })
            .is_none()
        );
        assert!(
            credentials_for(&SmtpSettings {
                password: None,
                ..sample()
            })
            .is_none()
        );
    }
}
