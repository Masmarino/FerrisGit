use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use serde_json::json;
use sqlx::PgPool;

/// `jwtTtlHours` must reject values that expire tokens immediately or make them as good as permanent (every session derives its lifetime from it).
#[sqlx::test]
async fn admin_settings_rejects_a_jwt_ttl_hours_outside_the_valid_range(pool: PgPool) {
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

    let login_res: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "admin", "password": "adminpassword123" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let jwt = login_res["token"].as_str().unwrap();

    let zero_res = client
        .put(format!("http://{addr}/api/admin/settings"))
        .bearer_auth(jwt)
        .json(&json!({ "jwtTtlHours": 0 }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        zero_res.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "0 hours must be rejected: it would expire every token immediately, including the admin's own"
    );

    let negative_res = client
        .put(format!("http://{addr}/api/admin/settings"))
        .bearer_auth(jwt)
        .json(&json!({ "jwtTtlHours": -5 }))
        .send()
        .await
        .unwrap();
    assert_eq!(negative_res.status(), reqwest::StatusCode::BAD_REQUEST);

    let too_large_res = client
        .put(format!("http://{addr}/api/admin/settings"))
        .bearer_auth(jwt)
        .json(&json!({ "jwtTtlHours": 721 }))
        .send()
        .await
        .unwrap();
    assert_eq!(too_large_res.status(), reqwest::StatusCode::BAD_REQUEST);

    let valid_res: serde_json::Value = client
        .put(format!("http://{addr}/api/admin/settings"))
        .bearer_auth(jwt)
        .json(&json!({ "jwtTtlHours": 48 }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(valid_res["jwtTtlHours"], 48);
}
