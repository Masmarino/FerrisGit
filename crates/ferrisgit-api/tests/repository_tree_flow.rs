mod common;

use common::http::{create_user, delete, get, login, mint_api_token, post_empty, post_json};

use serde_json::json;
use sqlx::PgPool;
use std::net::SocketAddr;

async fn create_repo(
    client: &reqwest::Client,
    addr: SocketAddr,
    owner_jwt: &str,
    name: &str,
) -> (String, String) {
    let repo_res: serde_json::Value = post_json(
        client,
        addr,
        owner_jwt,
        "/repositories",
        &json!({ "name": name, "visibility": "private" }),
    )
    .await;
    let repo_id = repo_res["id"].as_str().unwrap().to_string();
    let path_segments: Vec<String> = repo_res["path"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    (repo_id, path_segments.join("/"))
}

async fn push_files(
    addr: SocketAddr,
    token: &str,
    repo_path_segment: &str,
    branch: &str,
    files: &[(&str, &[u8])],
) -> bool {
    let remote_url = format!("http://admin:{token}@{addr}/{repo_path_segment}.git");
    common::git::push_files(&remote_url, branch, "seed", files).await
}

#[sqlx::test]
async fn tree_lists_root_entries_at_head_by_default_branch_name(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
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

    let response = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repository_id}/tree/main"),
    )
    .await;

    assert_eq!(response.status(), 200);
    let entries: serde_json::Value = response.json().await.unwrap();
    assert_eq!(entries[0]["name"], "README.md");
    assert_eq!(entries[0]["isDir"], false);
}

#[sqlx::test]
async fn tree_at_head_alias_matches_the_default_branch(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    let token = mint_api_token(&client, addr, &owner_jwt, "push").await;

    assert!(push_files(addr, &token, &repo_path, "trunk", &[("a.txt", b"a")]).await);

    let response = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repository_id}/tree/HEAD"),
    )
    .await;

    assert_eq!(response.status(), 200);
    let entries: serde_json::Value = response.json().await.unwrap();
    assert_eq!(entries[0]["name"], "a.txt");
}

#[sqlx::test]
async fn tree_lists_a_nested_subdirectory_via_the_wildcard_path(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
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

    let response = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repository_id}/tree/main/src"),
    )
    .await;

    assert_eq!(response.status(), 200);
    let entries: serde_json::Value = response.json().await.unwrap();
    assert_eq!(entries[0]["name"], "main.rs");
}

#[sqlx::test]
async fn tree_404s_for_an_unknown_ref(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, _repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;

    let response = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repository_id}/tree/does-not-exist"),
    )
    .await;

    assert_eq!(response.status(), 404);
}

#[sqlx::test]
async fn a_reader_without_access_gets_404_not_the_tree(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    let token = mint_api_token(&client, addr, &owner_jwt, "push").await;
    assert!(push_files(addr, &token, &repo_path, "main", &[("a.txt", b"a")]).await);
    create_user(&client, addr, &owner_jwt, "stranger").await;
    let stranger_jwt = login(&client, addr, "stranger", "password12345").await;

    let response = get(
        &client,
        addr,
        &stranger_jwt,
        &format!("/repositories/by-id/{repository_id}/tree/main"),
    )
    .await;

    assert_eq!(response.status(), 404);
}

#[sqlx::test]
async fn blob_returns_a_text_files_content(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
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

    let response = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repository_id}/blob/main/README.md"),
    )
    .await;

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["content"], "# hello world");
    assert_eq!(body["isBinary"], false);
    assert_eq!(body["size"], 13);
}

#[sqlx::test]
async fn blob_detects_a_binary_file_and_omits_its_content(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
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

    let response = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repository_id}/blob/main/image.bin"),
    )
    .await;

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["isBinary"], true);
    assert_eq!(body["content"], serde_json::Value::Null);
}

#[sqlx::test]
async fn blob_404s_for_a_path_that_does_not_exist(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    let token = mint_api_token(&client, addr, &owner_jwt, "push").await;
    assert!(push_files(addr, &token, &repo_path, "main", &[("a.txt", b"a")]).await);

    let response = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repository_id}/blob/main/does-not-exist.txt"),
    )
    .await;

    assert_eq!(response.status(), 404);
}

#[sqlx::test]
async fn blob_does_not_return_content_for_a_file_over_one_mebibyte(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    let token = mint_api_token(&client, addr, &owner_jwt, "push").await;
    let big = vec![b'a'; 2 * 1024 * 1024];
    assert!(push_files(addr, &token, &repo_path, "main", &[("big.txt", &big)]).await);

    let response = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repository_id}/blob/main/big.txt"),
    )
    .await;

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["content"], serde_json::Value::Null);
    assert_eq!(body["isBinary"], false);
    assert_eq!(body["size"], 2 * 1024 * 1024);
}

#[sqlx::test]
async fn readme_returns_the_root_readmes_content_case_insensitively(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
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

    let response = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repository_id}/readme/main"),
    )
    .await;

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["content"], "# Hello project");
}

#[sqlx::test]
async fn readme_returns_null_content_when_there_is_no_readme(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    let token = mint_api_token(&client, addr, &owner_jwt, "push").await;

    assert!(push_files(addr, &token, &repo_path, "main", &[("a.txt", b"a")]).await);

    let response = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repository_id}/readme/main"),
    )
    .await;

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["content"], serde_json::Value::Null);
}

#[sqlx::test]
async fn get_by_id_includes_star_count_is_starred_and_size(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, _repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;

    let response = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repository_id}"),
    )
    .await;

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
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, _repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;

    let star_response = post_empty(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repository_id}/star"),
    )
    .await;
    assert_eq!(star_response.status(), 200);
    let star_body: serde_json::Value = star_response.json().await.unwrap();
    assert_eq!(star_body["starCount"], 1);
    assert_eq!(star_body["isStarred"], true);

    let get_response = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repository_id}"),
    )
    .await;
    let get_body: serde_json::Value = get_response.json().await.unwrap();
    assert_eq!(get_body["starCount"], 1);
    assert_eq!(get_body["isStarred"], true);
}

#[sqlx::test]
async fn unstarring_decrements_the_count_and_flips_is_starred_back(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, _repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    post_empty(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repository_id}/star"),
    )
    .await;

    let unstar_response = delete(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repository_id}/star"),
    )
    .await;

    assert_eq!(unstar_response.status(), 200);
    let body: serde_json::Value = unstar_response.json().await.unwrap();
    assert_eq!(body["starCount"], 0);
    assert_eq!(body["isStarred"], false);
}

#[sqlx::test]
async fn starring_twice_does_not_double_count(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, _repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;

    post_empty(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repository_id}/star"),
    )
    .await;
    let second_response = post_empty(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repository_id}/star"),
    )
    .await;

    let body: serde_json::Value = second_response.json().await.unwrap();
    assert_eq!(body["starCount"], 1);
}

#[sqlx::test]
async fn contributors_tallies_commit_authors(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    let token = mint_api_token(&client, addr, &owner_jwt, "push").await;
    assert!(push_files(addr, &token, &repo_path, "main", &[("a.txt", b"a")]).await);

    let response = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repository_id}/contributors/main"),
    )
    .await;

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body[0]["email"], "a@b.c");
    assert_eq!(body[0]["commitCount"], 1);
}

#[sqlx::test]
async fn contributors_returns_an_empty_list_when_the_ref_does_not_exist(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, _repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;

    let response = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repository_id}/contributors/does-not-exist"),
    )
    .await;

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn languages_reports_a_byte_breakdown(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
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

    let response = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repository_id}/languages/main"),
    )
    .await;

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["languages"][0]["name"], "Rust");
}

#[sqlx::test]
async fn languages_returns_an_empty_list_when_the_ref_does_not_exist(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, _repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;

    let response = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repository_id}/languages/does-not-exist"),
    )
    .await;

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["languages"].as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn a_stranger_without_access_cannot_star_a_private_repository(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repository_id, _repo_path) = create_repo(&client, addr, &owner_jwt, "hello").await;
    create_user(&client, addr, &owner_jwt, "stranger").await;
    let stranger_jwt = login(&client, addr, "stranger", "password12345").await;

    let response = post_empty(
        &client,
        addr,
        &stranger_jwt,
        &format!("/repositories/by-id/{repository_id}/star"),
    )
    .await;

    assert_eq!(response.status(), 404);
}
