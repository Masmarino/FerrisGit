mod common;

use common::USER_PASSWORD;
use common::http::{create_user, get, login, post};

use serde_json::json;
use sqlx::PgPool;

/// Regression: `/resolve` performed no authorization, so any logged-in user could enumerate repositories and groups by 200 vs 404.
/// A stranger must get 404 for all three `ResolvedPath` variants.
#[sqlx::test]
async fn resolve_endpoint_enforces_reader_access_for_every_resolved_path_variant(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;

    let client = reqwest::Client::new();

    let admin_jwt = login(&client, addr, "admin", "adminpassword123").await;

    for username in ["owner", "stranger"] {
        create_user(&client, addr, &admin_jwt, username).await;
    }

    let owner_jwt = login(&client, addr, "owner", USER_PASSWORD).await;
    let stranger_jwt = login(&client, addr, "stranger", USER_PASSWORD).await;

    let create_personal_status = post(
        &client,
        addr,
        &owner_jwt,
        "/repositories",
        &json!({ "name": "widgets", "visibility": "private" }),
    )
    .await
    .status();
    assert_eq!(create_personal_status, 200);

    let group_res = post(
        &client,
        addr,
        &owner_jwt,
        "/groups",
        &json!({ "name": "acme", "description": "" }),
    )
    .await;
    assert_eq!(group_res.status(), 200);

    let create_group_repo_status = post(
        &client,
        addr,
        &owner_jwt,
        "/repositories",
        &json!({ "name": "gizmos", "visibility": "private", "groupPath": "acme" }),
    )
    .await
    .status();
    assert_eq!(create_group_repo_status, 200);

    let owner_personal = get(&client, addr, &owner_jwt, "/resolve/owner/widgets").await;
    assert_eq!(
        owner_personal.status(),
        200,
        "owner must be able to resolve their own personal repository"
    );

    let owner_group = get(&client, addr, &owner_jwt, "/resolve/acme").await;
    assert_eq!(
        owner_group.status(),
        200,
        "owner must be able to resolve a group they're a maintainer of"
    );

    let owner_group_repo = get(&client, addr, &owner_jwt, "/resolve/acme/gizmos").await;
    assert_eq!(
        owner_group_repo.status(),
        200,
        "owner must be able to resolve a group-scoped repository they created"
    );

    let stranger_personal = get(&client, addr, &stranger_jwt, "/resolve/owner/widgets").await;
    assert_eq!(
        stranger_personal.status(),
        404,
        "a stranger must not be able to resolve someone else's personal repository"
    );
    let stranger_personal_body: serde_json::Value = stranger_personal.json().await.unwrap();

    let stranger_group = get(&client, addr, &stranger_jwt, "/resolve/acme").await;
    assert_eq!(
        stranger_group.status(),
        404,
        "a stranger must not be able to resolve a group they have no access to"
    );

    let stranger_group_repo = get(&client, addr, &stranger_jwt, "/resolve/acme/gizmos").await;
    assert_eq!(
        stranger_group_repo.status(),
        404,
        "a stranger must not be able to resolve a group-scoped repository they have no access to"
    );
    let stranger_group_repo_body: serde_json::Value = stranger_group_repo.json().await.unwrap();

    // The body must not distinguish "denied" from "absent" either (`{"error":"repository"}` vs `{"error":"path"}`): compare against nonexistent paths of the same shape.

    let nonexistent_personal =
        get(&client, addr, &stranger_jwt, "/resolve/owner/nonexistent").await;
    assert_eq!(
        nonexistent_personal.status(),
        404,
        "a nonexistent personal-repository path must also 404"
    );
    let nonexistent_personal_body: serde_json::Value = nonexistent_personal.json().await.unwrap();
    assert_eq!(
        stranger_personal_body, nonexistent_personal_body,
        "the body for a hidden personal repository must be identical to the body for a nonexistent path — otherwise existence leaks through the error message"
    );

    let nonexistent_group_repo =
        get(&client, addr, &stranger_jwt, "/resolve/acme/nonexistent").await;
    assert_eq!(
        nonexistent_group_repo.status(),
        404,
        "a nonexistent group-scoped-repository path must also 404"
    );
    let nonexistent_group_repo_body: serde_json::Value =
        nonexistent_group_repo.json().await.unwrap();
    assert_eq!(
        stranger_group_repo_body, nonexistent_group_repo_body,
        "the body for a hidden group repository must be identical to the body for a nonexistent path — otherwise existence leaks through the error message"
    );
}
