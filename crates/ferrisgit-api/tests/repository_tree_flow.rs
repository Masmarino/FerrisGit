use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use serde_json::json;
use sqlx::PgPool;
use std::net::SocketAddr;
use std::process::Command;

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

async fn mint_api_token(
    client: &reqwest::Client,
    addr: SocketAddr,
    jwt: &str,
    name: &str,
) -> String {
    let res: serde_json::Value = client
        .post(format!("http://{addr}/api/tokens"))
        .bearer_auth(jwt)
        .json(&json!({ "name": name }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    res["token"].as_str().unwrap().to_string()
}

async fn create_repo(
    client: &reqwest::Client,
    addr: SocketAddr,
    owner_jwt: &str,
    name: &str,
) -> (String, String) {
    let repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": name, "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let repo_id = repo_res["id"].as_str().unwrap().to_string();
    let path_segments: Vec<String> = repo_res["path"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    (repo_id, path_segments.join("/"))
}

/// Every git call runs in `spawn_blocking`: a blocking `Command` deadlocks the single-threaded `#[sqlx::test]` runtime against the in-process `axum::serve` task.
async fn push_files(
    addr: SocketAddr,
    token: &str,
    repo_path_segment: &str,
    branch: &str,
    files: &[(&str, &[u8])],
) -> bool {
    let work_dir = tempfile::tempdir().unwrap();
    let work_dir_path = work_dir.path().to_path_buf();
    let clone_url = format!("http://admin:{token}@{addr}/{repo_path_segment}.git");
    let owned_files: Vec<(String, Vec<u8>)> = files
        .iter()
        .map(|(p, c)| (p.to_string(), c.to_vec()))
        .collect();
    let branch = branch.to_string();

    tokio::task::spawn_blocking(move || {
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(&work_dir_path)
            .status()
            .unwrap();
        for (path, content) in &owned_files {
            let full_path = work_dir_path.join(path);
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&full_path, content).unwrap();
        }
        Command::new("git")
            .args(["add", "."])
            .current_dir(&work_dir_path)
            .status()
            .unwrap();
        Command::new("git")
            .args([
                "-c",
                "user.email=a@b.c",
                "-c",
                "user.name=A",
                "commit",
                "-q",
                "-m",
                "seed",
            ])
            .current_dir(&work_dir_path)
            .status()
            .unwrap();
        Command::new("git")
            .args(["remote", "add", "origin", &clone_url])
            .current_dir(&work_dir_path)
            .status()
            .unwrap();
        Command::new("git")
            .args(["push", "-q", "origin", &format!("HEAD:{branch}")])
            .current_dir(&work_dir_path)
            .status()
            .unwrap()
            .success()
    })
    .await
    .unwrap()
}

#[sqlx::test]
async fn tree_lists_root_entries_at_head_by_default_branch_name(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    let token = mint_api_token(&client, addr, &owner_jwt, "push").await;

    assert!(
        push_files(
            addr,
            &token,
            &repo_path,
            "main",
            &[("README.md", b"# hello")]
        )
        .await
    );

    let response = client
        .get(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}/tree/main"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let entries: serde_json::Value = response.json().await.unwrap();
    assert_eq!(entries[0]["name"], "README.md");
    assert_eq!(entries[0]["isDir"], false);
}

#[sqlx::test]
async fn tree_at_head_alias_matches_the_default_branch(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    let token = mint_api_token(&client, addr, &owner_jwt, "push").await;

    assert!(push_files(addr, &token, &repo_path, "trunk", &[("a.txt", b"a")]).await);

    let response = client
        .get(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}/tree/HEAD"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let entries: serde_json::Value = response.json().await.unwrap();
    assert_eq!(entries[0]["name"], "a.txt");
}

#[sqlx::test]
async fn tree_lists_a_nested_subdirectory_via_the_wildcard_path(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    let token = mint_api_token(&client, addr, &owner_jwt, "push").await;

    assert!(
        push_files(
            addr,
            &token,
            &repo_path,
            "main",
            &[("src/main.rs", b"fn main() {}")]
        )
        .await
    );

    let response = client
        .get(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}/tree/main/src"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let entries: serde_json::Value = response.json().await.unwrap();
    assert_eq!(entries[0]["name"], "main.rs");
}

#[sqlx::test]
async fn tree_404s_for_an_unknown_ref(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, _repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;

    let response = client
        .get(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}/tree/does-not-exist"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[sqlx::test]
async fn a_reader_without_access_gets_404_not_the_tree(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    let token = mint_api_token(&client, addr, &owner_jwt, "push").await;
    assert!(push_files(addr, &token, &repo_path, "main", &[("a.txt", b"a")]).await);
    create_user(&client, addr, &owner_jwt, "stranger").await;
    let stranger_jwt = login(&client, addr, "stranger", "password12345").await;

    let response = client
        .get(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}/tree/main"
        ))
        .bearer_auth(&stranger_jwt)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[sqlx::test]
async fn blob_returns_a_text_files_content(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    let token = mint_api_token(&client, addr, &owner_jwt, "push").await;

    assert!(
        push_files(
            addr,
            &token,
            &repo_path,
            "main",
            &[("README.md", b"# hello world")]
        )
        .await
    );

    let response = client
        .get(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}/blob/main/README.md"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["content"], "# hello world");
    assert_eq!(body["isBinary"], false);
    assert_eq!(body["size"], 13);
}

#[sqlx::test]
async fn blob_detects_a_binary_file_and_omits_its_content(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    let token = mint_api_token(&client, addr, &owner_jwt, "push").await;

    assert!(
        push_files(
            addr,
            &token,
            &repo_path,
            "main",
            &[("image.bin", &[0u8, 1, 2, 0, 3])]
        )
        .await
    );

    let response = client
        .get(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}/blob/main/image.bin"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["isBinary"], true);
    assert_eq!(body["content"], serde_json::Value::Null);
}

#[sqlx::test]
async fn blob_404s_for_a_path_that_does_not_exist(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    let token = mint_api_token(&client, addr, &owner_jwt, "push").await;
    assert!(push_files(addr, &token, &repo_path, "main", &[("a.txt", b"a")]).await);

    let response = client
        .get(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}/blob/main/does-not-exist.txt"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[sqlx::test]
async fn blob_does_not_return_content_for_a_file_over_one_mebibyte(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    let token = mint_api_token(&client, addr, &owner_jwt, "push").await;
    let big = vec![b'a'; 2 * 1024 * 1024];
    assert!(push_files(addr, &token, &repo_path, "main", &[("big.txt", &big)]).await);

    let response = client
        .get(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}/blob/main/big.txt"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["content"], serde_json::Value::Null);
    assert_eq!(body["isBinary"], false);
    assert_eq!(body["size"], 2 * 1024 * 1024);
}

#[sqlx::test]
async fn readme_returns_the_root_readmes_content_case_insensitively(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    let token = mint_api_token(&client, addr, &owner_jwt, "push").await;

    assert!(
        push_files(
            addr,
            &token,
            &repo_path,
            "main",
            &[("readme.md", b"# Hello project")]
        )
        .await
    );

    let response = client
        .get(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}/readme/main"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["content"], "# Hello project");
}

#[sqlx::test]
async fn readme_returns_null_content_when_there_is_no_readme(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    let token = mint_api_token(&client, addr, &owner_jwt, "push").await;

    assert!(push_files(addr, &token, &repo_path, "main", &[("a.txt", b"a")]).await);

    let response = client
        .get(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}/readme/main"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["content"], serde_json::Value::Null);
}

#[sqlx::test]
async fn get_by_id_includes_star_count_is_starred_and_size(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, _repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;

    let response = client
        .get(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["starCount"], 0);
    assert_eq!(body["isStarred"], false);
    assert!(
        body["sizeBytes"].as_u64().is_some(),
        "sizeBytes must be present, got {body}"
    );
}

#[sqlx::test]
async fn starring_a_repository_increments_the_count_and_flips_is_starred(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, _repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;

    let star_response = client
        .post(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}/star"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(star_response.status(), 200);
    let star_body: serde_json::Value = star_response.json().await.unwrap();
    assert_eq!(star_body["starCount"], 1);
    assert_eq!(star_body["isStarred"], true);

    let get_response = client
        .get(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();
    let get_body: serde_json::Value = get_response.json().await.unwrap();
    assert_eq!(get_body["starCount"], 1);
    assert_eq!(get_body["isStarred"], true);
}

#[sqlx::test]
async fn unstarring_decrements_the_count_and_flips_is_starred_back(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, _repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    client
        .post(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}/star"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();

    let unstar_response = client
        .delete(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}/star"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();

    assert_eq!(unstar_response.status(), 200);
    let body: serde_json::Value = unstar_response.json().await.unwrap();
    assert_eq!(body["starCount"], 0);
    assert_eq!(body["isStarred"], false);
}

#[sqlx::test]
async fn starring_twice_does_not_double_count(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, _repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;

    client
        .post(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}/star"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();
    let second_response = client
        .post(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}/star"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();

    let body: serde_json::Value = second_response.json().await.unwrap();
    assert_eq!(body["starCount"], 1);
}

#[sqlx::test]
async fn contributors_tallies_commit_authors(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    let token = mint_api_token(&client, addr, &owner_jwt, "push").await;
    assert!(push_files(addr, &token, &repo_path, "main", &[("a.txt", b"a")]).await);

    let response = client
        .get(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}/contributors/main"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body[0]["email"], "a@b.c");
    assert_eq!(body[0]["commitCount"], 1);
}

#[sqlx::test]
async fn contributors_returns_an_empty_list_when_the_ref_does_not_exist(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, _repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;

    let response = client
        .get(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}/contributors/does-not-exist"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn languages_reports_a_byte_breakdown(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    let token = mint_api_token(&client, addr, &owner_jwt, "push").await;
    assert!(
        push_files(
            addr,
            &token,
            &repo_path,
            "main",
            &[("main.rs", b"fn main() {}")]
        )
        .await
    );

    let response = client
        .get(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}/languages/main"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["languages"][0]["name"], "Rust");
}

#[sqlx::test]
async fn languages_returns_an_empty_list_when_the_ref_does_not_exist(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, _repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;

    let response = client
        .get(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}/languages/does-not-exist"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["languages"].as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn a_stranger_without_access_cannot_star_a_private_repository(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, _repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    create_user(&client, addr, &owner_jwt, "stranger").await;
    let stranger_jwt = login(&client, addr, "stranger", "password12345").await;

    let response = client
        .post(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}/star"
        ))
        .bearer_auth(&stranger_jwt)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}
