mod common;

use common::http::get_json;

use common::http::login;
use sqlx::PgPool;

#[sqlx::test]
async fn me_reports_the_callers_own_id(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let jwt = login(&client, addr, "admin", "adminpassword123").await;

    let res: serde_json::Value = get_json(&client, addr, &jwt, "/auth/me").await;

    assert!(
        res["id"].is_string(),
        "expected a string id field, got: {res}"
    );
    assert!(!res["id"].as_str().unwrap().is_empty());
}
