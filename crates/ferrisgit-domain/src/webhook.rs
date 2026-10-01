use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::net::IpAddr;
use uuid::Uuid;

use crate::error::DomainError;

/// Shared by the create/update-time URL validator and the delivery-time `SafeWebhookResolver`. Checking at creation
/// alone is not enough: a hostname can be re-pointed at an internal address after saving (DNS rebinding), so the
/// resolver enforces it again atomically with the connection.
pub fn is_disallowed_webhook_target(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_unspecified()
                || v4.is_link_local() // covers 169.254.0.0/16, including the 169.254.169.254 cloud metadata address
                || v4.is_private() // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                || v4.is_multicast()
                // 100.64.0.0/10 - shared address space (CGNAT, RFC 6598)
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                // fc00::/7 - unique local addresses
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                // fe80::/10 - link-local addresses
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                // 64:ff9b::/96 - NAT64 well-known prefix (RFC 6052): re-check the embedded IPv4 address
                || (v6.segments()[0..6] == [0x0064, 0xff9b, 0, 0, 0, 0]
                    && is_disallowed_webhook_target(IpAddr::V4(std::net::Ipv4Addr::new(
                        (v6.segments()[6] >> 8) as u8,
                        (v6.segments()[6] & 0xff) as u8,
                        (v6.segments()[7] >> 8) as u8,
                        (v6.segments()[7] & 0xff) as u8,
                    ))))
                // 2002::/16 - 6to4: re-check the embedded IPv4 address
                || (v6.segments()[0] == 0x2002
                    && is_disallowed_webhook_target(IpAddr::V4(std::net::Ipv4Addr::new(
                        (v6.segments()[1] >> 8) as u8,
                        (v6.segments()[1] & 0xff) as u8,
                        (v6.segments()[2] >> 8) as u8,
                        (v6.segments()[2] & 0xff) as u8,
                    ))))
                // ::ffff:0:0/96 - IPv4-mapped addresses: re-check the embedded IPv4 address
                || v6.to_ipv4_mapped().is_some_and(|v4| is_disallowed_webhook_target(IpAddr::V4(v4)))
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Webhook {
    pub id: Uuid,
    pub repository_id: Uuid,
    pub url: String,
    pub events: Vec<String>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

pub struct NewWebhook {
    pub repository_id: Uuid,
    pub url: String,
    pub secret_plaintext: String,
    pub events: Vec<String>,
}

#[derive(Debug, Default)]
pub struct WebhookUpdate {
    pub url: Option<String>,
    pub secret_plaintext: Option<String>,
    pub events: Option<Vec<String>>,
    pub active: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebhookDelivery {
    pub id: Uuid,
    pub webhook_id: Uuid,
    pub event_kind: String,
    pub http_status: Option<i32>,
    pub success: bool,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub struct NewWebhookDelivery {
    pub webhook_id: Uuid,
    pub event_kind: String,
    pub http_status: Option<i32>,
    pub success: bool,
    pub error_message: Option<String>,
}

#[async_trait]
pub trait WebhookStorePort: Send + Sync {
    async fn create(&self, new_webhook: NewWebhook) -> Result<Webhook, DomainError>;
    async fn list_for_repository(&self, repository_id: Uuid) -> Result<Vec<Webhook>, DomainError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Webhook>, DomainError>;
    /// Scoped by `repository_id` so a webhook of another repository cannot be updated by guessing its id. `NotFound`
    /// if no match.
    async fn update(
        &self,
        id: Uuid,
        repository_id: Uuid,
        update: WebhookUpdate,
    ) -> Result<Webhook, DomainError>;
    /// Scoped by `repository_id` so a webhook of another repository cannot be deleted by guessing its id. `NotFound`
    /// if no match.
    async fn delete(&self, id: Uuid, repository_id: Uuid) -> Result<(), DomainError>;
    async fn list_active_for_event(
        &self,
        repository_id: Uuid,
        event_kind: &str,
    ) -> Result<Vec<Webhook>, DomainError>;
    /// Decrypts one webhook's secret for HMAC-signing a delivery: the only place a plaintext secret is reconstructed.
    async fn resolve_secret_plaintext(&self, webhook_id: Uuid) -> Result<String, DomainError>;
    async fn record_delivery(&self, delivery: NewWebhookDelivery) -> Result<(), DomainError>;
    /// Most recent first, capped at `limit`. Scoped by `repository_id`; `NotFound` if `webhook_id`/`repository_id`
    /// don't match.
    async fn list_deliveries(
        &self,
        webhook_id: Uuid,
        repository_id: Uuid,
        limit: i64,
    ) -> Result<Vec<WebhookDelivery>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_webhook_update_defaults_to_changing_nothing() {
        let update = WebhookUpdate::default();
        assert!(update.url.is_none());
        assert!(update.secret_plaintext.is_none());
        assert!(update.events.is_none());
        assert!(update.active.is_none());
    }

    #[test]
    fn loopback_link_local_and_private_ipv4_addresses_are_all_disallowed() {
        assert!(is_disallowed_webhook_target("127.0.0.1".parse().unwrap()));
        assert!(
            is_disallowed_webhook_target("169.254.169.254".parse().unwrap()),
            "the cloud metadata address must be rejected"
        );
        assert!(is_disallowed_webhook_target("10.0.0.5".parse().unwrap()));
        assert!(is_disallowed_webhook_target("172.16.0.1".parse().unwrap()));
        assert!(is_disallowed_webhook_target("192.168.1.1".parse().unwrap()));
        assert!(is_disallowed_webhook_target("0.0.0.0".parse().unwrap()));
    }

    #[test]
    fn a_normal_public_looking_ipv4_address_is_allowed() {
        assert!(
            !is_disallowed_webhook_target("93.184.216.34".parse().unwrap()),
            "a normal public IP must not be rejected"
        );
    }

    #[test]
    fn cgnat_and_multicast_ipv4_addresses_are_disallowed() {
        assert!(
            is_disallowed_webhook_target("100.64.0.1".parse().unwrap()),
            "the CGNAT range 100.64.0.0/10 must be rejected"
        );
        assert!(
            is_disallowed_webhook_target("100.127.255.255".parse().unwrap()),
            "the top of the CGNAT range must be rejected"
        );
        assert!(
            !is_disallowed_webhook_target("100.63.255.255".parse().unwrap()),
            "just below the CGNAT range must be allowed"
        );
        assert!(
            is_disallowed_webhook_target("224.0.0.1".parse().unwrap()),
            "multicast addresses must be rejected"
        );
    }

    #[test]
    fn loopback_and_unique_local_ipv6_addresses_are_disallowed() {
        assert!(is_disallowed_webhook_target("::1".parse().unwrap()));
        assert!(is_disallowed_webhook_target("fc00::1".parse().unwrap()));
        assert!(is_disallowed_webhook_target("fe80::1".parse().unwrap()));
        assert!(
            is_disallowed_webhook_target("::ffff:127.0.0.1".parse().unwrap()),
            "an IPv4-mapped loopback address must be rejected"
        );
    }

    #[test]
    fn a_normal_public_looking_ipv6_address_is_allowed() {
        assert!(!is_disallowed_webhook_target(
            "2606:2800:220:1:248:1893:25c8:1946".parse().unwrap()
        ));
    }

    #[test]
    fn multicast_nat64_and_6to4_ipv6_addresses_are_disallowed() {
        assert!(
            is_disallowed_webhook_target("ff02::1".parse().unwrap()),
            "multicast addresses must be rejected"
        );
        assert!(
            is_disallowed_webhook_target("64:ff9b::a9fe:a9fe".parse().unwrap()),
            "a NAT64 address embedding the cloud metadata IP must be rejected"
        );
        assert!(
            is_disallowed_webhook_target("2002:a9fe:a9fe::".parse().unwrap()),
            "a 6to4 address embedding the cloud metadata IP must be rejected"
        );
        assert!(
            !is_disallowed_webhook_target("64:ff9b::5db8:d822".parse().unwrap()),
            "a NAT64 address embedding a public IP must be allowed"
        );
    }
}
