use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use serde_json::json;
use sqlx::PgPool;
use std::net::SocketAddr;

async fn spawn_server(pool: PgPool) -> SocketAddr {
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

    let storage_dir = tempfile::tempdir().unwrap().keep();
    let static_dir = tempfile::tempdir().unwrap().keep();
    std::fs::write(static_dir.join("index.html"), "<html></html>").unwrap();

    let config = Config {
        database_url: String::new(),
        jwt_secret: "test-secret-that-is-at-least-32-characters-long".to_string(),
        storage_root: storage_dir.to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:0".to_string(),
        static_dir: static_dir.to_string_lossy().to_string(),
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

    let app = build_router(state, &static_dir);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

async fn login(
    client: &reqwest::Client,
    addr: SocketAddr,
    username: &str,
    password: &str,
) -> String {
    let res: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    res["token"].as_str().unwrap().to_string()
}

#[sqlx::test]
async fn revoking_a_token_stops_it_from_authenticating_a_subsequent_git_request(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let jwt = login(&client, addr, "admin", "adminpassword123").await;

    // Private repo: a public repo's Read bypass would let git succeed without credentials, defeating the test.
    let repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(&jwt)
        .json(&json!({ "name": "hello", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(repo_res["name"], "hello");

    let token_res: serde_json::Value = client
        .post(format!("http://{addr}/api/tokens"))
        .bearer_auth(&jwt)
        .json(&json!({ "name": "ci" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token_id = token_res["id"].as_str().unwrap().to_string();
    let plain_token = token_res["token"].as_str().unwrap().to_string();

    let before = client
        .get(format!(
            "http://{addr}/admin/hello.git/info/refs?service=git-upload-pack"
        ))
        .basic_auth("admin", Some(&plain_token))
        .send()
        .await
        .unwrap();
    assert_eq!(
        before.status(),
        200,
        "the freshly-created token must authenticate a git request before it is revoked"
    );

    let revoke = client
        .delete(format!("http://{addr}/api/tokens/{token_id}"))
        .bearer_auth(&jwt)
        .send()
        .await
        .unwrap();
    // This handler returns a bare `Result<(), ApiError>`, which axum renders as 200, not 204.
    assert_eq!(revoke.status(), 200);

    let after = client
        .get(format!(
            "http://{addr}/admin/hello.git/info/refs?service=git-upload-pack"
        ))
        .basic_auth("admin", Some(&plain_token))
        .send()
        .await
        .unwrap();
    assert_eq!(
        after.status(),
        401,
        "a revoked API token must stop authenticating git requests"
    );

    let listing: serde_json::Value = client
        .get(format!("http://{addr}/api/tokens"))
        .bearer_auth(&jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let remaining_ids: Vec<&str> = listing
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert!(
        !remaining_ids.contains(&token_id.as_str()),
        "the revoked token must be gone from the owner's own listing"
    );
}

#[sqlx::test]
async fn a_user_cannot_revoke_another_users_token(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let admin_jwt = login(&client, addr, "admin", "adminpassword123").await;

    client
        .post(format!("http://{addr}/api/admin/users"))
        .bearer_auth(&admin_jwt)
        .json(&json!({ "username": "alice", "email": "alice@example.com", "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let alice_jwt = login(&client, addr, "alice", "password12345").await;

    let alice_token_res: serde_json::Value = client
        .post(format!("http://{addr}/api/tokens"))
        .bearer_auth(&alice_jwt)
        .json(&json!({ "name": "alice-token" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let alice_token_id = alice_token_res["id"].as_str().unwrap().to_string();
    let alice_plain_token = alice_token_res["token"].as_str().unwrap().to_string();

    let cross_user_revoke = client
        .delete(format!("http://{addr}/api/tokens/{alice_token_id}"))
        .bearer_auth(&admin_jwt)
        .send()
        .await
        .unwrap();
    // `revoke` scopes its DELETE to `id AND user_id`, so another user's token looks identical to a missing one.
    assert_eq!(
        cross_user_revoke.status(),
        404,
        "revoking another user's token must not succeed"
    );

    let repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(&alice_jwt)
        .json(&json!({ "name": "alice-repo", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(repo_res["name"], "alice-repo");
    let still_works = client
        .get(format!(
            "http://{addr}/alice/alice-repo.git/info/refs?service=git-upload-pack"
        ))
        .basic_auth("alice", Some(&alice_plain_token))
        .send()
        .await
        .unwrap();
    assert_eq!(
        still_works.status(),
        200,
        "a token must remain valid after a cross-user revocation attempt was rejected"
    );
}
