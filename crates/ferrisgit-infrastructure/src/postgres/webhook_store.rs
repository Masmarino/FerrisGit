use crate::error::infra;
use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::secret_encryption::SecretEncryptorPort;
use ferrisgit_domain::webhook::{
    NewWebhook, NewWebhookDelivery, Webhook, WebhookDelivery, WebhookStorePort, WebhookUpdate,
};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

pub struct PostgresWebhookStore {
    pool: PgPool,
    encryptor: Arc<dyn SecretEncryptorPort>,
}

impl PostgresWebhookStore {
    pub fn new(pool: PgPool, encryptor: Arc<dyn SecretEncryptorPort>) -> Self {
        Self { pool, encryptor }
    }
}

struct Row {
    id: Uuid,
    repository_id: Uuid,
    url: String,
    events: Vec<String>,
    active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<Row> for Webhook {
    fn from(row: Row) -> Self {
        Webhook {
            id: row.id,
            repository_id: row.repository_id,
            url: row.url,
            events: row.events,
            active: row.active,
            created_at: row.created_at,
        }
    }
}

struct DeliveryRow {
    id: Uuid,
    webhook_id: Uuid,
    event_kind: String,
    http_status: Option<i32>,
    success: bool,
    error_message: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<DeliveryRow> for WebhookDelivery {
    fn from(row: DeliveryRow) -> Self {
        WebhookDelivery {
            id: row.id,
            webhook_id: row.webhook_id,
            event_kind: row.event_kind,
            http_status: row.http_status,
            success: row.success,
            error_message: row.error_message,
            created_at: row.created_at,
        }
    }
}

#[async_trait]
impl WebhookStorePort for PostgresWebhookStore {
    async fn create(&self, new_webhook: NewWebhook) -> Result<Webhook, DomainError> {
        let secret_ciphertext = self.encryptor.encrypt(&new_webhook.secret_plaintext)?;
        let row = sqlx::query_as!(
            Row,
            "INSERT INTO webhooks (repository_id, url, secret_ciphertext, events) VALUES ($1, $2, $3, $4) \
             RETURNING id, repository_id, url, events, active, created_at",
            new_webhook.repository_id,
            new_webhook.url,
            secret_ciphertext,
            &new_webhook.events,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(infra)?;
        Ok(row.into())
    }

    async fn list_for_repository(&self, repository_id: Uuid) -> Result<Vec<Webhook>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT id, repository_id, url, events, active, created_at FROM webhooks WHERE repository_id = $1 ORDER BY created_at DESC",
            repository_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Webhook>, DomainError> {
        let row = sqlx::query_as!(
            Row,
            "SELECT id, repository_id, url, events, active, created_at FROM webhooks WHERE id = $1",
            id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;
        Ok(row.map(Into::into))
    }

    async fn update(
        &self,
        id: Uuid,
        repository_id: Uuid,
        update: WebhookUpdate,
    ) -> Result<Webhook, DomainError> {
        let secret_ciphertext = match &update.secret_plaintext {
            Some(plaintext) => Some(self.encryptor.encrypt(plaintext)?),
            None => None,
        };
        let row = sqlx::query_as!(
            Row,
            "UPDATE webhooks SET \
             url = COALESCE($1, url), \
             secret_ciphertext = COALESCE($2, secret_ciphertext), \
             events = COALESCE($3, events), \
             active = COALESCE($4, active) \
             WHERE id = $5 AND repository_id = $6 \
             RETURNING id, repository_id, url, events, active, created_at",
            update.url,
            secret_ciphertext,
            update.events.as_deref(),
            update.active,
            id,
            repository_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?
        .ok_or_else(|| DomainError::NotFound("webhook".to_string()))?;
        Ok(row.into())
    }

    async fn delete(&self, id: Uuid, repository_id: Uuid) -> Result<(), DomainError> {
        let result = sqlx::query!(
            "DELETE FROM webhooks WHERE id = $1 AND repository_id = $2",
            id,
            repository_id
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound("webhook".to_string()));
        }
        Ok(())
    }

    async fn list_active_for_event(
        &self,
        repository_id: Uuid,
        event_kind: &str,
    ) -> Result<Vec<Webhook>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT id, repository_id, url, events, active, created_at FROM webhooks \
             WHERE repository_id = $1 AND active = TRUE AND $2 = ANY(events)",
            repository_id,
            event_kind,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn resolve_secret_plaintext(&self, webhook_id: Uuid) -> Result<String, DomainError> {
        let ciphertext = sqlx::query_scalar!(
            "SELECT secret_ciphertext FROM webhooks WHERE id = $1",
            webhook_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?
        .ok_or_else(|| DomainError::NotFound("webhook".to_string()))?;
        self.encryptor.decrypt(&ciphertext)
    }

    async fn record_delivery(&self, delivery: NewWebhookDelivery) -> Result<(), DomainError> {
        sqlx::query!(
            "INSERT INTO webhook_deliveries (webhook_id, event_kind, http_status, success, error_message) VALUES ($1, $2, $3, $4, $5)",
            delivery.webhook_id,
            delivery.event_kind,
            delivery.http_status,
            delivery.success,
            delivery.error_message,
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }

    async fn list_deliveries(
        &self,
        webhook_id: Uuid,
        repository_id: Uuid,
        limit: i64,
    ) -> Result<Vec<WebhookDelivery>, DomainError> {
        let rows = sqlx::query_as!(
            DeliveryRow,
            "SELECT wd.id, wd.webhook_id, wd.event_kind, wd.http_status, wd.success, wd.error_message, wd.created_at \
             FROM webhook_deliveries wd \
             JOIN webhooks w ON w.id = wd.webhook_id \
             WHERE wd.webhook_id = $1 AND w.repository_id = $2 \
             ORDER BY wd.created_at DESC LIMIT $3",
            webhook_id,
            repository_id,
            limit,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        if rows.is_empty()
            && self
                .find_by_id(webhook_id)
                .await?
                .filter(|w| w.repository_id == repository_id)
                .is_none()
        {
            return Err(DomainError::NotFound("webhook".to_string()));
        }
        Ok(rows.into_iter().map(Into::into).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::test_support::seed_owned_repository;

    struct IdentityEncryptor;
    impl SecretEncryptorPort for IdentityEncryptor {
        fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>, DomainError> {
            Ok(plaintext.as_bytes().to_vec())
        }
        fn decrypt(&self, ciphertext: &[u8]) -> Result<String, DomainError> {
            Ok(String::from_utf8(ciphertext.to_vec()).unwrap())
        }
    }

    async fn seed_repository(pool: &PgPool) -> Uuid {
        seed_owned_repository(pool, &format!("owner-{}", Uuid::new_v4()))
            .await
            .1
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn creating_then_listing_a_webhook_returns_it_without_the_secret(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let store = PostgresWebhookStore::new(pool, Arc::new(IdentityEncryptor));

        let created = store
            .create(NewWebhook {
                repository_id,
                url: "https://example.com/hook".to_string(),
                secret_plaintext: "shh".to_string(),
                events: vec!["issue_closed".to_string()],
            })
            .await
            .unwrap();

        assert_eq!(created.url, "https://example.com/hook");
        assert_eq!(created.events, vec!["issue_closed".to_string()]);
        assert!(created.active);

        let listed = store.list_for_repository(repository_id).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, created.id);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn the_secret_round_trips_through_resolve_secret_plaintext(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let store = PostgresWebhookStore::new(pool, Arc::new(IdentityEncryptor));
        let created = store
            .create(NewWebhook {
                repository_id,
                url: "https://example.com".to_string(),
                secret_plaintext: "top-secret".to_string(),
                events: vec![],
            })
            .await
            .unwrap();

        let resolved = store.resolve_secret_plaintext(created.id).await.unwrap();

        assert_eq!(resolved, "top-secret");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn update_changes_only_the_given_fields(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let store = PostgresWebhookStore::new(pool, Arc::new(IdentityEncryptor));
        let created = store
            .create(NewWebhook {
                repository_id,
                url: "https://old.example.com".to_string(),
                secret_plaintext: "s".to_string(),
                events: vec!["issue_closed".to_string()],
            })
            .await
            .unwrap();

        let updated = store
            .update(
                created.id,
                repository_id,
                WebhookUpdate {
                    active: Some(false),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(
            updated.url, "https://old.example.com",
            "url must be unchanged when not part of the update"
        );
        assert!(!updated.active);
        assert_eq!(
            updated.events,
            vec!["issue_closed".to_string()],
            "events must be unchanged when not part of the update"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn update_is_scoped_to_the_given_repository(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let other_repository_id = seed_repository(&pool).await;
        let store = PostgresWebhookStore::new(pool, Arc::new(IdentityEncryptor));
        let created = store
            .create(NewWebhook {
                repository_id,
                url: "https://example.com".to_string(),
                secret_plaintext: "s".to_string(),
                events: vec![],
            })
            .await
            .unwrap();

        let result = store
            .update(
                created.id,
                other_repository_id,
                WebhookUpdate {
                    active: Some(false),
                    ..Default::default()
                },
            )
            .await;

        assert!(
            matches!(result, Err(DomainError::NotFound(_))),
            "updating through the wrong repository_id must not succeed"
        );
        let unchanged = store.find_by_id(created.id).await.unwrap().unwrap();
        assert!(unchanged.active, "the webhook must be unchanged");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn delete_is_scoped_to_the_given_repository(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let other_repository_id = seed_repository(&pool).await;
        let store = PostgresWebhookStore::new(pool, Arc::new(IdentityEncryptor));
        let created = store
            .create(NewWebhook {
                repository_id,
                url: "https://example.com".to_string(),
                secret_plaintext: "s".to_string(),
                events: vec![],
            })
            .await
            .unwrap();

        let result = store.delete(created.id, other_repository_id).await;

        assert!(
            matches!(result, Err(DomainError::NotFound(_))),
            "deleting through the wrong repository_id must not succeed"
        );
        assert!(
            store.find_by_id(created.id).await.unwrap().is_some(),
            "the webhook must still exist"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_active_for_event_only_returns_active_webhooks_subscribed_to_that_event(
        pool: PgPool,
    ) {
        let repository_id = seed_repository(&pool).await;
        let store = PostgresWebhookStore::new(pool, Arc::new(IdentityEncryptor));
        let matching = store
            .create(NewWebhook {
                repository_id,
                url: "https://a.example.com".to_string(),
                secret_plaintext: "s".to_string(),
                events: vec!["issue_closed".to_string()],
            })
            .await
            .unwrap();
        let wrong_event = store
            .create(NewWebhook {
                repository_id,
                url: "https://b.example.com".to_string(),
                secret_plaintext: "s".to_string(),
                events: vec!["issue_assigned".to_string()],
            })
            .await
            .unwrap();
        let inactive = store
            .create(NewWebhook {
                repository_id,
                url: "https://c.example.com".to_string(),
                secret_plaintext: "s".to_string(),
                events: vec!["issue_closed".to_string()],
            })
            .await
            .unwrap();
        store
            .update(
                inactive.id,
                repository_id,
                WebhookUpdate {
                    active: Some(false),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        let results = store
            .list_active_for_event(repository_id, "issue_closed")
            .await
            .unwrap();

        assert_eq!(
            results.iter().map(|w| w.id).collect::<Vec<_>>(),
            vec![matching.id]
        );
        let _ = (wrong_event, inactive);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn deliveries_are_recorded_and_listed_most_recent_first(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let store = PostgresWebhookStore::new(pool, Arc::new(IdentityEncryptor));
        let webhook = store
            .create(NewWebhook {
                repository_id,
                url: "https://example.com".to_string(),
                secret_plaintext: "s".to_string(),
                events: vec![],
            })
            .await
            .unwrap();

        store
            .record_delivery(NewWebhookDelivery {
                webhook_id: webhook.id,
                event_kind: "issue_closed".to_string(),
                http_status: Some(200),
                success: true,
                error_message: None,
            })
            .await
            .unwrap();
        store
            .record_delivery(NewWebhookDelivery {
                webhook_id: webhook.id,
                event_kind: "issue_closed".to_string(),
                http_status: None,
                success: false,
                error_message: Some("timed out".to_string()),
            })
            .await
            .unwrap();

        let deliveries = store
            .list_deliveries(webhook.id, repository_id, 10)
            .await
            .unwrap();

        assert_eq!(deliveries.len(), 2);
        assert!(
            !deliveries[0].success,
            "the most recent delivery (the failed one) must come first"
        );
        assert_eq!(deliveries[0].error_message.as_deref(), Some("timed out"));
        assert!(deliveries[1].success);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_deliveries_is_scoped_to_the_given_repository(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let other_repository_id = seed_repository(&pool).await;
        let store = PostgresWebhookStore::new(pool, Arc::new(IdentityEncryptor));
        let webhook = store
            .create(NewWebhook {
                repository_id,
                url: "https://example.com".to_string(),
                secret_plaintext: "s".to_string(),
                events: vec![],
            })
            .await
            .unwrap();
        store
            .record_delivery(NewWebhookDelivery {
                webhook_id: webhook.id,
                event_kind: "issue_closed".to_string(),
                http_status: Some(200),
                success: true,
                error_message: None,
            })
            .await
            .unwrap();

        let result = store
            .list_deliveries(webhook.id, other_repository_id, 10)
            .await;

        assert!(
            matches!(result, Err(DomainError::NotFound(_))),
            "listing deliveries through the wrong repository_id must not succeed"
        );
        let legit = store
            .list_deliveries(webhook.id, repository_id, 10)
            .await
            .unwrap();
        assert_eq!(
            legit.len(),
            1,
            "the legitimate repository_id must still see the delivery"
        );
    }
}
