mod common;

use common::http::{login, post, post_json, put};

use serde_json::json;
use sqlx::PgPool;

/// `jwtTtlHours` rejects values that expire tokens at once or make them practically permanent, since every session's lifetime comes from it.
#[sqlx::test]
async fn admin_settings_rejects_a_jwt_ttl_hours_outside_the_valid_range(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;

    let client = reqwest::Client::new();

    let jwt = login(&client, addr, "admin", "adminpassword123").await;

    let zero_res = put(
        &client,
        addr,
        &jwt,
        "/admin/settings",
        &json!({ "jwtTtlHours": 0 }),
    )
    .await;
    assert_eq!(
        zero_res.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "0 hours must be rejected: it would expire every token immediately, including the admin's own"
    );

    let negative_res = put(
        &client,
        addr,
        &jwt,
        "/admin/settings",
        &json!({ "jwtTtlHours": -5 }),
    )
    .await;
    assert_eq!(negative_res.status(), reqwest::StatusCode::BAD_REQUEST);

    let too_large_res = put(
        &client,
        addr,
        &jwt,
        "/admin/settings",
        &json!({ "jwtTtlHours": 721 }),
    )
    .await;
    assert_eq!(too_large_res.status(), reqwest::StatusCode::BAD_REQUEST);

    let valid_res: serde_json::Value = put(
        &client,
        addr,
        &jwt,
        "/admin/settings",
        &json!({ "jwtTtlHours": 48 }),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(valid_res["jwtTtlHours"], 48);
}

/// A CI variable is exported into a job's shell: the server refuses a name no shell can export, whatever the form did.
#[sqlx::test]
async fn a_ci_variable_whose_name_no_shell_can_export_is_refused(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let jwt = login(&client, addr, "admin", "adminpassword123").await;
    let repo: serde_json::Value = post_json(
        &client,
        addr,
        &jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    let path = format!(
        "/repositories/{}/ci-variables",
        repo["id"].as_str().unwrap()
    );

    for key in ["", "1PASSWORD", "MY-SECRET", "A=B"] {
        let res = post(
            &client,
            addr,
            &jwt,
            &path,
            &json!({ "key": key, "value": "v" }),
        )
        .await;
        assert_eq!(res.status(), reqwest::StatusCode::BAD_REQUEST, "{key:?}");
    }
    let res = post(
        &client,
        addr,
        &jwt,
        &path,
        &json!({ "key": "DEPLOY_TOKEN", "value": "v" }),
    )
    .await;
    assert_eq!(res.status(), reqwest::StatusCode::OK);
}
