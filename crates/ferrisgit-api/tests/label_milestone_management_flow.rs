mod common;

use common::http::{delete, get_json, patch, post, post_json, post_ok};

use common::http::login;
use serde_json::json;
use sqlx::PgPool;
use std::net::SocketAddr;

/// The reader can see the repo but is below the Contributor+ gate on the mutating routes.
async fn setup_repo_with_reader(
    client: &reqwest::Client,
    addr: SocketAddr,
    admin_jwt: &str,
) -> (String, String) {
    let repo_res: serde_json::Value = post_json(
        client,
        addr,
        admin_jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    let repo_id = repo_res["id"].as_str().unwrap().to_string();

    post_ok(client, addr, admin_jwt, "/admin/users", &json!({ "username": "reader", "email": "reader@example.com", "password": "password12345" })).await;
    post_ok(
        client,
        addr,
        admin_jwt,
        &format!("/repositories/{repo_id}/collaborators"),
        &json!({ "username": "reader", "role": "reader" }),
    )
    .await;
    let reader_jwt = login(client, addr, "reader", "password12345").await;

    (repo_id, reader_jwt)
}

#[sqlx::test]
async fn a_label_can_be_updated_and_deleted_over_http_and_a_reader_is_rejected_from_both(
    pool: PgPool,
) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let admin_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repo_id, reader_jwt) = setup_repo_with_reader(&client, addr, &admin_jwt).await;

    let created: serde_json::Value = post_json(
        &client,
        addr,
        &admin_jwt,
        &format!("/repositories/{repo_id}/labels"),
        &json!({ "name": "bug", "color": "#ff0000" }),
    )
    .await;
    let label_id = created["id"].as_str().unwrap().to_string();

    let reader_create_attempt = post(
        &client,
        addr,
        &reader_jwt,
        &format!("/repositories/{repo_id}/labels"),
        &json!({ "name": "wontfix", "color": "#00ff00" }),
    )
    .await;
    assert_eq!(reader_create_attempt.status(), 404);

    let reader_update_attempt = patch(
        &client,
        addr,
        &reader_jwt,
        &format!("/labels/{label_id}"),
        &json!({ "name": "renamed-by-reader", "color": "#000000" }),
    )
    .await;
    assert_eq!(
        reader_update_attempt.status(),
        404,
        "a Reader must not be able to update a label"
    );

    let updated: serde_json::Value = patch(
        &client,
        addr,
        &admin_jwt,
        &format!("/labels/{label_id}"),
        &json!({ "name": "critical-bug", "color": "#123456" }),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(updated["name"], "critical-bug");
    assert_eq!(updated["color"], "#123456");

    let listing: serde_json::Value = get_json(
        &client,
        addr,
        &admin_jwt,
        &format!("/repositories/{repo_id}/labels"),
    )
    .await;
    let names: Vec<&str> = listing
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        vec!["critical-bug"],
        "the update must be visible in a subsequent listing"
    );

    let reader_delete_attempt =
        delete(&client, addr, &reader_jwt, &format!("/labels/{label_id}")).await;
    assert_eq!(
        reader_delete_attempt.status(),
        404,
        "a Reader must not be able to delete a label"
    );

    let delete_res = delete(&client, addr, &admin_jwt, &format!("/labels/{label_id}")).await;
    assert_eq!(delete_res.status(), 204);

    let listing_after: serde_json::Value = get_json(
        &client,
        addr,
        &admin_jwt,
        &format!("/repositories/{repo_id}/labels"),
    )
    .await;
    assert!(
        listing_after.as_array().unwrap().is_empty(),
        "a deleted label must no longer appear in the listing"
    );
}

#[sqlx::test]
async fn a_milestone_can_be_updated_and_deleted_over_http_and_a_reader_is_rejected_from_both(
    pool: PgPool,
) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let admin_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repo_id, reader_jwt) = setup_repo_with_reader(&client, addr, &admin_jwt).await;

    let created: serde_json::Value = post_json(
        &client,
        addr,
        &admin_jwt,
        &format!("/repositories/{repo_id}/milestones"),
        &json!({ "title": "v1.0", "description": "first release", "dueDate": null }),
    )
    .await;
    let milestone_id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["state"], "open");

    let reader_create_attempt = post(
        &client,
        addr,
        &reader_jwt,
        &format!("/repositories/{repo_id}/milestones"),
        &json!({ "title": "v2.0", "description": "", "dueDate": null }),
    )
    .await;
    assert_eq!(reader_create_attempt.status(), 404);

    let reader_update_attempt = patch(&client, addr, &reader_jwt, &format!("/milestones/{milestone_id}"), &json!({ "title": "renamed-by-reader", "description": "", "dueDate": null, "state": "open" })).await;
    assert_eq!(
        reader_update_attempt.status(),
        404,
        "a Reader must not be able to update a milestone"
    );

    let updated: serde_json::Value = patch(&client, addr, &admin_jwt, &format!("/milestones/{milestone_id}"), &json!({ "title": "v1.0-final", "description": "shipped", "dueDate": null, "state": "closed" })).await
        .json()
        .await
        .unwrap();
    assert_eq!(updated["title"], "v1.0-final");
    assert_eq!(updated["description"], "shipped");
    assert_eq!(updated["state"], "closed");

    let listing: serde_json::Value = get_json(
        &client,
        addr,
        &admin_jwt,
        &format!("/repositories/{repo_id}/milestones"),
    )
    .await;
    let titles: Vec<&str> = listing
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["title"].as_str().unwrap())
        .collect();
    assert_eq!(
        titles,
        vec!["v1.0-final"],
        "the update must be visible in a subsequent listing"
    );

    let reader_delete_attempt = delete(
        &client,
        addr,
        &reader_jwt,
        &format!("/milestones/{milestone_id}"),
    )
    .await;
    assert_eq!(
        reader_delete_attempt.status(),
        404,
        "a Reader must not be able to delete a milestone"
    );

    let delete_res = delete(
        &client,
        addr,
        &admin_jwt,
        &format!("/milestones/{milestone_id}"),
    )
    .await;
    assert_eq!(delete_res.status(), 204);

    let listing_after: serde_json::Value = get_json(
        &client,
        addr,
        &admin_jwt,
        &format!("/repositories/{repo_id}/milestones"),
    )
    .await;
    assert!(
        listing_after.as_array().unwrap().is_empty(),
        "a deleted milestone must no longer appear in the listing"
    );
}
