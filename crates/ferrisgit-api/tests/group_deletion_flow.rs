mod common;

use common::http::{create_user, delete, get_json, login, post, post_json, post_ok};

use serde_json::json;
use sqlx::PgPool;

#[sqlx::test]
async fn the_creator_can_delete_their_own_empty_group(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;

    let group_res: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/groups",
        &json!({ "name": "acme" }),
    )
    .await;
    let group_id = group_res["id"].as_str().unwrap().to_string();

    let delete_status = delete(&client, addr, &owner_jwt, &format!("/groups/{group_id}"))
        .await
        .status();
    assert_eq!(delete_status, 204);

    let list: serde_json::Value = get_json(&client, addr, &owner_jwt, "/groups/member").await;
    assert!(
        list.as_array().unwrap().is_empty(),
        "the deleted group must no longer appear in the caller's group list"
    );
}

#[sqlx::test]
async fn deleting_a_group_that_still_has_a_subgroup_is_rejected(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;

    let group_res: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/groups",
        &json!({ "name": "acme" }),
    )
    .await;
    let group_id = group_res["id"].as_str().unwrap().to_string();
    post_ok(
        &client,
        addr,
        &owner_jwt,
        &format!("/groups/{group_id}/subgroups"),
        &json!({ "name": "backend" }),
    )
    .await;

    let delete_status = delete(&client, addr, &owner_jwt, &format!("/groups/{group_id}"))
        .await
        .status();
    assert_eq!(
        delete_status, 409,
        "a non-empty group must be rejected, not cascade-deleted"
    );
}

#[sqlx::test]
async fn deleting_a_group_that_still_has_a_repository_is_rejected(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;

    let group_res: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/groups",
        &json!({ "name": "acme" }),
    )
    .await;
    let group_id = group_res["id"].as_str().unwrap().to_string();
    post_ok(
        &client,
        addr,
        &owner_jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private", "groupPath": "acme" }),
    )
    .await;

    let delete_status = delete(&client, addr, &owner_jwt, &format!("/groups/{group_id}"))
        .await
        .status();
    assert_eq!(delete_status, 409);
}

#[sqlx::test]
async fn a_contributor_member_cannot_delete_a_group(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    create_user(&client, addr, &owner_jwt, "contributor").await;

    let group_res: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/groups",
        &json!({ "name": "acme" }),
    )
    .await;
    let group_id = group_res["id"].as_str().unwrap().to_string();
    let status = post(
        &client,
        addr,
        &owner_jwt,
        &format!("/groups/{group_id}/members"),
        &json!({ "username": "contributor", "role": "contributor" }),
    )
    .await
    .status();
    // add_member returns a bare Result<(), ApiError>, which axum renders as 200, not 204.
    assert_eq!(status, 200);
    let contributor_jwt = login(&client, addr, "contributor", "password12345").await;

    let delete_status = delete(
        &client,
        addr,
        &contributor_jwt,
        &format!("/groups/{group_id}"),
    )
    .await
    .status();
    assert_eq!(
        delete_status, 404,
        "a contributor must not be able to delete a group — masked as not-found"
    );
}

#[sqlx::test]
async fn deleting_an_unknown_group_returns_not_found(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;

    let delete_status = delete(
        &client,
        addr,
        &owner_jwt,
        &format!("/groups/{}", uuid::Uuid::new_v4()),
    )
    .await
    .status();
    assert_eq!(delete_status, 404);
}
