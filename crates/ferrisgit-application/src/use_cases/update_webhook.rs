use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::webhook::{Webhook, WebhookStorePort, WebhookUpdate};
use uuid::Uuid;

use crate::use_cases::create_webhook::{validate_event_kinds, validate_webhook_url};

pub struct UpdateWebhookUseCase {
    webhooks: Arc<dyn WebhookStorePort>,
}

impl UpdateWebhookUseCase {
    pub fn new(webhooks: Arc<dyn WebhookStorePort>) -> Self {
        Self { webhooks }
    }

    pub async fn execute(
        &self,
        id: Uuid,
        repository_id: Uuid,
        update: WebhookUpdate,
    ) -> Result<Webhook, DomainError> {
        if let Some(url) = &update.url {
            validate_webhook_url(url).await?;
        }
        if let Some(events) = &update.events {
            validate_event_kinds(events)?;
        }
        self.webhooks.update(id, repository_id, update).await
    }
}

pub struct DeleteWebhookUseCase {
    webhooks: Arc<dyn WebhookStorePort>,
}

impl DeleteWebhookUseCase {
    pub fn new(webhooks: Arc<dyn WebhookStorePort>) -> Self {
        Self { webhooks }
    }

    pub async fn execute(&self, id: Uuid, repository_id: Uuid) -> Result<(), DomainError> {
        self.webhooks.delete(id, repository_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeWebhookStore;
    use chrono::Utc;

    fn seed(repository_id: Uuid) -> Webhook {
        Webhook {
            id: Uuid::new_v4(),
            repository_id,
            url: "https://example.com/hook".to_string(),
            events: vec!["issue_closed".to_string()],
            active: true,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn updates_the_url_when_it_is_a_valid_non_private_address() {
        let repository_id = Uuid::new_v4();
        let webhook = seed(repository_id);
        let webhooks = Arc::new(FakeWebhookStore::new(vec![webhook.clone()]));
        let use_case = UpdateWebhookUseCase::new(webhooks);

        let updated = use_case
            .execute(
                webhook.id,
                repository_id,
                WebhookUpdate {
                    url: Some("https://example.org/new-hook".to_string()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(updated.url, "https://example.org/new-hook");
    }

    #[tokio::test]
    async fn rejects_updating_the_url_to_a_private_address() {
        let repository_id = Uuid::new_v4();
        let webhook = seed(repository_id);
        let webhooks = Arc::new(FakeWebhookStore::new(vec![webhook.clone()]));
        let use_case = UpdateWebhookUseCase::new(webhooks);

        let result = use_case
            .execute(
                webhook.id,
                repository_id,
                WebhookUpdate {
                    url: Some("http://127.0.0.1/hook".to_string()),
                    ..Default::default()
                },
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn rejects_updating_events_to_an_unknown_kind() {
        let repository_id = Uuid::new_v4();
        let webhook = seed(repository_id);
        let webhooks = Arc::new(FakeWebhookStore::new(vec![webhook.clone()]));
        let use_case = UpdateWebhookUseCase::new(webhooks);

        let result = use_case
            .execute(
                webhook.id,
                repository_id,
                WebhookUpdate {
                    events: Some(vec!["totally_made_up".to_string()]),
                    ..Default::default()
                },
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn deleting_removes_the_webhook() {
        let repository_id = Uuid::new_v4();
        let webhook = seed(repository_id);
        let webhooks = Arc::new(FakeWebhookStore::new(vec![webhook.clone()]));
        let use_case = DeleteWebhookUseCase::new(webhooks.clone());

        use_case.execute(webhook.id, repository_id).await.unwrap();

        assert!(webhooks.snapshot().is_empty());
    }
}
