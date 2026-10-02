mod common;

use common::http::post_anon;

use serde_json::json;
use sqlx::PgPool;

/// Regression: login had no rate limit. The fixed-window limiter must trip after repeated attempts from one connection.
#[sqlx::test]
async fn repeated_login_attempts_from_the_same_connection_are_eventually_rate_limited(
    pool: PgPool,
) {
    let addr = common::spawn_app(pool).await.addr;

    let client = reqwest::Client::new();
    let attempt = || {
        let client = client.clone();
        async move {
            post_anon(
                &client,
                addr,
                "/auth/login",
                &json!({ "username": "admin", "password": "wrong-password" }),
            )
            .await
            .status()
        }
    };

    let mut saw_unauthorized = false;
    let mut saw_rate_limited = false;
    for _ in 0..15 {
        match attempt().await {
            reqwest::StatusCode::UNAUTHORIZED => saw_unauthorized = true,
            reqwest::StatusCode::TOO_MANY_REQUESTS => saw_rate_limited = true,
            other => panic!("unexpected status: {other}"),
        }
    }

    assert!(
        saw_unauthorized,
        "attempts within the window's budget must fail normally (wrong password)"
    );
    assert!(
        saw_rate_limited,
        "attempts past the window's budget must be rejected with 429, got only: {saw_unauthorized}"
    );

    // A correct password does not bypass the limiter: it is a connection-level gate, not a failed-attempt counter.
    let still_limited = post_anon(
        &client,
        addr,
        "/auth/login",
        &json!({ "username": "admin", "password": "adminpassword123" }),
    )
    .await;
    assert_eq!(
        still_limited.status(),
        reqwest::StatusCode::TOO_MANY_REQUESTS
    );
}
