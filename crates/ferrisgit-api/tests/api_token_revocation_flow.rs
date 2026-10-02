mod common;

use common::http::{create_user, delete, get_json, post_json};

use common::http::login;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test]
async fn revoking_a_token_stops_it_from_authenticating_a_subsequent_git_request(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let jwt = login(&client, addr, "admin", "adminpassword123").await;

    // Private repo: a public one would let git read without credentials and defeat the test.
    let repo_res: serde_json::Value = post_json(
        &client,
        addr,
        &jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    assert_eq!(repo_res["name"], "hello");

    let token_res: serde_json::Value =
        post_json(&client, addr, &jwt, "/tokens", &json!({ "name": "ci" })).await;
    let token_id = token_res["id"].as_str().unwrap().to_string();
    let plain_token = token_res["token"].as_str().unwrap().to_string();

    let before = client
        .get(format!(
            "http://{addr}/admin/hello.git/info/refs?service=git-upload-pack"
        ))
        .basic_auth("admin", Some(&plain_token))
        .send()
        .await
        .unwrap();
    assert_eq!(
        before.status(),
        200,
        "the freshly-created token must authenticate a git request before it is revoked"
    );

    let revoke = delete(&client, addr, &jwt, &format!("/tokens/{token_id}")).await;
    // This handler returns a bare Result<(), ApiError>, which axum renders as 200, not 204.
    assert_eq!(revoke.status(), 200);

    let after = client
        .get(format!(
            "http://{addr}/admin/hello.git/info/refs?service=git-upload-pack"
        ))
        .basic_auth("admin", Some(&plain_token))
        .send()
        .await
        .unwrap();
    assert_eq!(
        after.status(),
        401,
        "a revoked API token must stop authenticating git requests"
    );

    let listing: serde_json::Value = get_json(&client, addr, &jwt, "/tokens").await;
    let remaining_ids: Vec<&str> = listing
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert!(
        !remaining_ids.contains(&token_id.as_str()),
        "the revoked token must be gone from the owner's own listing"
    );
}

#[sqlx::test]
async fn a_user_cannot_revoke_another_users_token(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let admin_jwt = login(&client, addr, "admin", "adminpassword123").await;

    create_user(&client, addr, &admin_jwt, "alice").await;
    let alice_jwt = login(&client, addr, "alice", "password12345").await;

    let alice_token_res: serde_json::Value = post_json(
        &client,
        addr,
        &alice_jwt,
        "/tokens",
        &json!({ "name": "alice-token" }),
    )
    .await;
    let alice_token_id = alice_token_res["id"].as_str().unwrap().to_string();
    let alice_plain_token = alice_token_res["token"].as_str().unwrap().to_string();

    let cross_user_revoke = delete(
        &client,
        addr,
        &admin_jwt,
        &format!("/tokens/{alice_token_id}"),
    )
    .await;
    // revoke scopes its DELETE to id and user_id, so someone else's token looks like a missing one.
    assert_eq!(
        cross_user_revoke.status(),
        404,
        "revoking another user's token must not succeed"
    );

    let repo_res: serde_json::Value = post_json(
        &client,
        addr,
        &alice_jwt,
        "/repositories",
        &json!({ "name": "alice-repo", "visibility": "private" }),
    )
    .await;
    assert_eq!(repo_res["name"], "alice-repo");
    let still_works = client
        .get(format!(
            "http://{addr}/alice/alice-repo.git/info/refs?service=git-upload-pack"
        ))
        .basic_auth("alice", Some(&alice_plain_token))
        .send()
        .await
        .unwrap();
    assert_eq!(
        still_works.status(),
        200,
        "a token must remain valid after a cross-user revocation attempt was rejected"
    );
}
