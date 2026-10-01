use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use serde_json::json;
use sqlx::PgPool;

/// The `AdminUser` guard rejects a non-admin on every handler.
#[sqlx::test]
async fn admin_metrics_endpoints_are_admin_gated_and_shaped_as_expected(pool: PgPool) {
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
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();

    let admin_login: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "admin", "password": "adminpassword123" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let admin_jwt = admin_login["token"].as_str().unwrap();

    client
        .post(format!("http://{addr}/api/admin/users"))
        .bearer_auth(admin_jwt)
        .json(&json!({ "username": "regular", "email": "regular@example.com", "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let regular_login: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "regular", "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let regular_jwt = regular_login["token"].as_str().unwrap();

    for path in [
        "/api/admin/stats",
        "/api/admin/metrics/history",
        "/api/admin/health",
    ] {
        let res = client
            .get(format!("http://{addr}{path}"))
            .bearer_auth(regular_jwt)
            .send()
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            reqwest::StatusCode::UNAUTHORIZED,
            "non-admin must be rejected from {path}, got {}",
            res.status()
        );
    }

    let stats_res = client
        .get(format!("http://{addr}/api/admin/stats"))
        .bearer_auth(admin_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(stats_res.status(), reqwest::StatusCode::OK);
    let stats: serde_json::Value = stats_res.json().await.unwrap();
    assert!(
        stats.get("totalUsers").is_some(),
        "stats missing totalUsers: {stats}"
    );
    assert!(
        stats.get("totalRepositories").is_some(),
        "stats missing totalRepositories: {stats}"
    );
    assert!(
        stats.get("pipelinesLast7Days").is_some(),
        "stats missing pipelinesLast7Days: {stats}"
    );

    let health_res = client
        .get(format!("http://{addr}/api/admin/health"))
        .bearer_auth(admin_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(health_res.status(), reqwest::StatusCode::OK);
    let health: serde_json::Value = health_res.json().await.unwrap();
    assert!(
        health.get("database").is_some(),
        "health missing database: {health}"
    );
    assert!(
        health.get("storage").is_some(),
        "health missing storage: {health}"
    );
    assert!(
        health.get("uptimeSeconds").is_some(),
        "health missing uptimeSeconds: {health}"
    );

    let history_res = client
        .get(format!("http://{addr}/api/admin/metrics/history"))
        .bearer_auth(admin_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(history_res.status(), reqwest::StatusCode::OK);
    let history: serde_json::Value = history_res.json().await.unwrap();
    assert!(
        history.is_array(),
        "history response should be a JSON array: {history}"
    );

    let days_zero_res = client
        .get(format!("http://{addr}/api/admin/metrics/history?days=0"))
        .bearer_auth(admin_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        days_zero_res.status(),
        reqwest::StatusCode::OK,
        "days=0 must be clamped rather than rejected"
    );

    let days_huge_res = client
        .get(format!("http://{addr}/api/admin/metrics/history?days=9999"))
        .bearer_auth(admin_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        days_huge_res.status(),
        reqwest::StatusCode::OK,
        "days=9999 must be clamped rather than rejected"
    );
}
