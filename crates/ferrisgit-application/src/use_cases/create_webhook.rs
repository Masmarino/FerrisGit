use std::net::IpAddr;
use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::webhook::{
    NewWebhook, Webhook, WebhookStorePort, is_disallowed_webhook_target,
};
use ferrisgit_domain::webhook_event::WebhookEvent;
use uuid::Uuid;

/// Any user can create a repository and be its Maintainer, so without a cap one event could fan out to unbounded
/// simultaneous outbound requests at one target (amplification, or a self-inflicted DoS).
const MAX_WEBHOOKS_PER_REPOSITORY: usize = 20;

pub struct CreateWebhookUseCase {
    webhooks: Arc<dyn WebhookStorePort>,
}

impl CreateWebhookUseCase {
    pub fn new(webhooks: Arc<dyn WebhookStorePort>) -> Self {
        Self { webhooks }
    }

    pub async fn execute(
        &self,
        repository_id: Uuid,
        url: String,
        secret_plaintext: String,
        events: Vec<String>,
    ) -> Result<Webhook, DomainError> {
        validate_webhook_url(&url).await?;
        validate_event_kinds(&events)?;
        let existing_count = self
            .webhooks
            .list_for_repository(repository_id)
            .await?
            .len();
        if existing_count >= MAX_WEBHOOKS_PER_REPOSITORY {
            return Err(DomainError::Validation(format!(
                "a repository may have at most {MAX_WEBHOOKS_PER_REPOSITORY} webhooks"
            )));
        }
        self.webhooks
            .create(NewWebhook {
                repository_id,
                url,
                secret_plaintext,
                events,
            })
            .await
    }
}

/// Rejects non-`http`/`https` URLs and hosts that resolve (via DNS, or literally) to loopback, link-local, private
/// (RFC 1918) or the cloud metadata address 169.254.169.254. Closes a blind SSRF: any Maintainer could otherwise scan
/// internal hosts and ports by reading `httpStatus`/`success` from the deliveries endpoint.
///
/// The resolved IPs are checked, not just the hostname string: a public-looking name can resolve to an internal
/// address.
pub(crate) async fn validate_webhook_url(url: &str) -> Result<(), DomainError> {
    let parsed = url::Url::parse(url)
        .map_err(|_| DomainError::Validation("invalid webhook url".to_string()))?;

    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(DomainError::Validation(format!(
                "webhook url scheme must be http or https, got {other}"
            )));
        }
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| DomainError::Validation("webhook url must have a host".to_string()))?;

    if let Ok(ip) = host.parse::<IpAddr>() {
        return reject_if_disallowed_ip(ip);
    }

    let port = parsed.port_or_known_default().unwrap_or(443);
    let mut resolved_any = false;
    let addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| DomainError::Validation(format!("could not resolve webhook host: {e}")))?;
    for addr in addrs {
        resolved_any = true;
        reject_if_disallowed_ip(addr.ip())?;
    }
    if !resolved_any {
        return Err(DomainError::Validation(
            "could not resolve webhook host".to_string(),
        ));
    }
    Ok(())
}

fn reject_if_disallowed_ip(ip: IpAddr) -> Result<(), DomainError> {
    if is_disallowed_webhook_target(ip) {
        return Err(DomainError::Validation(format!(
            "webhook url resolves to a disallowed address ({ip})"
        )));
    }
    Ok(())
}

/// A typo or outdated integration would otherwise silently subscribe to nothing. Failing right away with the bad value
/// named is easier to diagnose.
pub(crate) fn validate_event_kinds(events: &[String]) -> Result<(), DomainError> {
    let unknown: Vec<&str> = events
        .iter()
        .map(String::as_str)
        .filter(|e| !WebhookEvent::ALL_KINDS.contains(e))
        .collect();
    if !unknown.is_empty() {
        return Err(DomainError::Validation(format!(
            "unknown webhook event kind(s): {}",
            unknown.join(", ")
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeWebhookStore;

    #[tokio::test]
    async fn creates_a_webhook_with_a_valid_url_and_known_event_kinds() {
        let repository_id = Uuid::new_v4();
        let use_case = CreateWebhookUseCase::new(Arc::new(FakeWebhookStore::empty()));

        let webhook = use_case
            .execute(
                repository_id,
                "https://example.com/hook".to_string(),
                "shh".to_string(),
                vec!["issue_closed".to_string()],
            )
            .await
            .unwrap();

        assert_eq!(webhook.repository_id, repository_id);
        assert_eq!(webhook.url, "https://example.com/hook");
    }

    #[tokio::test]
    async fn rejects_a_url_resolving_to_a_private_address() {
        let use_case = CreateWebhookUseCase::new(Arc::new(FakeWebhookStore::empty()));

        let result = use_case
            .execute(
                Uuid::new_v4(),
                "http://127.0.0.1/hook".to_string(),
                "shh".to_string(),
                vec![],
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn rejects_an_unknown_event_kind() {
        let use_case = CreateWebhookUseCase::new(Arc::new(FakeWebhookStore::empty()));

        let result = use_case
            .execute(
                Uuid::new_v4(),
                "https://example.com/hook".to_string(),
                "shh".to_string(),
                vec!["totally_made_up".to_string()],
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn rejects_creation_past_the_per_repository_cap() {
        let repository_id = Uuid::new_v4();
        let webhooks = Arc::new(FakeWebhookStore::empty());
        for _ in 0..MAX_WEBHOOKS_PER_REPOSITORY {
            webhooks
                .create(NewWebhook {
                    repository_id,
                    url: "https://example.com/hook".to_string(),
                    secret_plaintext: "shh".to_string(),
                    events: vec![],
                })
                .await
                .unwrap();
        }
        let use_case = CreateWebhookUseCase::new(webhooks);

        let result = use_case
            .execute(
                repository_id,
                "https://example.com/one-too-many".to_string(),
                "shh".to_string(),
                vec![],
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    // `is_disallowed_webhook_target`'s unit tests live in `ferrisgit_domain::webhook`, shared with the delivery-time
    // resolver.

    #[tokio::test]
    async fn validate_webhook_url_rejects_literal_private_ips_without_any_dns_lookup() {
        assert!(
            validate_webhook_url("http://127.0.0.1/whatever")
                .await
                .is_err()
        );
        assert!(
            validate_webhook_url("http://169.254.169.254/latest/meta-data")
                .await
                .is_err()
        );
        assert!(validate_webhook_url("http://10.0.0.5/hook").await.is_err());
        assert!(
            validate_webhook_url("http://192.168.1.1/hook")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn validate_webhook_url_rejects_a_non_http_scheme() {
        let err = validate_webhook_url("ftp://example.com/hook")
            .await
            .unwrap_err();
        assert!(matches!(err, DomainError::Validation(_)));
    }

    #[tokio::test]
    async fn validate_webhook_url_accepts_a_normal_public_looking_https_url() {
        // example.com is IANA-reserved and resolves publicly: exercises real DNS without depending on a third party's
        // uptime for a rejection.
        validate_webhook_url("https://example.com/hook")
            .await
            .expect("a normal public https url must be accepted");
    }

    #[test]
    fn validate_event_kinds_accepts_every_known_kind() {
        let events: Vec<String> = WebhookEvent::ALL_KINDS
            .iter()
            .map(|k| k.to_string())
            .collect();
        validate_event_kinds(&events).expect("every known kind must be accepted");
    }

    #[test]
    fn validate_event_kinds_rejects_an_unknown_kind_and_names_it() {
        let err =
            validate_event_kinds(&["issue_closed".to_string(), "totally_made_up".to_string()])
                .unwrap_err();
        match err {
            DomainError::Validation(message) => assert!(
                message.contains("totally_made_up"),
                "error must name the bad value, got: {message}"
            ),
            other => panic!("expected a Validation error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn validate_webhook_url_rejects_a_hostname_that_resolves_to_loopback() {
        // `localhost` looks nothing like a private IP as a string, which is why the name must be resolved before the IP
        // check.
        let err = validate_webhook_url("http://localhost/hook")
            .await
            .unwrap_err();
        assert!(matches!(err, DomainError::Validation(_)));
    }
}
