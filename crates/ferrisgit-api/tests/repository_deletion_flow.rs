use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use serde_json::json;
use sqlx::PgPool;
use std::net::SocketAddr;
use std::path::PathBuf;

async fn spawn_server(pool: PgPool) -> (SocketAddr, PathBuf) {
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
    (addr, storage_dir)
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

async fn add_collaborator(
    client: &reqwest::Client,
    addr: SocketAddr,
    owner_jwt: &str,
    repo_id: &str,
    username: &str,
    role: &str,
) {
    let status = client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/collaborators"
        ))
        .bearer_auth(owner_jwt)
        .json(&json!({ "username": username, "role": role }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, 204);
}

#[sqlx::test]
async fn the_owner_can_delete_their_own_repository_and_its_storage_directory_disappears(
    pool: PgPool,
) {
    let (addr, storage_dir) = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;

    let repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "name": "hello", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let repo_id = repo_res["id"].as_str().unwrap().to_string();
    // Personal-repo disk paths use `{owner_id}/{name}.git` and the admin's id is not at hand, so find the one owner directory.
    let owner_dir = std::fs::read_dir(&storage_dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let disk_path = owner_dir.join("hello.git");
    assert!(
        disk_path.exists(),
        "the bare repo directory must exist right after creation"
    );

    let delete_status = client
        .delete(format!("http://{addr}/api/repositories/by-id/{repo_id}"))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(delete_status, 204);

    let get_status = client
        .get(format!("http://{addr}/api/repositories/by-id/{repo_id}"))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        get_status, 404,
        "a deleted repository must no longer be reachable"
    );
    assert!(
        !disk_path.exists(),
        "the bare repo directory must be removed from disk after deletion"
    );
}

#[sqlx::test]
async fn a_maintainer_collaborator_can_delete_a_repository_they_do_not_own(pool: PgPool) {
    let (addr, _storage_dir) = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    create_user(&client, addr, &owner_jwt, "maintainer").await;

    let repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "name": "hello", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let repo_id = repo_res["id"].as_str().unwrap().to_string();
    add_collaborator(
        &client,
        addr,
        &owner_jwt,
        &repo_id,
        "maintainer",
        "maintainer",
    )
    .await;
    let maintainer_jwt = login(&client, addr, "maintainer", "password12345").await;

    let delete_status = client
        .delete(format!("http://{addr}/api/repositories/by-id/{repo_id}"))
        .bearer_auth(&maintainer_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(delete_status, 204);
}

#[sqlx::test]
async fn a_contributor_cannot_delete_a_repository(pool: PgPool) {
    let (addr, _storage_dir) = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    create_user(&client, addr, &owner_jwt, "contributor").await;

    let repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "name": "hello", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let repo_id = repo_res["id"].as_str().unwrap().to_string();
    add_collaborator(
        &client,
        addr,
        &owner_jwt,
        &repo_id,
        "contributor",
        "contributor",
    )
    .await;
    let contributor_jwt = login(&client, addr, "contributor", "password12345").await;

    let delete_status = client
        .delete(format!("http://{addr}/api/repositories/by-id/{repo_id}"))
        .bearer_auth(&contributor_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        delete_status, 404,
        "a contributor must not be able to delete a repository — masked as not-found, per this codebase's authz convention"
    );

    let get_status = client
        .get(format!("http://{addr}/api/repositories/by-id/{repo_id}"))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(get_status, 200, "the repository must still exist");
}

#[sqlx::test]
async fn deleting_an_unknown_repository_returns_not_found(pool: PgPool) {
    let (addr, _storage_dir) = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;

    let delete_status = client
        .delete(format!(
            "http://{addr}/api/repositories/by-id/{}",
            uuid::Uuid::new_v4()
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(delete_status, 404);
}
