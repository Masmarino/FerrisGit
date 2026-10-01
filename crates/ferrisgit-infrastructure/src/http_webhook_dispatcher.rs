use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::webhook::{
    NewWebhookDelivery, Webhook, WebhookStorePort, is_disallowed_webhook_target,
};
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use ferrisgit_domain::webhook_event::WebhookEvent;
use hmac::{Hmac, KeyInit, Mac};
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use sha2::Sha256;
use tokio::sync::Semaphore;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

fn signature(secret: &str, body: &[u8]) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts a key of any length");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

/// Resolver that rejects disallowed addresses at connection time. `validate_webhook_url` only runs at
/// create/update, which leaves a DNS-rebinding gap. Here, resolving and checking are one atomic step.
struct SafeWebhookResolver;

impl Resolve for SafeWebhookResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let host = name.as_str().to_string();
            // Port 0 per `Resolve`'s contract; reqwest substitutes the URL's real port.
            let addrs: Vec<SocketAddr> =
                tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            if addrs.is_empty() {
                return Err(format!("could not resolve webhook host '{host}'").into());
            }
            if let Some(disallowed) = addrs
                .iter()
                .find(|addr| is_disallowed_webhook_target(addr.ip()))
            {
                return Err(format!(
                    "webhook host '{host}' resolves to a disallowed address ({})",
                    disallowed.ip()
                )
                .into());
            }
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

/// Caps concurrent deliveries per dispatcher, so one event can't fire an unbounded number of
/// outbound calls or tasks (amplification). `dispatch` itself still returns immediately.
const MAX_CONCURRENT_DELIVERIES: usize = 10;

pub struct HttpWebhookDispatcher {
    webhooks: Arc<dyn WebhookStorePort>,
    client: reqwest::Client,
    delivery_semaphore: Arc<Semaphore>,
}

impl HttpWebhookDispatcher {
    pub fn new(webhooks: Arc<dyn WebhookStorePort>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            // Redirects are off so a receiver can't bounce the signed request to a host other than the
            // validated one (SSRF). reqwest keeps `X-FerrisGit-Signature-256` on cross-host hops.
            .redirect(reqwest::redirect::Policy::none())
            .dns_resolver(Arc::new(SafeWebhookResolver))
            // A configured proxy would resolve DNS itself and bypass `SafeWebhookResolver`.
            .no_proxy()
            .build()
            .expect("reqwest client with a fixed timeout and no-redirect policy always builds");
        Self {
            webhooks,
            client,
            delivery_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_DELIVERIES)),
        }
    }
}

#[async_trait]
impl WebhookDispatcherPort for HttpWebhookDispatcher {
    async fn dispatch(&self, repository_id: Uuid, event: WebhookEvent) -> Result<(), DomainError> {
        let targets = self
            .webhooks
            .list_active_for_event(repository_id, event.kind())
            .await?;
        for webhook in targets {
            let webhooks = self.webhooks.clone();
            let client = self.client.clone();
            let event = event.clone();
            let semaphore = self.delivery_semaphore.clone();
            tokio::spawn(async move {
                let _permit = semaphore.acquire_owned().await;
                deliver(&client, webhooks.as_ref(), webhook, event).await;
            });
        }
        Ok(())
    }
}

/// `reqwest::Error`'s `Display` leaves out the underlying cause (a `SafeWebhookResolver` rejection sits
/// several `source()` hops down), so the deepest message is appended to make the delivery record useful.
fn describe_send_error(err: &reqwest::Error) -> String {
    let mut message = err.to_string();
    let mut source = std::error::Error::source(err);
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

/// Signs, sends and logs one delivery attempt. Never returns an error: it runs on a spawned task after
/// `dispatch` returned, so every failure ends in `record_delivery`.
async fn deliver(
    client: &reqwest::Client,
    store: &dyn WebhookStorePort,
    webhook: Webhook,
    event: WebhookEvent,
) {
    let event_kind = event.kind().to_string();
    let Ok(body) = serde_json::to_vec(&event) else {
        store
            .record_delivery(NewWebhookDelivery {
                webhook_id: webhook.id,
                event_kind,
                http_status: None,
                success: false,
                error_message: Some("failed to serialize event payload".to_string()),
            })
            .await
            .ok();
        return;
    };

    let secret = match store.resolve_secret_plaintext(webhook.id).await {
        Ok(secret) => secret,
        Err(e) => {
            store
                .record_delivery(NewWebhookDelivery {
                    webhook_id: webhook.id,
                    event_kind,
                    http_status: None,
                    success: false,
                    error_message: Some(format!("failed to resolve secret: {e}")),
                })
                .await
                .ok();
            return;
        }
    };

    let signature = signature(&secret, &body);

    // Delivery id and timestamp stay out of the HMAC so existing receivers keep verifying.
    let delivery_id = Uuid::new_v4();
    let delivered_at = chrono::Utc::now().to_rfc3339();

    let (http_status, success, error_message) = match client
        .post(&webhook.url)
        .header("X-FerrisGit-Signature-256", format!("sha256={signature}"))
        .header("X-FerrisGit-Delivery", delivery_id.to_string())
        .header("X-FerrisGit-Delivered-At", delivered_at)
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status();
            let error_message = if status.is_success() {
                None
            } else {
                Some(format!(
                    "received {} {}",
                    status.as_u16(),
                    status.canonical_reason().unwrap_or("unknown status")
                ))
            };
            (
                Some(status.as_u16() as i32),
                status.is_success(),
                error_message,
            )
        }
        Err(e) => (None, false, Some(describe_send_error(&e))),
    };

    if !success {
        tracing::warn!(webhook_id = %webhook.id, event_kind = %event.kind(), ?http_status, ?error_message, "webhook delivery failed");
    }

    store
        .record_delivery(NewWebhookDelivery {
            webhook_id: webhook.id,
            event_kind: event.kind().to_string(),
            http_status,
            success,
            error_message,
        })
        .await
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrisgit_domain::webhook::{Webhook, WebhookUpdate};
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeWebhookStore {
        webhooks: Mutex<Vec<Webhook>>,
        secrets: Mutex<std::collections::HashMap<Uuid, String>>,
        deliveries: Mutex<Vec<NewWebhookDelivery>>,
    }

    #[async_trait]
    impl WebhookStorePort for FakeWebhookStore {
        async fn create(
            &self,
            _new_webhook: ferrisgit_domain::webhook::NewWebhook,
        ) -> Result<Webhook, DomainError> {
            unimplemented!()
        }
        async fn list_for_repository(
            &self,
            _repository_id: Uuid,
        ) -> Result<Vec<Webhook>, DomainError> {
            unimplemented!()
        }
        async fn find_by_id(&self, _id: Uuid) -> Result<Option<Webhook>, DomainError> {
            unimplemented!()
        }
        async fn update(
            &self,
            _id: Uuid,
            _repository_id: Uuid,
            _update: WebhookUpdate,
        ) -> Result<Webhook, DomainError> {
            unimplemented!()
        }
        async fn delete(&self, _id: Uuid, _repository_id: Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn list_active_for_event(
            &self,
            repository_id: Uuid,
            event_kind: &str,
        ) -> Result<Vec<Webhook>, DomainError> {
            Ok(self
                .webhooks
                .lock()
                .unwrap()
                .iter()
                .filter(|w| {
                    w.repository_id == repository_id
                        && w.active
                        && w.events.iter().any(|e| e == event_kind)
                })
                .cloned()
                .collect())
        }
        async fn resolve_secret_plaintext(&self, webhook_id: Uuid) -> Result<String, DomainError> {
            self.secrets
                .lock()
                .unwrap()
                .get(&webhook_id)
                .cloned()
                .ok_or_else(|| DomainError::NotFound("webhook".to_string()))
        }
        async fn record_delivery(&self, delivery: NewWebhookDelivery) -> Result<(), DomainError> {
            self.deliveries.lock().unwrap().push(delivery);
            Ok(())
        }
        async fn list_deliveries(
            &self,
            _webhook_id: Uuid,
            _repository_id: Uuid,
            _limit: i64,
        ) -> Result<Vec<ferrisgit_domain::webhook::WebhookDelivery>, DomainError> {
            unimplemented!()
        }
    }

    fn webhook(id: Uuid, repository_id: Uuid, url: String, events: Vec<String>) -> Webhook {
        Webhook {
            id,
            repository_id,
            url,
            events,
            active: true,
            created_at: chrono::Utc::now(),
        }
    }

    fn sample_event() -> WebhookEvent {
        WebhookEvent::IssueClosed {
            repository_owner: "alice".to_string(),
            repository_name: "hello".to_string(),
            actor_username: "bob".to_string(),
            issue_id: Uuid::nil(),
            issue_title: "bug".to_string(),
        }
    }

    /// Regression: the disallow-list must run at connection time, not just at create/update. Targets
    /// `localhost` rather than `127.0.0.1` because reqwest skips its resolver for literal IPs.
    #[tokio::test]
    async fn a_delivery_to_a_disallowed_address_is_blocked_even_though_it_would_otherwise_succeed()
    {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            // A real listener, so a missing resolver check would produce a successful delivery.
            if let Ok((mut socket, _)) = listener.accept().await {
                use tokio::io::AsyncWriteExt;
                let _ = socket
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n")
                    .await;
            }
        });

        let repository_id = Uuid::new_v4();
        let webhook_id = Uuid::new_v4();
        let store = Arc::new(FakeWebhookStore::default());
        store.webhooks.lock().unwrap().push(webhook(
            webhook_id,
            repository_id,
            format!("http://localhost:{}/", addr.port()),
            vec!["issue_closed".to_string()],
        ));
        store
            .secrets
            .lock()
            .unwrap()
            .insert(webhook_id, "secret".to_string());
        let dispatcher = HttpWebhookDispatcher::new(store.clone());

        dispatcher
            .dispatch(repository_id, sample_event())
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let deliveries = store.deliveries.lock().unwrap();
        assert_eq!(deliveries.len(), 1);
        assert!(
            !deliveries[0].success,
            "a loopback address must be blocked even though a real listener is there to accept it"
        );
        let error_message = deliveries[0].error_message.as_deref().unwrap_or_default();
        assert!(
            error_message.contains("disallowed"),
            "the failure must be attributed to the disallow-list check specifically, got: {error_message}"
        );
    }

    #[tokio::test]
    async fn dispatch_only_fires_active_webhooks_subscribed_to_the_event() {
        let repository_id = Uuid::new_v4();
        let matching_id = Uuid::new_v4();
        let store = Arc::new(FakeWebhookStore::default());
        store.webhooks.lock().unwrap().push(webhook(
            matching_id,
            repository_id,
            "http://127.0.0.1:1/unreachable".to_string(),
            vec!["issue_closed".to_string()],
        ));
        store
            .secrets
            .lock()
            .unwrap()
            .insert(matching_id, "secret".to_string());
        let dispatcher = HttpWebhookDispatcher::new(store.clone());

        dispatcher
            .dispatch(repository_id, sample_event())
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let deliveries = store.deliveries.lock().unwrap();
        assert_eq!(deliveries.len(), 1);
        assert!(
            !deliveries[0].success,
            "an unreachable address must be recorded as a failed delivery, not panic or hang"
        );
    }

    #[tokio::test]
    async fn dispatch_never_fires_a_webhook_from_a_different_repository() {
        let repository_id = Uuid::new_v4();
        let other_repository_id = Uuid::new_v4();
        let store = Arc::new(FakeWebhookStore::default());
        store.webhooks.lock().unwrap().push(webhook(
            Uuid::new_v4(),
            other_repository_id,
            "http://127.0.0.1:1/unreachable".to_string(),
            vec!["issue_closed".to_string()],
        ));
        let dispatcher = HttpWebhookDispatcher::new(store.clone());

        dispatcher
            .dispatch(repository_id, sample_event())
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        assert!(store.deliveries.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_delivery_carries_a_unique_delivery_id_and_a_delivered_at_timestamp_header() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (request_tx, request_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = vec![0u8; 8192];
                let n = socket.read(&mut buf).await.unwrap_or(0);
                let _ = socket
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n")
                    .await;
                let _ = request_tx.send(String::from_utf8_lossy(&buf[..n]).to_string());
            }
        });

        let repository_id = Uuid::new_v4();
        let webhook_id = Uuid::new_v4();
        let store = Arc::new(FakeWebhookStore::default());
        store.webhooks.lock().unwrap().push(webhook(
            webhook_id,
            repository_id,
            format!("http://127.0.0.1:{}/", addr.port()),
            vec!["issue_closed".to_string()],
        ));
        store
            .secrets
            .lock()
            .unwrap()
            .insert(webhook_id, "secret".to_string());
        let dispatcher = HttpWebhookDispatcher::new(store.clone());

        dispatcher
            .dispatch(repository_id, sample_event())
            .await
            .unwrap();
        let raw_request = tokio::time::timeout(std::time::Duration::from_secs(2), request_rx)
            .await
            .unwrap()
            .unwrap();

        assert!(
            raw_request.contains("x-ferrisgit-delivery:"),
            "missing delivery-id header in request:\n{raw_request}"
        );
        assert!(
            raw_request.contains("x-ferrisgit-delivered-at:"),
            "missing delivered-at header in request:\n{raw_request}"
        );

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let deliveries = store.deliveries.lock().unwrap();
        assert_eq!(deliveries.len(), 1);
        assert!(deliveries[0].success);
    }

    #[tokio::test]
    async fn a_non_2xx_response_records_a_descriptive_error_message() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                use tokio::io::AsyncWriteExt;
                let _ = socket
                    .write_all(b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 0\r\n\r\n")
                    .await;
            }
        });

        let repository_id = Uuid::new_v4();
        let webhook_id = Uuid::new_v4();
        let store = Arc::new(FakeWebhookStore::default());
        store.webhooks.lock().unwrap().push(webhook(
            webhook_id,
            repository_id,
            format!("http://127.0.0.1:{}/", addr.port()),
            vec!["issue_closed".to_string()],
        ));
        store
            .secrets
            .lock()
            .unwrap()
            .insert(webhook_id, "secret".to_string());
        let dispatcher = HttpWebhookDispatcher::new(store.clone());

        dispatcher
            .dispatch(repository_id, sample_event())
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let deliveries = store.deliveries.lock().unwrap();
        assert_eq!(deliveries.len(), 1);
        assert!(!deliveries[0].success);
        assert_eq!(deliveries[0].http_status, Some(500));
        let error_message = deliveries[0].error_message.as_deref().unwrap_or_default();
        assert!(
            error_message.contains("500"),
            "expected the status code in the error message, got: {error_message}"
        );
    }

    #[test]
    fn the_computed_signature_matches_an_independently_recomputed_hmac() {
        let secret = "shh";
        let body = br#"{"event":"IssueClosed"}"#;

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let signature_a = hex::encode(mac.finalize().into_bytes());

        let mut verifier = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        verifier.update(body);
        let signature_b = hex::encode(verifier.finalize().into_bytes());

        assert_eq!(signature_a, signature_b);
        assert_eq!(
            signature_a.len(),
            64,
            "a hex-encoded SHA-256 HMAC is 64 characters"
        );
    }

    // Produced by hmac 0.12: receivers already verify this signature, so it must not change.
    #[test]
    fn the_signature_matches_the_one_the_previous_hmac_version_produced() {
        assert_eq!(
            signature("webhook-secret", br#"{"event":"merge_request_merged"}"#),
            "0eb7da4085d148bfacba8d84fd171203c965b4dac3dbd1c2332ea32a3121d568"
        );
    }
}
