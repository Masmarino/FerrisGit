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

async fn create_user(client: &reqwest::Client, addr: SocketAddr, owner_jwt: &str, username: &str) {
    client
        .post(format!("http://{addr}/api/admin/users"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "username": username, "email": format!("{username}@example.com"), "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
}

#[sqlx::test]
async fn the_member_listing_reports_the_callers_own_role_in_each_group(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;

    let group_res: serde_json::Value = client
        .post(format!("http://{addr}/api/groups"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "name": "acme" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let group_id = group_res["id"].as_str().unwrap().to_string();

    create_user(&client, addr, &owner_jwt, "reader").await;
    let status = client
        .post(format!("http://{addr}/api/groups/{group_id}/members"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "username": "reader", "role": "reader" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, 200);
    let reader_jwt = login(&client, addr, "reader", "password12345").await;

    let owner_list: serde_json::Value = client
        .get(format!("http://{addr}/api/groups/member"))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let owner_entry = owner_list
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["id"] == group_id)
        .unwrap();
    assert_eq!(
        owner_entry["role"], "maintainer",
        "the group's creator is auto-added as a Maintainer"
    );

    let reader_list: serde_json::Value = client
        .get(format!("http://{addr}/api/groups/member"))
        .bearer_auth(&reader_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let reader_entry = reader_list
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["id"] == group_id)
        .unwrap();
    assert_eq!(reader_entry["role"], "reader");
}
