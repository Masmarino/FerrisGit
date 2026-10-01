use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use serde_json::json;
use sqlx::PgPool;
use std::net::SocketAddr;

/// Regression: login had no rate limit. The fixed-window limiter must trip after repeated attempts from one connection.
#[sqlx::test]
async fn repeated_login_attempts_from_the_same_connection_are_eventually_rate_limited(
    pool: PgPool,
) {
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

    let storage_dir = tempfile::tempdir().unwrap();
    let static_dir = tempfile::tempdir().unwrap();
    std::fs::write(static_dir.path().join("index.html"), "<html></html>").unwrap();

    let config = Config {
        database_url: String::new(),
        jwt_secret: "test-secret-that-is-at-least-32-characters-long".to_string(),
        storage_root: storage_dir.path().to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:0".to_string(),
        static_dir: static_dir.path().to_string_lossy().to_string(),
        bootstrap_admin_username: Some("admin".to_string()),
        bootstrap_admin_password: Some("adminpassword123".to_string()),
        settings_encryption_key: [b'k'; 32],
        public_url: "http://localhost:4200".to_string(),
        trusted_proxy_cidrs: vec![],
    };

    let mut state = AppState::new(pool, config.clone()).await;
    state.mfa_enforced = false; // these tests are not about MFA: they log in with a plain session
    BootstrapAdminUseCase::new(state.users.clone(), state.hasher.clone())
        .execute(
            config.bootstrap_admin_username.clone(),
            config.bootstrap_admin_password.clone(),
        )
        .await
        .unwrap();

    let app = build_router(state, static_dir.path());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    let client = reqwest::Client::new();
    let attempt = || {
        let client = client.clone();
        async move {
            client
                .post(format!("http://{addr}/api/auth/login"))
                .json(&json!({ "username": "admin", "password": "wrong-password" }))
                .send()
                .await
                .unwrap()
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
    let still_limited = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "admin", "password": "adminpassword123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        still_limited.status(),
        reqwest::StatusCode::TOO_MANY_REQUESTS
    );
}
