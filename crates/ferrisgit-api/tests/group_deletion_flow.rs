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
async fn the_creator_can_delete_their_own_empty_group(pool: PgPool) {
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

    let delete_status = client
        .delete(format!("http://{addr}/api/groups/{group_id}"))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(delete_status, 204);

    let list: serde_json::Value = client
        .get(format!("http://{addr}/api/groups/member"))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        list.as_array().unwrap().is_empty(),
        "the deleted group must no longer appear in the caller's group list"
    );
}

#[sqlx::test]
async fn deleting_a_group_that_still_has_a_subgroup_is_rejected(pool: PgPool) {
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
    client
        .post(format!("http://{addr}/api/groups/{group_id}/subgroups"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "name": "backend" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let delete_status = client
        .delete(format!("http://{addr}/api/groups/{group_id}"))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        delete_status, 409,
        "a non-empty group must be rejected, not cascade-deleted"
    );
}

#[sqlx::test]
async fn deleting_a_group_that_still_has_a_repository_is_rejected(pool: PgPool) {
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
    client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "name": "hello", "visibility": "private", "groupPath": "acme" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let delete_status = client
        .delete(format!("http://{addr}/api/groups/{group_id}"))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(delete_status, 409);
}

#[sqlx::test]
async fn a_contributor_member_cannot_delete_a_group(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    create_user(&client, addr, &owner_jwt, "contributor").await;

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
    let status = client
        .post(format!("http://{addr}/api/groups/{group_id}/members"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "username": "contributor", "role": "contributor" }))
        .send()
        .await
        .unwrap()
        .status();
    // `add_member` returns a bare `Result<(), ApiError>`, which axum renders as 200, not 204.
    assert_eq!(status, 200);
    let contributor_jwt = login(&client, addr, "contributor", "password12345").await;

    let delete_status = client
        .delete(format!("http://{addr}/api/groups/{group_id}"))
        .bearer_auth(&contributor_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        delete_status, 404,
        "a contributor must not be able to delete a group — masked as not-found"
    );
}

#[sqlx::test]
async fn deleting_an_unknown_group_returns_not_found(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;

    let delete_status = client
        .delete(format!("http://{addr}/api/groups/{}", uuid::Uuid::new_v4()))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(delete_status, 404);
}
