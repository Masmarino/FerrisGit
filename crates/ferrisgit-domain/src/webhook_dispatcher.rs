use async_trait::async_trait;
use uuid::Uuid;

use crate::error::DomainError;
use crate::webhook_event::WebhookEvent;

#[async_trait]
pub trait WebhookDispatcherPort: Send + Sync {
    /// Fire-and-forget: a delivery failure must never fail the triggering action. Delivers to every active webhook
    /// subscribed to `event.kind()` independently, without waiting on the HTTP calls.
    async fn dispatch(&self, repository_id: Uuid, event: WebhookEvent) -> Result<(), DomainError>;
}
