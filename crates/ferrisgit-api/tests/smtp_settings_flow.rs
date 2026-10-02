// The mailer is a recording fake, except in the one test that uses the real, unconfigured SmtpEmailSender.

mod common;

use common::http::{post_anon, post_ok};

use common::RecordingEmail;
use ferrisgit_api::state::AppState;
use ferrisgit_application::mailer::Mailer;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;
use std::sync::Arc;

async fn spawn_server(pool: PgPool, fake: Option<Arc<RecordingEmail>>) -> (SocketAddr, AppState) {
    let app = common::spawn_app_with(pool, common::Options::default(), |state| {
        if let Some(fake) = fake {
            state.mailer = Arc::new(Mailer::new(fake));
        }
    })
    .await;
    (app.addr, app.state)
}

async fn login(
    client: &reqwest::Client,
    addr: SocketAddr,
    username: &str,
    password: &str,
) -> String {
    let body: Value = post_anon(
        client,
        addr,
        "/auth/login",
        &json!({ "username": username, "password": password }),
    )
    .await
    .json()
    .await
    .unwrap();
    body["token"].as_str().unwrap().to_string()
}

async fn admin_and_regular_tokens(client: &reqwest::Client, addr: SocketAddr) -> (String, String) {
    let admin = login(client, addr, "admin", "adminpassword123").await;
    post_ok(client, addr, &admin, "/admin/users", &json!({ "username": "regular", "email": "regular@example.com", "password": "password12345" })).await;
    let regular = login(client, addr, "regular", "password12345").await;
    (admin, regular)
}

fn valid_body() -> Value {
    json!({ "host": "smtp.example.com", "port": 587, "security": "starttls", "username": "mailer", "password": "s3cret-pass", "fromAddress": "noreply@example.com", "fromName": "Ma forge" })
}

fn url(addr: SocketAddr, suffix: &str) -> String {
    format!("http://{addr}/api/admin/settings/smtp{suffix}")
}

#[sqlx::test]
async fn anonymous_and_non_admin_callers_are_refused(pool: PgPool) {
    let (addr, _) = spawn_server(pool, Some(RecordingEmail::new())).await;
    let client = reqwest::Client::new();
    let (_, regular) = admin_and_regular_tokens(&client, addr).await;

    let anonymous = [
        client.get(url(addr, "")).send().await.unwrap(),
        client
            .put(url(addr, ""))
            .json(&valid_body())
            .send()
            .await
            .unwrap(),
        client
            .post(url(addr, "/test"))
            .json(&json!({ "to": "a@example.com" }))
            .send()
            .await
            .unwrap(),
    ];
    for res in anonymous {
        assert_eq!(res.status(), 401);
    }

    let non_admin = [
        client
            .get(url(addr, ""))
            .bearer_auth(&regular)
            .send()
            .await
            .unwrap(),
        client
            .put(url(addr, ""))
            .bearer_auth(&regular)
            .json(&valid_body())
            .send()
            .await
            .unwrap(),
        client
            .post(url(addr, "/test"))
            .bearer_auth(&regular)
            .json(&json!({ "to": "a@example.com" }))
            .send()
            .await
            .unwrap(),
    ];
    // A non-admin comes back as Unauthorized (401), there's no 403 variant.
    for res in non_admin {
        assert_eq!(res.status(), 401);
    }
}

#[sqlx::test]
async fn get_reports_defaults_when_unconfigured(pool: PgPool) {
    let (addr, _) = spawn_server(pool, Some(RecordingEmail::new())).await;
    let client = reqwest::Client::new();
    let (admin, _) = admin_and_regular_tokens(&client, addr).await;

    let res = client
        .get(url(addr, ""))
        .bearer_auth(&admin)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(
        body,
        json!({ "configured": false, "host": "", "port": 587, "security": "starttls", "username": "", "passwordSet": false, "fromAddress": "", "fromName": "FerrisGit" })
    );
}

#[sqlx::test]
async fn put_saves_then_get_returns_everything_but_the_password(pool: PgPool) {
    let (addr, state) = spawn_server(pool, Some(RecordingEmail::new())).await;
    let client = reqwest::Client::new();
    let (admin, _) = admin_and_regular_tokens(&client, addr).await;

    let put = client
        .put(url(addr, ""))
        .bearer_auth(&admin)
        .json(&valid_body())
        .send()
        .await
        .unwrap();
    assert_eq!(put.status(), 200);
    let put_text = put.text().await.unwrap();
    assert!(
        !put_text.contains("s3cret-pass") && !put_text.contains("password\":"),
        "PUT response leaked the password: {put_text}"
    );
    let put_body: Value = serde_json::from_str(&put_text).unwrap();
    assert_eq!(put_body["configured"], true);
    assert_eq!(put_body["passwordSet"], true);

    let get = client
        .get(url(addr, ""))
        .bearer_auth(&admin)
        .send()
        .await
        .unwrap();
    assert_eq!(get.status(), 200);
    let text = get.text().await.unwrap();
    assert!(
        !text.contains("s3cret-pass"),
        "GET response leaked the password: {text}"
    );
    assert!(
        !text.contains("password\":"),
        "GET response carries a `password` field: {text}"
    );
    let body: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        body,
        json!({ "configured": true, "host": "smtp.example.com", "port": 587, "security": "starttls", "username": "mailer", "passwordSet": true, "fromAddress": "noreply@example.com", "fromName": "Ma forge" })
    );

    let stored = state.smtp_settings.get().await.unwrap().unwrap();
    assert_eq!(stored.password.as_deref(), Some("s3cret-pass"));
}

#[sqlx::test]
async fn put_without_a_password_keeps_the_stored_one(pool: PgPool) {
    let (addr, state) = spawn_server(pool, Some(RecordingEmail::new())).await;
    let client = reqwest::Client::new();
    let (admin, _) = admin_and_regular_tokens(&client, addr).await;

    client
        .put(url(addr, ""))
        .bearer_auth(&admin)
        .json(&valid_body())
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let update = json!({ "host": "smtp2.example.com", "port": 465, "security": "tls", "username": "mailer", "fromAddress": "noreply@example.com", "fromName": "Ma forge" });
    let res = client
        .put(url(addr, ""))
        .bearer_auth(&admin)
        .json(&update)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["passwordSet"], true);
    assert_eq!(body["host"], "smtp2.example.com");

    let stored = state.smtp_settings.get().await.unwrap().unwrap();
    assert_eq!(stored.password.as_deref(), Some("s3cret-pass"));
    assert_eq!(stored.port, 465);
}

#[sqlx::test]
async fn put_validation_errors_are_400(pool: PgPool) {
    let (addr, state) = spawn_server(pool, Some(RecordingEmail::new())).await;
    let client = reqwest::Client::new();
    let (admin, _) = admin_and_regular_tokens(&client, addr).await;

    let mut cases: Vec<(&str, Value)> = Vec::new();
    for (label, field, value) in [
        ("port 0", "port", json!(0)),
        ("port 70000", "port", json!(70000)),
        ("bad security", "security", json!("ssl")),
        ("bad from address", "fromAddress", json!("nope")),
        ("blank host", "host", json!("   ")),
    ] {
        let mut body = valid_body();
        body[field] = value;
        cases.push((label, body));
    }
    let mut no_password = valid_body();
    no_password.as_object_mut().unwrap().remove("password");
    cases.push(("username without password", no_password));

    for (label, body) in cases {
        let res = client
            .put(url(addr, ""))
            .bearer_auth(&admin)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400, "{label} must be a validation error");
    }
    assert!(
        state.smtp_settings.get().await.unwrap().is_none(),
        "nothing may be saved by a rejected request"
    );
}

#[sqlx::test]
async fn test_endpoint_sends_the_test_message_to_the_recipient(pool: PgPool) {
    let fake = RecordingEmail::new();
    let (addr, _) = spawn_server(pool, Some(fake.clone())).await;
    let client = reqwest::Client::new();
    let (admin, _) = admin_and_regular_tokens(&client, addr).await;

    let res = client
        .post(url(addr, "/test"))
        .bearer_auth(&admin)
        .json(&json!({ "to": "someone@example.com" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body, json!({ "sent": true }));

    let sent = fake.delivered();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to, "someone@example.com");
    assert_eq!(sent[0].subject, "E-mail de test FerrisGit");
    assert!(sent[0].html.contains("cid:ferrisgit-logo"));
}

#[sqlx::test]
async fn test_endpoint_reports_a_delivery_failure_with_its_reason(pool: PgPool) {
    let fake = RecordingEmail::failing("connexion refusée");
    let (addr, _) = spawn_server(pool, Some(fake)).await;
    let client = reqwest::Client::new();
    let (admin, _) = admin_and_regular_tokens(&client, addr).await;

    let res = client
        .post(url(addr, "/test"))
        .bearer_auth(&admin)
        .json(&json!({ "to": "someone@example.com" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["sent"], false);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("connexion refusée"),
        "got {body}"
    );
}

#[sqlx::test]
async fn test_endpoint_rejects_an_invalid_recipient(pool: PgPool) {
    let fake = RecordingEmail::new();
    let (addr, _) = spawn_server(pool, Some(fake.clone())).await;
    let client = reqwest::Client::new();
    let (admin, _) = admin_and_regular_tokens(&client, addr).await;

    let res = client
        .post(url(addr, "/test"))
        .bearer_auth(&admin)
        .json(&json!({ "to": "not-an-address" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    assert!(fake.delivered().is_empty());
}

#[sqlx::test]
async fn test_endpoint_without_configuration_reports_it(pool: PgPool) {
    let (addr, _) = spawn_server(pool, None).await;
    let client = reqwest::Client::new();
    let (admin, _) = admin_and_regular_tokens(&client, addr).await;

    let res = client
        .post(url(addr, "/test"))
        .bearer_auth(&admin)
        .json(&json!({ "to": "someone@example.com" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["sent"], false);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("SMTP non configuré"),
        "got {body}"
    );
}
