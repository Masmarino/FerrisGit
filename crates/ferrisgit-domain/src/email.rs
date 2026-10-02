use async_trait::async_trait;

use crate::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtpSecurity {
    None,
    StartTls,
    Tls,
}

impl SmtpSecurity {
    pub fn as_str(self) -> &'static str {
        match self {
            SmtpSecurity::None => "none",
            SmtpSecurity::StartTls => "starttls",
            SmtpSecurity::Tls => "tls",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "none" => Ok(SmtpSecurity::None),
            "starttls" => Ok(SmtpSecurity::StartTls),
            "tls" => Ok(SmtpSecurity::Tls),
            other => Err(DomainError::Validation(format!(
                "unknown SMTP security mode: {other}"
            ))),
        }
    }
}

/// Kept apart from `SystemSettings` so the password never rides along in a row that is echoed over HTTP.
#[derive(Clone, PartialEq, Eq)]
pub struct SmtpSettings {
    pub host: String,
    pub port: u16,
    pub security: SmtpSecurity,
    /// Empty means "no SMTP authentication" (an internal relay).
    pub username: String,
    /// Plaintext at this layer. Encryption at rest is the persistence adapter's job.
    pub password: Option<String>,
    pub from_address: String,
    pub from_name: String,
}

impl std::fmt::Debug for SmtpSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmtpSettings")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("security", &self.security)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "[redacted]"))
            .field("from_address", &self.from_address)
            .field("from_name", &self.from_name)
            .finish()
    }
}

/// Kept small on purpose: length, no whitespace or control characters, one `@`, a dotted domain. Rejects
/// `admin@localhost` (an address no relay can deliver to) as well as anything malformed.
pub fn is_valid_mailbox(address: &str) -> bool {
    address.len() <= 254
        && !address.chars().any(|c| c.is_whitespace() || c.is_control())
        && address.matches('@').count() == 1
        && address.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        })
}

#[async_trait]
pub trait SmtpSettingsPort: Send + Sync {
    /// `None` means never configured: mail is simply unavailable, which is not an error in itself.
    async fn get(&self) -> Result<Option<SmtpSettings>, DomainError>;
    async fn save(&self, settings: &SmtpSettings) -> Result<(), DomainError>;
}

#[async_trait]
pub trait EmailPort: Send + Sync {
    async fn send(
        &self,
        to: &str,
        subject: &str,
        text_body: &str,
        html_body: &str,
    ) -> Result<(), DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DomainError;

    fn sample() -> SmtpSettings {
        SmtpSettings {
            host: "smtp.example.com".into(),
            port: 587,
            security: SmtpSecurity::StartTls,
            username: "mailer".into(),
            password: Some("smtp-password-value".into()),
            from_address: "noreply@example.com".into(),
            from_name: "FerrisGit".into(),
        }
    }

    #[test]
    fn debug_output_never_contains_the_smtp_password() {
        let printed = format!("{:?}", sample());
        assert!(!printed.contains("smtp-password-value"), "got: {printed}");
        assert!(printed.contains("mailer"));
        assert!(printed.contains("[redacted]"));
    }

    #[test]
    fn security_round_trips_through_its_string_form() {
        for mode in [
            SmtpSecurity::None,
            SmtpSecurity::StartTls,
            SmtpSecurity::Tls,
        ] {
            assert_eq!(SmtpSecurity::parse(mode.as_str()).unwrap(), mode);
        }
    }

    #[test]
    fn an_unknown_security_mode_is_a_validation_error() {
        assert!(matches!(
            SmtpSecurity::parse("ssl"),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn ordinary_addresses_are_valid_mailboxes() {
        for address in [
            "x@example.com",
            "first.last+tag@mail.example.co.uk",
            "a@b.c",
        ] {
            assert!(is_valid_mailbox(address), "{address} should be valid");
        }
    }

    #[test]
    fn malformed_or_undeliverable_addresses_are_not_valid_mailboxes() {
        let too_long = format!("{}@example.com", "a".repeat(250));
        for address in [
            "",
            "nope",
            "admin@localhost",
            "@example.com",
            "a@@example.com",
            "a@b@example.com",
            "a@.example.com",
            "a@example.com.",
            "a b@example.com",
            "a@exa\nmple.com",
            "a@example.com\r\nBcc: x@y.z",
            too_long.as_str(),
        ] {
            assert!(!is_valid_mailbox(address), "{address:?} should be invalid");
        }
    }
}
