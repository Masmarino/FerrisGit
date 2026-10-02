// Per-IP throttles key on the TCP peer, or, for a peer in `TRUSTED_PROXY_CIDRS`, on `X-Forwarded-For`.
// Two servers share one database: one trusts the loopback peer (the test client), the other trusts nobody.

mod common;

use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;

struct Server {
    addr: SocketAddr,
    client: reqwest::Client,
}

impl Server {
    async fn post(&self, path: &str, body: Value, forwarded_for: Option<&str>) -> u16 {
        let mut req = self
            .client
            .post(format!("http://{}/api{path}", self.addr))
            .json(&body);
        if let Some(value) = forwarded_for {
            req = req.header("X-Forwarded-For", value);
        }
        req.send().await.unwrap().status().as_u16()
    }

    async fn register(&self, forwarded_for: Option<&str>) -> u16 {
        // Registration is off in these tests, so the answer is a plain 400. Only 400 vs 429 matters here.
        self.post("/auth/register", json!({ "username": "alice", "email": "alice@example.com", "password": "password12345" }), forwarded_for).await
    }

    async fn login(&self, forwarded_for: Option<&str>) -> u16 {
        self.post(
            "/auth/login",
            json!({ "username": "nobody", "password": "wrong-password" }),
            forwarded_for,
        )
        .await
    }

    async fn activate(&self, forwarded_for: Option<&str>) -> u16 {
        self.post(
            "/auth/activate",
            json!({ "token": "f".repeat(64), "password": "password12345" }),
            forwarded_for,
        )
        .await
    }
}

async fn spawn_server(pool: PgPool, trusted_proxy_cidrs: &str) -> Server {
    let started = common::spawn_with(
        pool,
        common::Options {
            trusted_proxy_cidrs: trusted_proxy_cidrs.to_string(),
            ..Default::default()
        },
        |_| {},
    )
    .await;
    let addr = started.addr;
    Server {
        addr,
        client: reqwest::Client::new(),
    }
}

#[sqlx::test]
async fn a_trusted_proxy_gives_each_forwarded_client_its_own_bucket_and_an_untrusted_peer_cannot_spoof(
    pool: PgPool,
) {
    let behind_proxy = spawn_server(pool.clone(), "127.0.0.0/8").await;
    let exposed = spawn_server(pool, "").await;

    for i in 1..=15 {
        let client = format!("203.0.113.{i}");
        assert_eq!(
            behind_proxy.register(Some(&client)).await,
            400,
            "register as {client}"
        );
        assert_eq!(
            behind_proxy.login(Some(&client)).await,
            401,
            "login as {client}"
        );
        assert_eq!(
            behind_proxy.activate(Some(&client)).await,
            400,
            "activate as {client}"
        );
    }
    for _ in 0..10 {
        assert_eq!(behind_proxy.register(Some("198.51.100.9")).await, 400);
        assert_eq!(behind_proxy.login(Some("198.51.100.9")).await, 401);
        assert_eq!(behind_proxy.activate(Some("198.51.100.9")).await, 400);
    }
    assert_eq!(behind_proxy.register(Some("198.51.100.9")).await, 429);
    assert_eq!(behind_proxy.login(Some("198.51.100.9")).await, 429);
    assert_eq!(behind_proxy.activate(Some("198.51.100.9")).await, 429);
    // ...and cannot shake it off by prepending fake addresses: the proxy appends the real one on the right.
    assert_eq!(
        behind_proxy.register(Some("9.9.9.9, 198.51.100.9")).await,
        429
    );
    assert_eq!(behind_proxy.register(Some("198.51.100.10")).await, 400);
    assert_eq!(behind_proxy.register(None).await, 400);

    for i in 1..=10 {
        assert_eq!(
            exposed.register(Some(&format!("203.0.113.{i}"))).await,
            400,
            "attempt {i}"
        );
        assert_eq!(
            exposed.login(Some(&format!("203.0.113.{i}"))).await,
            401,
            "attempt {i}"
        );
    }
    assert_eq!(
        exposed.register(Some("203.0.113.99")).await,
        429,
        "a spoofed address is no way out of the limit"
    );
    assert_eq!(exposed.login(Some("203.0.113.98")).await, 429);
    assert_eq!(exposed.register(None).await, 429);
}
