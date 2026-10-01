use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test]
async fn only_a_maintainer_can_manage_webhooks_and_the_secret_never_leaks(pool: PgPool) {
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

    let owner_login: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "admin", "password": "adminpassword123" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let owner_jwt = owner_login["token"].as_str().unwrap();

    client
        .post(format!("http://{addr}/api/admin/users"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "username": "contributor-carl", "email": "carl@example.com", "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": "hello", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let repo_id = repo_res["id"].as_str().unwrap();

    client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/collaborators"
        ))
        .bearer_auth(owner_jwt)
        .json(&json!({ "username": "contributor-carl", "role": "contributor" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let carl_login: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "contributor-carl", "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let carl_jwt = carl_login["token"].as_str().unwrap();

    let forbidden = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/webhooks"))
        .bearer_auth(carl_jwt)
        .json(&json!({ "url": "https://example.com/hook", "secret": "shh", "events": ["issue_closed"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        forbidden.status(),
        reqwest::StatusCode::NOT_FOUND,
        "a Contributor must not be able to create a webhook (this codebase returns NotFound, not Forbidden, for an unauthorized repository action — see require_role_by_id)"
    );

    let created: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/webhooks"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "url": "https://example.com/hook", "secret": "shh", "events": ["issue_closed"] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(created["url"], "https://example.com/hook");
    assert!(
        created.get("secret").is_none(),
        "the secret must never appear in the create response"
    );
    assert!(created.get("secretCiphertext").is_none());

    let listed: serde_json::Value = client
        .get(format!("http://{addr}/api/repositories/{repo_id}/webhooks"))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let body_text = listed.to_string();
    assert!(
        !body_text.contains("shh"),
        "the plaintext secret must never appear anywhere in a list response"
    );
}

/// Regression (cross-repository IDOR): `update` and `deliveries` must check that the webhook belongs to the path's repository, not only authorize on it.
/// The admin owns both repositories, so any 404 can only come from the ownership check.
#[sqlx::test]
async fn a_maintainer_of_one_repository_cannot_touch_a_webhook_that_belongs_to_another(
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
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();

    let owner_login: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "admin", "password": "adminpassword123" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let owner_jwt = owner_login["token"].as_str().unwrap();

    let repo_a: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": "repo-a", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let repo_a_id = repo_a["id"].as_str().unwrap();

    // Repository B, owned by the same admin but a different repository, holds the webhook under attack.
    let repo_b: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": "repo-b", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let repo_b_id = repo_b["id"].as_str().unwrap();

    let webhook_on_b: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories/{repo_b_id}/webhooks"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "url": "https://example.com/repo-b-hook", "secret": "b-secret", "events": ["issue_closed"] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let webhook_b_id = webhook_on_b["id"].as_str().unwrap();

    // The URL must be a real, resolvable, non-private hostname: the SSRF guard runs before the ownership check and would otherwise mask the 404 with a 400.
    let cross_repo_update = client
        .patch(format!(
            "http://{addr}/api/repositories/{repo_a_id}/webhooks/{webhook_b_id}"
        ))
        .bearer_auth(owner_jwt)
        .json(&json!({ "url": "https://example.com/hijacked" }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        cross_repo_update.status(),
        reqwest::StatusCode::NOT_FOUND,
        "a Maintainer of repo A must not be able to update a webhook that actually belongs to repo B"
    );

    let cross_repo_deliveries = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_a_id}/webhooks/{webhook_b_id}/deliveries"
        ))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        cross_repo_deliveries.status(),
        reqwest::StatusCode::NOT_FOUND,
        "a Maintainer of repo A must not be able to read delivery history for a webhook that actually belongs to repo B"
    );

    let still_on_b: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_b_id}/webhooks"
        ))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let urls: Vec<&str> = still_on_b
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["url"].as_str().unwrap())
        .collect();
    assert_eq!(
        urls,
        vec!["https://example.com/repo-b-hook"],
        "the cross-repository update attempt must not have mutated repo B's webhook"
    );

    let legit_deliveries = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_b_id}/webhooks/{webhook_b_id}/deliveries"
        ))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        legit_deliveries.status(),
        reqwest::StatusCode::OK,
        "reading deliveries for a webhook via its OWN repository must still work"
    );
}

/// Regression (blind SSRF): a webhook pointed at an internal address such as loopback must be rejected.
#[sqlx::test]
async fn creating_a_webhook_pointed_at_loopback_is_rejected(pool: PgPool) {
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

    let owner_login: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "admin", "password": "adminpassword123" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let owner_jwt = owner_login["token"].as_str().unwrap();

    let repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": "hello", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let repo_id = repo_res["id"].as_str().unwrap();

    let rejected = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/webhooks"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "url": "http://127.0.0.1/whatever", "secret": "shh", "events": ["issue_closed"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        rejected.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "a webhook url pointed at loopback must be rejected"
    );

    let listed: serde_json::Value = client
        .get(format!("http://{addr}/api/repositories/{repo_id}/webhooks"))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        listed.as_array().unwrap().len(),
        0,
        "the rejected webhook must not have been created"
    );
}

/// Regression: the number of webhooks per repository is capped, and the 21st is rejected.
#[sqlx::test]
async fn the_21st_webhook_on_one_repository_is_rejected(pool: PgPool) {
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

    let owner_login: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "admin", "password": "adminpassword123" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let owner_jwt = owner_login["token"].as_str().unwrap();

    let repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": "hello", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let repo_id = repo_res["id"].as_str().unwrap();

    for i in 0..20 {
        let created = client
            .post(format!("http://{addr}/api/repositories/{repo_id}/webhooks"))
            .bearer_auth(owner_jwt)
            .json(&json!({ "url": format!("https://example.com/hook-{i}"), "secret": "shh", "events": ["issue_closed"] }))
            .send()
            .await
            .unwrap();
        assert_eq!(
            created.status(),
            reqwest::StatusCode::OK,
            "webhook {i} (within the cap) must be created successfully"
        );
    }

    let rejected = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/webhooks"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "url": "https://example.com/hook-21", "secret": "shh", "events": ["issue_closed"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        rejected.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "the 21st webhook on one repository must be rejected"
    );

    let listed: serde_json::Value = client
        .get(format!("http://{addr}/api/repositories/{repo_id}/webhooks"))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        listed.as_array().unwrap().len(),
        20,
        "the repository must still have exactly 20 webhooks"
    );
}
