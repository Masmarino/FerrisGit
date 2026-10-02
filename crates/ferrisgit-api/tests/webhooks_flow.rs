mod common;

use common::http::{get, get_json, login, patch, post, post_json, post_ok};

use serde_json::json;
use sqlx::PgPool;

#[sqlx::test]
async fn only_a_maintainer_can_manage_webhooks_and_the_secret_never_leaks(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;

    let client = reqwest::Client::new();

    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;

    post_ok(&client, addr, &owner_jwt, "/admin/users", &json!({ "username": "contributor-carl", "email": "carl@example.com", "password": "password12345" })).await;

    let repo_res: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    let repo_id = repo_res["id"].as_str().unwrap();

    post_ok(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/collaborators"),
        &json!({ "username": "contributor-carl", "role": "contributor" }),
    )
    .await;

    let carl_jwt = login(&client, addr, "contributor-carl", "password12345").await;

    let forbidden = post(
        &client,
        addr,
        &carl_jwt,
        &format!("/repositories/{repo_id}/webhooks"),
        &json!({ "url": "https://example.com/hook", "secret": "shh", "events": ["issue_closed"] }),
    )
    .await;
    assert_eq!(
        forbidden.status(),
        reqwest::StatusCode::NOT_FOUND,
        "a Contributor must not be able to create a webhook (this codebase returns NotFound, not Forbidden, for an unauthorized repository action — see require_role_by_id)"
    );

    let created: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/webhooks"),
        &json!({ "url": "https://example.com/hook", "secret": "shh", "events": ["issue_closed"] }),
    )
    .await;
    assert_eq!(created["url"], "https://example.com/hook");
    assert!(
        created.get("secret").is_none(),
        "the secret must never appear in the create response"
    );
    assert!(created.get("secretCiphertext").is_none());

    let listed: serde_json::Value = get_json(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/webhooks"),
    )
    .await;
    let body_text = listed.to_string();
    assert!(
        !body_text.contains("shh"),
        "the plaintext secret must never appear anywhere in a list response"
    );
}

/// Cross-repository IDOR: `update` and `deliveries` have to check that the webhook belongs to the repository in the path,
/// not just authorize on it. The admin owns both repositories, so a 404 can only come from the ownership check.
#[sqlx::test]
async fn a_maintainer_of_one_repository_cannot_touch_a_webhook_that_belongs_to_another(
    pool: PgPool,
) {
    let addr = common::spawn_app(pool).await.addr;

    let client = reqwest::Client::new();

    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;

    let repo_a: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/repositories",
        &json!({ "name": "repo-a", "visibility": "private" }),
    )
    .await;
    let repo_a_id = repo_a["id"].as_str().unwrap();

    // Repository B, also the admin's, holds the webhook being attacked.
    let repo_b: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/repositories",
        &json!({ "name": "repo-b", "visibility": "private" }),
    )
    .await;
    let repo_b_id = repo_b["id"].as_str().unwrap();

    let webhook_on_b: serde_json::Value = post_json(&client, addr, &owner_jwt, &format!("/repositories/{repo_b_id}/webhooks"), &json!({ "url": "https://example.com/repo-b-hook", "secret": "b-secret", "events": ["issue_closed"] })).await;
    let webhook_b_id = webhook_on_b["id"].as_str().unwrap();

    // Use a real, resolvable, non-private hostname: the SSRF guard runs before the ownership check and would otherwise turn the 404 into a 400.
    let cross_repo_update = patch(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_a_id}/webhooks/{webhook_b_id}"),
        &json!({ "url": "https://example.com/hijacked" }),
    )
    .await;
    assert_eq!(
        cross_repo_update.status(),
        reqwest::StatusCode::NOT_FOUND,
        "a Maintainer of repo A must not be able to update a webhook that actually belongs to repo B"
    );

    let cross_repo_deliveries = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_a_id}/webhooks/{webhook_b_id}/deliveries"),
    )
    .await;
    assert_eq!(
        cross_repo_deliveries.status(),
        reqwest::StatusCode::NOT_FOUND,
        "a Maintainer of repo A must not be able to read delivery history for a webhook that actually belongs to repo B"
    );

    let still_on_b: serde_json::Value = get_json(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_b_id}/webhooks"),
    )
    .await;
    let urls: Vec<&str> = still_on_b
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["url"].as_str().unwrap())
        .collect();
    assert_eq!(
        urls,
        vec!["https://example.com/repo-b-hook"],
        "the cross-repository update attempt must not have mutated repo B's webhook"
    );

    let legit_deliveries = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_b_id}/webhooks/{webhook_b_id}/deliveries"),
    )
    .await;
    assert_eq!(
        legit_deliveries.status(),
        reqwest::StatusCode::OK,
        "reading deliveries for a webhook via its OWN repository must still work"
    );
}

/// Blind SSRF: a webhook pointing at an internal address such as loopback is rejected.
#[sqlx::test]
async fn creating_a_webhook_pointed_at_loopback_is_rejected(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;

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
    let repo_id = repo_res["id"].as_str().unwrap();

    let rejected = post(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/webhooks"),
        &json!({ "url": "http://127.0.0.1/whatever", "secret": "shh", "events": ["issue_closed"] }),
    )
    .await;
    assert_eq!(
        rejected.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "a webhook url pointed at loopback must be rejected"
    );

    let listed: serde_json::Value = get_json(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/webhooks"),
    )
    .await;
    assert_eq!(
        listed.as_array().unwrap().len(),
        0,
        "the rejected webhook must not have been created"
    );
}

/// Webhooks per repository are capped, the 21st is rejected.
#[sqlx::test]
async fn the_21st_webhook_on_one_repository_is_rejected(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;

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
    let repo_id = repo_res["id"].as_str().unwrap();

    for i in 0..20 {
        let created = post(&client, addr, &owner_jwt, &format!("/repositories/{repo_id}/webhooks"), &json!({ "url": format!("https://example.com/hook-{i}"), "secret": "shh", "events": ["issue_closed"] })).await;
        assert_eq!(
            created.status(),
            reqwest::StatusCode::OK,
            "webhook {i} (within the cap) must be created successfully"
        );
    }

    let rejected = post(&client, addr, &owner_jwt, &format!("/repositories/{repo_id}/webhooks"), &json!({ "url": "https://example.com/hook-21", "secret": "shh", "events": ["issue_closed"] })).await;
    assert_eq!(
        rejected.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "the 21st webhook on one repository must be rejected"
    );

    let listed: serde_json::Value = get_json(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/webhooks"),
    )
    .await;
    assert_eq!(
        listed.as_array().unwrap().len(),
        20,
        "the repository must still have exactly 20 webhooks"
    );
}
