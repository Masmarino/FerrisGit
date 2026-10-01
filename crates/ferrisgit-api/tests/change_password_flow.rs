use async_trait::async_trait;
use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::mailer::Mailer;
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use ferrisgit_domain::email::EmailPort;
use ferrisgit_domain::error::DomainError;
use serde_json::json;
use sqlx::PgPool;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

struct RecordingEmail {
    sent: Mutex<Vec<(String, String)>>,
    attempts: std::sync::atomic::AtomicUsize,
    fail_with: Option<String>,
}

impl RecordingEmail {
    fn new(fail_with: Option<&str>) -> Arc<Self> {
        Arc::new(Self {
            sent: Mutex::new(Vec::new()),
            attempts: std::sync::atomic::AtomicUsize::new(0),
            fail_with: fail_with.map(str::to_string),
        })
    }
}

#[async_trait]
impl EmailPort for RecordingEmail {
    async fn send(
        &self,
        to: &str,
        subject: &str,
        _text_body: &str,
        _html_body: &str,
    ) -> Result<(), DomainError> {
        self.attempts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if let Some(message) = &self.fail_with {
            return Err(DomainError::Infrastructure(message.clone()));
        }
        self.sent
            .lock()
            .unwrap()
            .push((to.to_string(), subject.to_string()));
        Ok(())
    }
}

async fn wait_for_attempts(mailer: &RecordingEmail, count: usize) {
    for _ in 0..100 {
        if mailer.attempts.load(std::sync::atomic::Ordering::SeqCst) >= count {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!(
        "expected {count} delivery attempt(s), saw {}",
        mailer.attempts.load(std::sync::atomic::Ordering::SeqCst)
    );
}

async fn spawn_server(pool: PgPool) -> SocketAddr {
    spawn_server_with_mailer(pool, RecordingEmail::new(None)).await
}

async fn spawn_server_with_mailer(pool: PgPool, mailer: Arc<RecordingEmail>) -> SocketAddr {
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

    let storage_dir = tempfile::tempdir().unwrap().keep();
    let static_dir = tempfile::tempdir().unwrap().keep();
    std::fs::write(static_dir.join("index.html"), "<html></html>").unwrap();

    let config = Config {
        database_url: String::new(),
        jwt_secret: "test-secret-that-is-at-least-32-characters-long".to_string(),
        storage_root: storage_dir.to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:0".to_string(),
        static_dir: static_dir.to_string_lossy().to_string(),
        bootstrap_admin_username: Some("admin".to_string()),
        bootstrap_admin_password: Some("adminpassword123".to_string()),
        settings_encryption_key: [b'k'; 32],
        public_url: "http://localhost:4200".to_string(),
        trusted_proxy_cidrs: vec![],
    };

    let mut state = AppState::new(pool, config.clone()).await;
    state.mfa_enforced = false; // these tests are not about MFA: they log in with a plain session
    state.mailer = Arc::new(Mailer::new(mailer));
    BootstrapAdminUseCase::new(state.users.clone(), state.hasher.clone())
        .execute(
            config.bootstrap_admin_username.clone(),
            config.bootstrap_admin_password.clone(),
        )
        .await
        .unwrap();

    let app = build_router(state, &static_dir);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

async fn login(
    client: &reqwest::Client,
    addr: SocketAddr,
    username: &str,
    password: &str,
) -> reqwest::Response {
    client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .unwrap()
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
    let res = client
        .patch(format!("http://{addr}/api/auth/me"))
        .bearer_auth(&jwt)
        .json(&json!({ "email": email }))
        .send()
        .await
        .unwrap();
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

    let res = client
        .post(format!("http://{addr}/api/auth/me/password"))
        .bearer_auth(&jwt)
        .json(&json!({ "currentPassword": "totally-wrong", "newPassword": "new-password123" }))
        .send()
        .await
        .unwrap();

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

    let res = client
        .post(format!("http://{addr}/api/auth/me/password"))
        .bearer_auth(&jwt)
        .json(&json!({ "currentPassword": "adminpassword123", "newPassword": "short" }))
        .send()
        .await
        .unwrap();

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

    let before = client
        .get(format!("http://{addr}/api/auth/me"))
        .bearer_auth(&old_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(before.status(), 200);

    let change_res = client
        .post(format!("http://{addr}/api/auth/me/password"))
        .bearer_auth(&old_jwt)
        .json(&json!({ "currentPassword": "adminpassword123", "newPassword": "new-password123" }))
        .send()
        .await
        .unwrap();
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
    let old_jwt_after_change = client
        .get(format!("http://{addr}/api/auth/me"))
        .bearer_auth(&old_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        old_jwt_after_change.status(),
        401,
        "a JWT issued before the password change must be revoked, not just stay valid until its own expiry"
    );

    let new_jwt_works = client
        .get(format!("http://{addr}/api/auth/me"))
        .bearer_auth(new_jwt)
        .send()
        .await
        .unwrap();
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

    let change_res = client
        .post(format!("http://{addr}/api/auth/me/password"))
        .bearer_auth(&old_jwt)
        .json(&json!({ "currentPassword": "adminpassword123", "newPassword": "new-password123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(change_res.status(), 200);
    let body: serde_json::Value = change_res.json().await.unwrap();
    let fresh_jwt = body["token"]
        .as_str()
        .expect("the response must carry a fresh token in the same shape as login")
        .to_string();
    assert_ne!(fresh_jwt, old_jwt);

    let fresh_works = client
        .get(format!("http://{addr}/api/auth/me"))
        .bearer_auth(&fresh_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        fresh_works.status(),
        200,
        "the token returned by the password change must keep the current session alive"
    );

    let old_after = client
        .get(format!("http://{addr}/api/auth/me"))
        .bearer_auth(&old_jwt)
        .send()
        .await
        .unwrap();
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

    let res = client
        .post(format!("http://{addr}/api/auth/me/password"))
        .bearer_auth(&jwt)
        .json(&json!({ "currentPassword": "totally-wrong", "newPassword": "new-password123" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
    let body = res.text().await.unwrap();
    assert!(
        !body.contains("\"token\""),
        "a rejected change must not hand out a token, got {body}"
    );

    // No epoch bump on failure.
    let still = client
        .get(format!("http://{addr}/api/auth/me"))
        .bearer_auth(&jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(still.status(), 200);
}

#[sqlx::test]
async fn changing_the_password_sends_a_notification_to_the_users_email(pool: PgPool) {
    let mailer = RecordingEmail::new(None);
    let addr = spawn_server_with_mailer(pool, mailer.clone()).await;
    let client = reqwest::Client::new();
    let email = "x@example.com".to_string();
    let jwt = admin_jwt_with_email(&client, addr, &email).await;

    let res = client
        .post(format!("http://{addr}/api/auth/me/password"))
        .bearer_auth(&jwt)
        .json(&json!({ "currentPassword": "adminpassword123", "newPassword": "new-password123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    wait_for_attempts(&mailer, 1).await;
    let sent = mailer.sent.lock().unwrap();
    assert_eq!(
        sent.as_slice(),
        &[(
            email,
            "Votre mot de passe FerrisGit a été modifié".to_string()
        )]
    );
}

#[sqlx::test]
async fn a_refused_password_change_sends_nothing(pool: PgPool) {
    let mailer = RecordingEmail::new(None);
    let addr = spawn_server_with_mailer(pool, mailer.clone()).await;
    let client = reqwest::Client::new();
    let jwt = admin_jwt_with_email(&client, addr, "x@example.com").await;

    let res = client
        .post(format!("http://{addr}/api/auth/me/password"))
        .bearer_auth(&jwt)
        .json(&json!({ "currentPassword": "totally-wrong", "newPassword": "new-password123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);

    // The mail is sent from a background task, so an immediate "nothing sent" proves nothing. A later successful change must be the only delivery attempt.
    let ok = client
        .post(format!("http://{addr}/api/auth/me/password"))
        .bearer_auth(&jwt)
        .json(&json!({ "currentPassword": "adminpassword123", "newPassword": "new-password123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(ok.status(), 200);
    wait_for_attempts(&mailer, 1).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(
        mailer.attempts.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "only the successful change may send a mail"
    );
}

#[sqlx::test]
async fn a_failing_mailer_does_not_fail_the_password_change(pool: PgPool) {
    let mailer = RecordingEmail::new(Some("smtp down"));
    let addr = spawn_server_with_mailer(pool, mailer.clone()).await;
    let client = reqwest::Client::new();
    let jwt = admin_jwt_with_email(&client, addr, "x@example.com").await;

    let res = client
        .post(format!("http://{addr}/api/auth/me/password"))
        .bearer_auth(&jwt)
        .json(&json!({ "currentPassword": "adminpassword123", "newPassword": "new-password123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    wait_for_attempts(&mailer, 1).await;
    let fresh = res.json::<serde_json::Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let me = client
        .get(format!("http://{addr}/api/auth/me"))
        .bearer_auth(&fresh)
        .send()
        .await
        .unwrap();
    assert_eq!(
        me.status(),
        200,
        "the fresh token must stay usable when the notification mail fails"
    );
}

#[sqlx::test]
async fn an_address_no_relay_can_deliver_to_gets_no_notification(pool: PgPool) {
    let mailer = RecordingEmail::new(None);
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
    let res = client
        .post(format!("http://{addr}/api/auth/me/password"))
        .bearer_auth(&jwt)
        .json(&json!({ "currentPassword": "adminpassword123", "newPassword": "new-password123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let fresh = res.json::<serde_json::Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();

    // Shows the background task works, and that the first change made no attempt of its own.
    let set = client
        .patch(format!("http://{addr}/api/auth/me"))
        .bearer_auth(&fresh)
        .json(&json!({ "email": "x@example.com" }))
        .send()
        .await
        .unwrap();
    assert_eq!(set.status(), 200);
    let again = client
        .post(format!("http://{addr}/api/auth/me/password"))
        .bearer_auth(&fresh)
        .json(
            &json!({ "currentPassword": "new-password123", "newPassword": "another-password123" }),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), 200);
    wait_for_attempts(&mailer, 1).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(
        mailer.attempts.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "only the deliverable address may receive the notification"
    );
    assert_eq!(
        mailer.sent.lock().unwrap().as_slice(),
        &[(
            "x@example.com".to_string(),
            "Votre mot de passe FerrisGit a été modifié".to_string()
        )]
    );
}
