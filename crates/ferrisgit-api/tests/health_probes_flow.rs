//! `/healthz` and `/readyz`, what the Kubernetes probes call: liveness must hold when the database does not, readiness
//! must follow the database, and neither says anything about why.

mod common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use ferrisgit_api::login_rate_limiter::LoginRateLimiter;
use ferrisgit_api::state::AppState;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::health::{ComponentHealth, DatabaseHealth, HealthCheckPort};
use ferrisgit_domain::public_pages::{
    PublicPagesSettings, PublicPagesSettingsPort, PublicPagesSettingsUpdate,
};
use reqwest::StatusCode;
use sqlx::PgPool;

struct Harness {
    addr: SocketAddr,
    client: reqwest::Client,
}

impl Harness {
    async fn get(&self, path: &str) -> reqwest::Response {
        self.client
            .get(format!("http://{}{path}", self.addr))
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .unwrap()
    }
}

async fn spawn(pool: PgPool, customize: impl FnOnce(&mut AppState)) -> Harness {
    let app = common::spawn_app_with(
        pool,
        common::Options {
            trusted_proxy_cidrs: "127.0.0.1/32".to_string(),
            ..Default::default()
        },
        |state| {
            // A budget of one request: the probes must not draw on it.
            state.public_rate_limiter =
                Arc::new(LoginRateLimiter::with_limits(1, Duration::from_secs(60)));
            customize(state);
        },
    )
    .await;
    Harness {
        addr: app.addr,
        client: reqwest::Client::new(),
    }
}

struct DownDatabase;

#[async_trait]
impl HealthCheckPort for DownDatabase {
    async fn check(&self) -> DatabaseHealth {
        DatabaseHealth {
            status: ComponentHealth::Down(
                "password authentication failed for user ferrisgit".into(),
            ),
            response_time_ms: 0,
            active_connections: 0,
            max_connections: 10,
            server_version: None,
        }
    }
}

struct HangingDatabase;

#[async_trait]
impl HealthCheckPort for HangingDatabase {
    async fn check(&self) -> DatabaseHealth {
        std::future::pending().await
    }
}

/// A settings store whose read never returns, like a database that accepts no connection.
struct HangingSettings;

#[async_trait]
impl PublicPagesSettingsPort for HangingSettings {
    async fn get(&self) -> Result<PublicPagesSettings, DomainError> {
        std::future::pending().await
    }

    async fn update(
        &self,
        _update: PublicPagesSettingsUpdate,
    ) -> Result<PublicPagesSettings, DomainError> {
        unimplemented!("not used by the probes")
    }
}

#[sqlx::test]
async fn both_probes_answer_ok_with_a_working_database(pool: PgPool) {
    let h = spawn(pool, |_| {}).await;

    for path in ["/health", "/healthz", "/readyz"] {
        let res = h.get(path).await;
        assert_eq!(res.status(), StatusCode::OK, "{path}");
        assert_eq!(res.text().await.unwrap(), "ok", "{path}");
    }
}

#[sqlx::test]
async fn the_probes_need_no_authentication_and_have_no_rate_limit(pool: PgPool) {
    let h = spawn(pool, |_| {}).await;

    // The public API budget is one request: a second call to a public API route is throttled...
    assert_eq!(
        h.get("/api/public/repositories").await.status(),
        StatusCode::OK
    );
    assert_eq!(
        h.get("/api/public/repositories").await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    // ...while the probes keep answering, however often they are polled.
    for _ in 0..5 {
        assert_eq!(h.get("/healthz").await.status(), StatusCode::OK);
        assert_eq!(h.get("/readyz").await.status(), StatusCode::OK);
    }
}

#[sqlx::test]
async fn readiness_fails_with_no_detail_when_the_database_is_closed(pool: PgPool) {
    let to_close = pool.clone();
    let h = spawn(pool, |_| {}).await;
    assert_eq!(h.get("/readyz").await.status(), StatusCode::OK);

    to_close.close().await;

    let ready = h.get("/readyz").await;
    assert_eq!(ready.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(ready.text().await.unwrap(), "unavailable");
    let live = h.get("/healthz").await;
    assert_eq!(
        live.status(),
        StatusCode::OK,
        "liveness does not depend on the database"
    );
    assert_eq!(live.text().await.unwrap(), "ok");
}

#[sqlx::test]
async fn a_database_error_is_not_leaked_by_readiness(pool: PgPool) {
    let h = spawn(pool, |state| state.health_check = Arc::new(DownDatabase)).await;

    let res = h.get("/readyz").await;
    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = res.text().await.unwrap();
    assert_eq!(body, "unavailable");
    assert!(!body.contains("password"), "{body}");
}

#[sqlx::test]
async fn readiness_gives_up_on_a_hanging_database_after_a_couple_of_seconds(pool: PgPool) {
    let h = spawn(pool, |state| state.health_check = Arc::new(HangingDatabase)).await;

    let started = std::time::Instant::now();
    let res = h.get("/readyz").await;
    let waited = started.elapsed();

    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(waited >= Duration::from_secs(2), "{waited:?}");
    assert!(waited < Duration::from_secs(5), "{waited:?}");
}

#[sqlx::test]
async fn liveness_does_not_wait_for_a_database_that_never_answers(pool: PgPool) {
    let h = spawn(pool, |state| {
        state.health_check = Arc::new(HangingDatabase);
        state.public_pages_settings = Arc::new(HangingSettings);
    })
    .await;

    // `get` times out after 10 s in the harness: a probe that touched the database would hang until then.
    let started = std::time::Instant::now();
    let res = h.get("/healthz").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.text().await.unwrap(), "ok");
    assert!(started.elapsed() < Duration::from_secs(1));
}
