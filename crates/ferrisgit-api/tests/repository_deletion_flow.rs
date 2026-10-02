mod common;

use common::http::{add_collaborator, create_user, delete, get, login, post_json};

use serde_json::json;
use sqlx::PgPool;
use std::net::SocketAddr;
use std::path::PathBuf;

async fn spawn_server(pool: PgPool) -> (SocketAddr, PathBuf) {
    let app = common::spawn_app(pool).await;
    (app.addr, app.storage_root)
}

#[sqlx::test]
async fn the_owner_can_delete_their_own_repository_and_its_storage_directory_disappears(
    pool: PgPool,
) {
    let (addr, storage_dir) = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;

    let repo_res: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
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

    let delete_status = delete(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repo_id}"),
    )
    .await
    .status();
    assert_eq!(delete_status, 204);

    let get_status = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repo_id}"),
    )
    .await
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

    let repo_res: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
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

    let delete_status = delete(
        &client,
        addr,
        &maintainer_jwt,
        &format!("/repositories/by-id/{repo_id}"),
    )
    .await
    .status();
    assert_eq!(delete_status, 204);
}

#[sqlx::test]
async fn a_contributor_cannot_delete_a_repository(pool: PgPool) {
    let (addr, _storage_dir) = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    create_user(&client, addr, &owner_jwt, "contributor").await;

    let repo_res: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
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

    let delete_status = delete(
        &client,
        addr,
        &contributor_jwt,
        &format!("/repositories/by-id/{repo_id}"),
    )
    .await
    .status();
    assert_eq!(
        delete_status, 404,
        "a contributor must not be able to delete a repository — masked as not-found, per this codebase's authz convention"
    );

    let get_status = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{repo_id}"),
    )
    .await
    .status();
    assert_eq!(get_status, 200, "the repository must still exist");
}

#[sqlx::test]
async fn deleting_an_unknown_repository_returns_not_found(pool: PgPool) {
    let (addr, _storage_dir) = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;

    let delete_status = delete(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/by-id/{}", uuid::Uuid::new_v4()),
    )
    .await
    .status();
    assert_eq!(delete_status, 404);
}
