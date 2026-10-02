mod common;

use common::http::{create_user, get_json, login, post, post_json};

use serde_json::json;
use sqlx::PgPool;

#[sqlx::test]
async fn the_member_listing_reports_the_callers_own_role_in_each_group(pool: PgPool) {
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

    create_user(&client, addr, &owner_jwt, "reader").await;
    let status = post(
        &client,
        addr,
        &owner_jwt,
        &format!("/groups/{group_id}/members"),
        &json!({ "username": "reader", "role": "reader" }),
    )
    .await
    .status();
    assert_eq!(status, 200);
    let reader_jwt = login(&client, addr, "reader", "password12345").await;

    let owner_list: serde_json::Value = get_json(&client, addr, &owner_jwt, "/groups/member").await;
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

    let reader_list: serde_json::Value =
        get_json(&client, addr, &reader_jwt, "/groups/member").await;
    let reader_entry = reader_list
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["id"] == group_id)
        .unwrap();
    assert_eq!(reader_entry["role"], "reader");
}
