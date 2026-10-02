use async_trait::async_trait;

use crate::error::DomainError;

/// Admin switch for open self-registration. Off until an admin turns it on.
#[async_trait]
pub trait RegistrationSettingsPort: Send + Sync {
    async fn is_enabled(&self) -> Result<bool, DomainError>;
    async fn set_enabled(&self, enabled: bool) -> Result<(), DomainError>;
}
