mod common;

use common::http::{get, patch, post, post_anon};

use common::{RecordingEmail, wait_for_attempts};
use ferrisgit_application::mailer::Mailer;
use serde_json::json;
use sqlx::PgPool;
use std::net::SocketAddr;
use std::sync::Arc;

async fn spawn_server(pool: PgPool) -> SocketAddr {
    spawn_server_with_mailer(pool, RecordingEmail::new()).await
}

async fn spawn_server_with_mailer(pool: PgPool, mailer: Arc<RecordingEmail>) -> SocketAddr {
    common::spawn_app_with(pool, common::Options::default(), |state| {
        state.mailer = Arc::new(Mailer::new(mailer));
    })
    .await
    .addr
}

async fn login(
    client: &reqwest::Client,
    addr: SocketAddr,
    username: &str,
    password: &str,
) -> reqwest::Response {
    post_anon(
        client,
        addr,
        "/auth/login",
        &json!({ "username": username, "password": password }),
    )
    .await
}

/// Gives the bootstrap admin a deliverable `email` (`admin@localhost` is not a deliverable mailbox).
async fn admin_jwt_with_email(client: &reqwest::Client, addr: SocketAddr, email: &str) -> String {
    let jwt = login(client, addr, "admin", "adminpassword123")
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let res = patch(client, addr, &jwt, "/auth/me", &json!({ "email": email })).await;
    assert_eq!(res.status(), 200);
    jwt
}

#[sqlx::test]
async fn rejects_an_incorrect_current_password(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let jwt = login(&client, addr, "admin", "adminpassword123")
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();

    let res = post(
        &client,
        addr,
        &jwt,
        "/auth/me/password",
        &json!({ "currentPassword": "totally-wrong", "newPassword": "new-password123" }),
    )
    .await;

    assert_eq!(
        res.status(),
        400,
        "a wrong current password must be rejected as a validation error, not silently accepted"
    );

    let still_works = login(&client, addr, "admin", "adminpassword123").await;
    assert_eq!(still_works.status(), 200);
}

#[sqlx::test]
async fn rejects_a_new_password_shorter_than_8_characters(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let jwt = login(&client, addr, "admin", "adminpassword123")
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();

    let res = post(
        &client,
        addr,
        &jwt,
        "/auth/me/password",
        &json!({ "currentPassword": "adminpassword123", "newPassword": "short" }),
    )
    .await;

    assert_eq!(res.status(), 400);
}

#[sqlx::test]
async fn a_successful_change_swaps_which_password_logs_in_and_revokes_the_old_jwt(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let old_jwt = login(&client, addr, "admin", "adminpassword123")
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();

    let before = get(&client, addr, &old_jwt, "/auth/me").await;
    assert_eq!(before.status(), 200);

    let change_res = post(
        &client,
        addr,
        &old_jwt,
        "/auth/me/password",
        &json!({ "currentPassword": "adminpassword123", "newPassword": "new-password123" }),
    )
    .await;
    assert!(
        change_res.status().is_success(),
        "expected the password change to succeed, got {}",
        change_res.status()
    );

    let old_login_attempt = login(&client, addr, "admin", "adminpassword123").await;
    assert_eq!(
        old_login_attempt.status(),
        401,
        "the old password must stop working once it has been changed"
    );

    let new_login: serde_json::Value = login(&client, addr, "admin", "new-password123")
        .await
        .json()
        .await
        .unwrap();
    let new_jwt = new_login["token"].as_str().unwrap();

    // A password change bumps the token epoch: the pre-change JWT stops authenticating immediately.
    let old_jwt_after_change = get(&client, addr, &old_jwt, "/auth/me").await;
    assert_eq!(
        old_jwt_after_change.status(),
        401,
        "a JWT issued before the password change must be revoked, not just stay valid until its own expiry"
    );

    let new_jwt_works = get(&client, addr, new_jwt, "/auth/me").await;
    assert_eq!(new_jwt_works.status(), 200);
}

#[sqlx::test]
async fn a_successful_change_returns_a_fresh_token_that_survives_the_epoch_bump(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let old_jwt = login(&client, addr, "admin", "adminpassword123")
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();

    let change_res = post(
        &client,
        addr,
        &old_jwt,
        "/auth/me/password",
        &json!({ "currentPassword": "adminpassword123", "newPassword": "new-password123" }),
    )
    .await;
    assert_eq!(change_res.status(), 200);
    let body: serde_json::Value = change_res.json().await.unwrap();
    let fresh_jwt = body["token"]
        .as_str()
        .expect("the response must carry a fresh token in the same shape as login")
        .to_string();
    assert_ne!(fresh_jwt, old_jwt);

    let fresh_works = get(&client, addr, &fresh_jwt, "/auth/me").await;
    assert_eq!(
        fresh_works.status(),
        200,
        "the token returned by the password change must keep the current session alive"
    );

    let old_after = get(&client, addr, &old_jwt, "/auth/me").await;
    assert_eq!(old_after.status(), 401);
}

#[sqlx::test]
async fn a_failed_change_returns_no_token(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let jwt = login(&client, addr, "admin", "adminpassword123")
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();

    let res = post(
        &client,
        addr,
        &jwt,
        "/auth/me/password",
        &json!({ "currentPassword": "totally-wrong", "newPassword": "new-password123" }),
    )
    .await;

    assert_eq!(res.status(), 400);
    let body = res.text().await.unwrap();
    assert!(
        !body.contains("\"token\""),
        "a rejected change must not hand out a token, got {body}"
    );

    // No epoch bump on failure.
    let still = get(&client, addr, &jwt, "/auth/me").await;
    assert_eq!(still.status(), 200);
}

#[sqlx::test]
async fn changing_the_password_sends_a_notification_to_the_users_email(pool: PgPool) {
    let mailer = RecordingEmail::new();
    let addr = spawn_server_with_mailer(pool, mailer.clone()).await;
    let client = reqwest::Client::new();
    let email = "x@example.com".to_string();
    let jwt = admin_jwt_with_email(&client, addr, &email).await;

    let res = post(
        &client,
        addr,
        &jwt,
        "/auth/me/password",
        &json!({ "currentPassword": "adminpassword123", "newPassword": "new-password123" }),
    )
    .await;
    assert_eq!(res.status(), 200);

    wait_for_attempts(&mailer, 1).await;
    assert_eq!(
        mailer.sent(),
        [(
            email,
            "Votre mot de passe FerrisGit a été modifié".to_string()
        )]
    );
}

#[sqlx::test]
async fn a_refused_password_change_sends_nothing(pool: PgPool) {
    let mailer = RecordingEmail::new();
    let addr = spawn_server_with_mailer(pool, mailer.clone()).await;
    let client = reqwest::Client::new();
    let jwt = admin_jwt_with_email(&client, addr, "x@example.com").await;

    let res = post(
        &client,
        addr,
        &jwt,
        "/auth/me/password",
        &json!({ "currentPassword": "totally-wrong", "newPassword": "new-password123" }),
    )
    .await;
    assert_eq!(res.status(), 400);

    // The mail is sent from a background task, so an immediate "nothing sent" proves nothing. A later successful change must be the only delivery attempt.
    let ok = post(
        &client,
        addr,
        &jwt,
        "/auth/me/password",
        &json!({ "currentPassword": "adminpassword123", "newPassword": "new-password123" }),
    )
    .await;
    assert_eq!(ok.status(), 200);
    wait_for_attempts(&mailer, 1).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(
        mailer.attempts(),
        1,
        "only the successful change may send a mail"
    );
}

#[sqlx::test]
async fn a_failing_mailer_does_not_fail_the_password_change(pool: PgPool) {
    let mailer = RecordingEmail::failing("smtp down");
    let addr = spawn_server_with_mailer(pool, mailer.clone()).await;
    let client = reqwest::Client::new();
    let jwt = admin_jwt_with_email(&client, addr, "x@example.com").await;

    let res = post(
        &client,
        addr,
        &jwt,
        "/auth/me/password",
        &json!({ "currentPassword": "adminpassword123", "newPassword": "new-password123" }),
    )
    .await;
    assert_eq!(res.status(), 200);
    wait_for_attempts(&mailer, 1).await;
    let fresh = res.json::<serde_json::Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let me = get(&client, addr, &fresh, "/auth/me").await;
    assert_eq!(
        me.status(),
        200,
        "the fresh token must stay usable when the notification mail fails"
    );
}

#[sqlx::test]
async fn an_address_no_relay_can_deliver_to_gets_no_notification(pool: PgPool) {
    let mailer = RecordingEmail::new();
    let addr = spawn_server_with_mailer(pool, mailer.clone()).await;
    let client = reqwest::Client::new();
    let jwt = login(&client, addr, "admin", "adminpassword123")
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let res = post(
        &client,
        addr,
        &jwt,
        "/auth/me/password",
        &json!({ "currentPassword": "adminpassword123", "newPassword": "new-password123" }),
    )
    .await;
    assert_eq!(res.status(), 200);
    let fresh = res.json::<serde_json::Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();

    // Shows the background task works, and that the first change made no attempt of its own.
    let set = patch(
        &client,
        addr,
        &fresh,
        "/auth/me",
        &json!({ "email": "x@example.com" }),
    )
    .await;
    assert_eq!(set.status(), 200);
    let again = post(
        &client,
        addr,
        &fresh,
        "/auth/me/password",
        &json!({ "currentPassword": "new-password123", "newPassword": "another-password123" }),
    )
    .await;
    assert_eq!(again.status(), 200);
    wait_for_attempts(&mailer, 1).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(
        mailer.attempts(),
        1,
        "only the deliverable address may receive the notification"
    );
    assert_eq!(
        mailer.sent(),
        [(
            "x@example.com".to_string(),
            "Votre mot de passe FerrisGit a été modifié".to_string()
        )]
    );
}
