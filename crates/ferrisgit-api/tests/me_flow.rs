mod common;

use common::http::{get, get_json, post_empty};

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

#[sqlx::test]
async fn me_reports_when_the_account_was_created(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let jwt = login(&client, addr, "admin", "adminpassword123").await;

    let res: serde_json::Value = get_json(&client, addr, &jwt, "/auth/me").await;

    let created_at = res["createdAt"].as_str().expect("a createdAt string");
    assert!(
        chrono::DateTime::parse_from_rfc3339(created_at).is_ok(),
        "expected an RFC 3339 date, got: {created_at}"
    );
}

#[sqlx::test]
async fn logout_all_ends_every_session_of_the_account_and_records_it(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let client = reqwest::Client::new();
    let this_browser = login(&client, app.addr, "admin", "adminpassword123").await;
    let other_device = login(&client, app.addr, "admin", "adminpassword123").await;
    let me: serde_json::Value = get_json(&client, app.addr, &this_browser, "/auth/me").await;

    let res = post_empty(&client, app.addr, &this_browser, "/auth/logout-all").await;
    assert_eq!(res.status(), 204);

    for (name, jwt) in [
        ("this browser", &this_browser),
        ("another device", &other_device),
    ] {
        assert_eq!(
            get(&client, app.addr, jwt, "/auth/me").await.status(),
            401,
            "the session of {name} must end"
        );
    }
    let events = sqlx::query_scalar::<_, String>(
        "SELECT event_type FROM domain_events WHERE aggregate_type = 'Security' AND aggregate_id = $1",
    )
    .bind(me["id"].as_str().unwrap())
    .fetch_all(&app.pool)
    .await
    .unwrap();
    assert!(events.contains(&"SessionsRevoked".to_string()));
}

#[sqlx::test]
async fn logout_all_needs_a_session(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let res = reqwest::Client::new()
        .post(format!("http://{addr}/api/auth/logout-all"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}
