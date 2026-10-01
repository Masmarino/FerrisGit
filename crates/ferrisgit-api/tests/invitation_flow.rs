// Admin invitations and activation. MFA stays enforced: activation issues no session, and the invited user then goes through the TOTP setup.

use async_trait::async_trait;
use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::mailer::Mailer;
use ferrisgit_application::mfa_crypto::generate_code_at;
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use ferrisgit_domain::email::EmailPort;
use ferrisgit_domain::error::DomainError;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const ADMIN_PASSWORD: &str = "adminpassword123";
const NEW_PASSWORD: &str = "a-brand-new-password";
const PUBLIC_URL: &str = "http://localhost:4200";

#[derive(Debug, Clone)]
struct Mail {
    to: String,
    subject: String,
    html: String,
}

/// `fail` makes deliveries fail like an unreachable SMTP server.
struct RecordingEmail {
    delivered: Mutex<Vec<Mail>>,
    attempted: Mutex<Vec<Mail>>,
    fail: AtomicBool,
}

impl RecordingEmail {
    fn delivered(&self) -> Vec<Mail> {
        self.delivered.lock().unwrap().clone()
    }

    fn attempted(&self) -> Vec<Mail> {
        self.attempted.lock().unwrap().clone()
    }
}

#[async_trait]
impl EmailPort for RecordingEmail {
    async fn send(
        &self,
        to: &str,
        subject: &str,
        _text_body: &str,
        html_body: &str,
    ) -> Result<(), DomainError> {
        let mail = Mail {
            to: to.to_string(),
            subject: subject.to_string(),
            html: html_body.to_string(),
        };
        self.attempted.lock().unwrap().push(mail.clone());
        if self.fail.load(Ordering::SeqCst) {
            return Err(DomainError::Infrastructure(
                "smtp connection refused".to_string(),
            ));
        }
        self.delivered.lock().unwrap().push(mail);
        Ok(())
    }
}

struct Server {
    addr: SocketAddr,
    pool: PgPool,
    mailer: Arc<RecordingEmail>,
    client: reqwest::Client,
    admin: String,
}

impl Server {
    fn url(&self, path: &str) -> String {
        format!("http://{}/api{path}", self.addr)
    }

    async fn post(&self, path: &str, body: Value) -> reqwest::Response {
        self.client
            .post(self.url(path))
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    async fn admin_post(&self, path: &str, body: Value) -> reqwest::Response {
        self.client
            .post(self.url(path))
            .bearer_auth(&self.admin)
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    async fn login(&self, username: &str, password: &str) -> reqwest::Response {
        self.post(
            "/auth/login",
            json!({ "username": username, "password": password }),
        )
        .await
    }

    async fn invite(&self, username: &str, email: &str, is_admin: bool) -> reqwest::Response {
        self.admin_post(
            "/admin/users/invite",
            json!({ "username": username, "email": email, "isAdmin": is_admin }),
        )
        .await
    }

    async fn invited(&self, username: &str, email: &str) -> Value {
        let res = self.invite(username, email, false).await;
        assert_eq!(res.status(), 200);
        res.json().await.unwrap()
    }

    async fn resend(&self, user_id: &str) -> reqwest::Response {
        self.admin_post(&format!("/admin/users/{user_id}/invitation"), json!({}))
            .await
    }

    async fn activate(&self, token: &str, password: &str) -> reqwest::Response {
        self.post(
            "/auth/activate",
            json!({ "token": token, "password": password }),
        )
        .await
    }

    async fn list(&self) -> Vec<Value> {
        let res = self
            .client
            .get(self.url("/admin/users"))
            .bearer_auth(&self.admin)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        res.json().await.unwrap()
    }

    async fn listed(&self, username: &str) -> Value {
        self.list()
            .await
            .into_iter()
            .find(|u| u["username"] == username)
            .unwrap_or_else(|| panic!("{username} is not listed"))
    }

    async fn enrolled(&self, username: &str, password: &str) -> String {
        let body: Value = self.login(username, password).await.json().await.unwrap();
        let mfa_token = body["mfaToken"].as_str().expect("an mfaToken").to_string();
        let secret = self
            .post(
                "/auth/mfa/setup/totp/enroll",
                json!({ "mfaToken": mfa_token }),
            )
            .await
            .json::<Value>()
            .await
            .unwrap()["secret"]
            .as_str()
            .unwrap()
            .to_string();
        let res = self
            .post(
                "/auth/mfa/setup/totp/confirm",
                json!({ "mfaToken": mfa_token, "code": totp_code(&secret) }),
            )
            .await;
        assert_eq!(res.status(), 200);
        res.json::<Value>().await.unwrap()["token"]
            .as_str()
            .unwrap()
            .to_string()
    }

    async fn user_count(&self) -> i64 {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM users")
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
}

fn totp_code(secret: &str) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    generate_code_at(secret, now)
}

/// The token is in the URL fragment (never sent to a server, so absent from access logs), not in the query string.
fn activation_link(html: &str) -> String {
    let prefix = format!("{PUBLIC_URL}/activate#token=");
    let start = html
        .find(&prefix)
        .unwrap_or_else(|| panic!("no activation link in the mail: {html}"));
    let token: String = html[start + prefix.len()..]
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect();
    assert_eq!(token.len(), 64, "the token is 64 hex characters: {token}");
    assert!(token.chars().all(|c| c.is_ascii_hexdigit()), "{token}");
    let link = format!("{prefix}{token}");
    assert!(
        !link.contains('?'),
        "the token must not be in a query string: {link}"
    );
    link
}

fn token_of(link: &str) -> String {
    link.split_once("token=").unwrap().1.to_string()
}

async fn spawn_server(pool: PgPool) -> Server {
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
        bootstrap_admin_password: Some(ADMIN_PASSWORD.to_string()),
        settings_encryption_key: [b'k'; 32],
        public_url: PUBLIC_URL.to_string(),
        trusted_proxy_cidrs: vec![],
    };

    let mailer = Arc::new(RecordingEmail {
        delivered: Mutex::new(Vec::new()),
        attempted: Mutex::new(Vec::new()),
        fail: AtomicBool::new(false),
    });
    let mut state = AppState::new(pool.clone(), config.clone()).await;
    assert!(
        state.mfa_enforced,
        "AppState::new must enforce MFA: production has no way to turn it off"
    );
    state.mailer = Arc::new(Mailer::new(mailer.clone()));
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
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    let mut server = Server {
        addr,
        pool,
        mailer,
        client: reqwest::Client::new(),
        admin: String::new(),
    };
    server.admin = server.enrolled("admin", ADMIN_PASSWORD).await;
    server
}

#[sqlx::test]
async fn every_admin_route_refuses_anonymous_callers_and_non_admins(pool: PgPool) {
    let server = spawn_server(pool).await;
    let invited = server.invited("alice", "alice@example.com").await;
    let token = token_of(&activation_link(&server.mailer.delivered()[0].html));
    assert_eq!(server.activate(&token, NEW_PASSWORD).await.status(), 204);
    let alice = server.enrolled("alice", NEW_PASSWORD).await;
    let alice_id = invited["user"]["id"].as_str().unwrap();
    let before = server.user_count().await;

    let routes: Vec<(&str, String, Value)> = vec![
        ("GET", "/admin/users".to_string(), Value::Null),
        (
            "POST",
            "/admin/users/invite".to_string(),
            json!({ "username": "bob", "email": "bob@example.com", "isAdmin": false }),
        ),
        (
            "POST",
            format!("/admin/users/{alice_id}/invitation"),
            json!({}),
        ),
        ("GET", "/admin/settings".to_string(), Value::Null),
    ];
    for (method, path, body) in routes {
        for bearer in [None, Some(alice.as_str())] {
            let mut req = if method == "GET" {
                server.client.get(server.url(&path))
            } else {
                server.client.post(server.url(&path)).json(&body)
            };
            if let Some(bearer) = bearer {
                req = req.bearer_auth(bearer);
            }
            let res = req.send().await.unwrap();
            assert_eq!(
                res.status(),
                401,
                "{method} {path} as {}",
                if bearer.is_some() {
                    "a non-admin"
                } else {
                    "anonymous"
                }
            );
        }
    }
    assert_eq!(
        server.user_count().await,
        before,
        "the refused invitation created nobody"
    );
}

#[sqlx::test]
async fn an_invitation_creates_an_invited_user_and_mails_the_activation_link(pool: PgPool) {
    let server = spawn_server(pool).await;

    let res = server.invite("Bob", "Bob@Example.com", false).await;

    assert_eq!(res.status(), 200);
    let text = res.text().await.unwrap();
    let body: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(body["emailSent"], json!(true));
    assert!(
        body.get("activationUrl").is_none(),
        "the link is only handed to the admin when the mail could not be sent: {body}"
    );
    assert!(body.get("emailError").is_none(), "{body}");
    assert_eq!(body["user"]["username"], "bob");
    assert_eq!(body["user"]["email"], "Bob@Example.com");
    assert_eq!(body["user"]["isAdmin"], json!(false));
    assert_eq!(body["user"]["state"], "invited");
    assert_eq!(body["user"]["mfaEnabled"], json!(false));

    let mails = server.mailer.delivered();
    assert_eq!(mails.len(), 1);
    assert_eq!(mails[0].to, "Bob@Example.com");
    assert_eq!(mails[0].subject, "Votre compte FerrisGit");
    assert!(
        mails[0]
            .html
            .contains("Votre nom d'utilisateur : <strong>bob</strong>"),
        "the mail names the username to sign in with: {}",
        mails[0].html
    );
    let link = activation_link(&mails[0].html);
    assert!(
        !text.contains(&token_of(&link)),
        "the token must not be in the API response when the mail went out: {text}"
    );

    let listed = server.listed("bob").await;
    assert_eq!(listed["state"], "invited");
    assert_eq!(listed["mfaEnabled"], json!(false));
    assert_eq!(listed["id"], body["user"]["id"]);
    let expires: chrono::DateTime<chrono::Utc> = listed["invitationExpiresAt"]
        .as_str()
        .expect("an expiry")
        .parse()
        .unwrap();
    let remaining = expires - chrono::Utc::now();
    assert!(
        remaining > chrono::Duration::minutes(23 * 60 + 58)
            && remaining <= chrono::Duration::hours(24),
        "the link lives 24 h, got {remaining}"
    );
    assert_eq!(
        body["user"]["invitationExpiresAt"],
        listed["invitationExpiresAt"]
    );

    let stored: Vec<String> = sqlx::query_scalar("SELECT token_hash FROM user_invitations")
        .fetch_all(&server.pool)
        .await
        .unwrap();
    assert_eq!(stored.len(), 1);
    assert_ne!(stored[0], token_of(&link));
}

#[sqlx::test]
async fn an_invited_user_can_sign_in_with_the_casing_typed_by_the_admin(pool: PgPool) {
    let server = spawn_server(pool).await;
    server.invited("Bob_2", "bob2@example.com").await;
    let token = token_of(&activation_link(&server.mailer.delivered()[0].html));
    assert_eq!(server.activate(&token, NEW_PASSWORD).await.status(), 204);

    for typed in ["bob_2", "Bob_2", "BOB_2"] {
        let res = server.login(typed, NEW_PASSWORD).await;
        assert_eq!(res.status(), 200, "signing in as {typed:?}");
        assert!(res.json::<Value>().await.unwrap()["mfaToken"].is_string());
    }
    assert_eq!(server.login("Bob_2", "wrong-password").await.status(), 401);
}

#[sqlx::test]
async fn an_invited_admin_keeps_the_admin_flag(pool: PgPool) {
    let server = spawn_server(pool).await;

    let res = server.invite("carol", "carol@example.com", true).await;

    assert_eq!(res.status(), 200);
    assert_eq!(
        res.json::<Value>().await.unwrap()["user"]["isAdmin"],
        json!(true)
    );
    assert_eq!(server.listed("carol").await["isAdmin"], json!(true));
}

#[sqlx::test]
async fn a_failing_smtp_still_creates_the_invitation_and_hands_the_link_to_the_admin(pool: PgPool) {
    let server = spawn_server(pool).await;
    server.mailer.fail.store(true, Ordering::SeqCst);

    let res = server.invite("bob", "bob@example.com", false).await;

    assert_eq!(
        res.status(),
        200,
        "a mail failure never fails the invitation"
    );
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["emailSent"], json!(false));
    assert!(
        body["emailError"]
            .as_str()
            .is_some_and(|e| e.contains("smtp connection refused")),
        "{body}"
    );
    let url = body["activationUrl"]
        .as_str()
        .expect("the link is handed over when the mail did not go out");
    assert!(server.mailer.delivered().is_empty());
    let attempts = server.mailer.attempted();
    assert_eq!(attempts.len(), 1);
    assert_eq!(
        url,
        activation_link(&attempts[0].html),
        "the same link a successful send would have contained"
    );
    assert_eq!(body["user"]["state"], "invited");

    assert_eq!(
        server.activate(&token_of(url), NEW_PASSWORD).await.status(),
        204
    );
    assert_eq!(server.listed("bob").await["state"], "active");
}

#[sqlx::test]
async fn an_invalid_or_taken_invitation_is_refused_and_sends_nothing(pool: PgPool) {
    let server = spawn_server(pool).await;
    server.invited("bob", "bob@example.com").await;
    let before = server.user_count().await;
    let sent_before = server.mailer.attempted().len();

    for (username, email, expected, why) in [
        ("bob", "other@example.com", 409, "username taken"),
        (
            "BOB",
            "other@example.com",
            409,
            "username taken, other casing",
        ),
        (
            "newbie",
            "BOB@example.com",
            409,
            "e-mail taken, other casing",
        ),
        (
            "admin",
            "other@example.com",
            400,
            "`admin` is a reserved name anyway",
        ),
        ("ab", "x@example.com", 400, "username too short"),
        ("login", "x@example.com", 400, "reserved name"),
        ("newbie", "not-an-email", 400, "invalid e-mail"),
    ] {
        assert_eq!(
            server.invite(username, email, false).await.status(),
            expected,
            "{why}"
        );
    }
    assert_eq!(server.user_count().await, before);
    assert_eq!(
        server.mailer.attempted().len(),
        sent_before,
        "no mail for a refused invitation"
    );
}

#[sqlx::test]
async fn an_invited_user_cannot_log_in_before_activating(pool: PgPool) {
    let server = spawn_server(pool).await;
    server.invited("bob", "bob@example.com").await;

    for password in [
        "password12345",
        "bob",
        "bob@example.com",
        "",
        "a-brand-new-password",
        "adminpassword123",
    ] {
        let res = server.login("bob", password).await;
        assert_eq!(res.status(), 401, "password {password:?}");
        let body: Value = res.json().await.unwrap();
        assert!(
            body.get("mfaToken").is_none() && body.get("token").is_none(),
            "{body}"
        );
    }
}

#[sqlx::test]
async fn activation_sets_the_password_issues_no_session_and_forces_the_mfa_setup(pool: PgPool) {
    let server = spawn_server(pool).await;
    server.invited("bob", "bob@example.com").await;
    let token = token_of(&activation_link(&server.mailer.delivered()[0].html));

    let res = server.activate(&token, NEW_PASSWORD).await;

    assert_eq!(res.status(), 204);
    assert!(
        res.bytes().await.unwrap().is_empty(),
        "activation answers with no body and no session"
    );
    let listed = server.listed("bob").await;
    assert_eq!(listed["state"], "active");
    assert!(listed["invitationExpiresAt"].is_null());
    let invitations: i64 = sqlx::query_scalar("SELECT count(*) FROM user_invitations")
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(invitations, 0, "the invitation is consumed");

    let login: Value = server
        .login("bob", NEW_PASSWORD)
        .await
        .json()
        .await
        .unwrap();
    assert!(login["token"].is_null(), "{login}");
    assert!(login["mfaToken"].as_str().is_some());
    assert_eq!(
        login["mfaSetupRequired"],
        json!(true),
        "the first login is the forced TOTP setup"
    );

    let again = server.activate(&token, "another-password-1").await;
    assert_eq!(again.status(), 400);
    assert_eq!(
        server
            .activate(&"0".repeat(64), NEW_PASSWORD)
            .await
            .status(),
        400
    );
    assert_eq!(server.activate("", NEW_PASSWORD).await.status(), 400);
    assert_eq!(
        server.login("bob", "another-password-1").await.status(),
        401,
        "the second activation changed nothing"
    );
    assert_eq!(server.login("bob", NEW_PASSWORD).await.status(), 200);
}

#[sqlx::test]
async fn a_weak_password_is_refused_before_the_token_is_consumed(pool: PgPool) {
    let server = spawn_server(pool).await;
    server.invited("bob", "bob@example.com").await;
    let token = token_of(&activation_link(&server.mailer.delivered()[0].html));

    assert_eq!(server.activate(&token, "short").await.status(), 400);
    assert_eq!(
        server.listed("bob").await["state"],
        "invited",
        "still not activated"
    );

    assert_eq!(
        server.activate(&token, NEW_PASSWORD).await.status(),
        204,
        "the token still works after the rejected attempt"
    );
}

#[sqlx::test]
async fn an_expired_token_is_refused_and_can_be_replaced_by_a_resend(pool: PgPool) {
    let server = spawn_server(pool).await;
    let invited = server.invited("bob", "bob@example.com").await;
    let token = token_of(&activation_link(&server.mailer.delivered()[0].html));
    sqlx::query("UPDATE user_invitations SET expires_at = now() - interval '1 minute'")
        .execute(&server.pool)
        .await
        .unwrap();

    assert_eq!(server.activate(&token, NEW_PASSWORD).await.status(), 400);
    assert_eq!(server.login("bob", NEW_PASSWORD).await.status(), 401);
    let listed = server.listed("bob").await;
    assert_eq!(
        listed["state"], "invited",
        "an expired invitation stays listed as invited"
    );
    let expires: chrono::DateTime<chrono::Utc> = listed["invitationExpiresAt"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(expires < chrono::Utc::now(), "and shows its (past) expiry");

    let res = server.resend(invited["user"]["id"].as_str().unwrap()).await;
    assert_eq!(res.status(), 200);
    let link = activation_link(&server.mailer.delivered().last().unwrap().html);
    assert_eq!(
        server
            .activate(&token_of(&link), NEW_PASSWORD)
            .await
            .status(),
        204
    );
}

#[sqlx::test]
async fn a_resend_issues_a_new_link_and_kills_the_old_one(pool: PgPool) {
    let server = spawn_server(pool).await;
    let invited = server.invited("bob", "bob@example.com").await;
    let id = invited["user"]["id"].as_str().unwrap();
    let old_token = token_of(&activation_link(&server.mailer.delivered()[0].html));

    let res = server.resend(id).await;

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["emailSent"], json!(true));
    assert!(body.get("activationUrl").is_none(), "{body}");
    assert_eq!(body["user"]["id"], json!(id));
    assert_eq!(body["user"]["state"], "invited");
    let mails = server.mailer.delivered();
    assert_eq!(mails.len(), 2);
    assert_eq!(mails[1].to, "bob@example.com");
    assert_eq!(mails[1].subject, "Votre compte FerrisGit");
    let new_token = token_of(&activation_link(&mails[1].html));
    assert_ne!(new_token, old_token);
    let invitations: i64 = sqlx::query_scalar("SELECT count(*) FROM user_invitations")
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(invitations, 1, "one live invitation per user");

    assert_eq!(
        server.activate(&old_token, NEW_PASSWORD).await.status(),
        400,
        "the old link is dead"
    );
    assert_eq!(
        server.activate(&new_token, NEW_PASSWORD).await.status(),
        204,
        "the new one activates"
    );
}

#[sqlx::test]
async fn a_resend_with_a_failing_smtp_hands_the_new_link_to_the_admin(pool: PgPool) {
    let server = spawn_server(pool).await;
    let invited = server.invited("bob", "bob@example.com").await;
    server.mailer.fail.store(true, Ordering::SeqCst);

    let res = server.resend(invited["user"]["id"].as_str().unwrap()).await;

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["emailSent"], json!(false));
    assert!(body["emailError"].is_string());
    let url = body["activationUrl"].as_str().unwrap();
    assert_eq!(
        url,
        activation_link(&server.mailer.attempted().last().unwrap().html)
    );
    assert_eq!(
        server.activate(&token_of(url), NEW_PASSWORD).await.status(),
        204
    );
}

#[sqlx::test]
async fn a_resend_for_an_active_or_unknown_user_is_refused(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin_id: String =
        sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM users WHERE username = 'admin'")
            .fetch_one(&server.pool)
            .await
            .unwrap()
            .to_string();
    let sent_before = server.mailer.attempted().len();

    assert_eq!(
        server.resend(&admin_id).await.status(),
        400,
        "an active user has nothing to resend"
    );
    assert_eq!(
        server
            .resend(&uuid::Uuid::new_v4().to_string())
            .await
            .status(),
        404
    );
    assert_eq!(server.mailer.attempted().len(), sent_before);

    let invited = server.invited("bob", "bob@example.com").await;
    let token = token_of(&activation_link(&server.mailer.delivered()[0].html));
    assert_eq!(server.activate(&token, NEW_PASSWORD).await.status(), 204);
    assert_eq!(
        server
            .resend(invited["user"]["id"].as_str().unwrap())
            .await
            .status(),
        400
    );
}

#[sqlx::test]
async fn the_user_list_reports_the_state_and_mfa_of_everyone_and_leaks_no_secret(pool: PgPool) {
    let server = spawn_server(pool).await;
    server.invited("alice", "alice@example.com").await;
    let token = token_of(&activation_link(&server.mailer.delivered()[0].html));
    assert_eq!(server.activate(&token, NEW_PASSWORD).await.status(), 204);
    server.enrolled("alice", NEW_PASSWORD).await;
    server.invited("bob", "bob@example.com").await;

    let res = server
        .client
        .get(server.url("/admin/users"))
        .bearer_auth(&server.admin)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let text = res.text().await.unwrap();
    let users: Vec<Value> = serde_json::from_str(&text).unwrap();

    let names: Vec<&str> = users
        .iter()
        .map(|u| u["username"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["admin", "alice", "bob"], "in creation order");
    let by = |name: &str| users.iter().find(|u| u["username"] == name).unwrap();
    assert_eq!(
        (
            by("admin")["state"].as_str(),
            by("admin")["mfaEnabled"].as_bool(),
            by("admin")["isAdmin"].as_bool()
        ),
        (Some("active"), Some(true), Some(true))
    );
    assert_eq!(
        (
            by("alice")["state"].as_str(),
            by("alice")["mfaEnabled"].as_bool(),
            by("alice")["isAdmin"].as_bool()
        ),
        (Some("active"), Some(true), Some(false))
    );
    assert_eq!(
        (
            by("bob")["state"].as_str(),
            by("bob")["mfaEnabled"].as_bool()
        ),
        (Some("invited"), Some(false))
    );
    assert!(
        by("admin")["invitationExpiresAt"].is_null()
            && by("alice")["invitationExpiresAt"].is_null()
    );
    assert!(by("bob")["invitationExpiresAt"].is_string());

    // Exactly these fields, nothing else (no hash, no token, no secret).
    let expected: std::collections::BTreeSet<&str> = [
        "id",
        "username",
        "email",
        "isAdmin",
        "createdAt",
        "state",
        "invitationExpiresAt",
        "mfaEnabled",
    ]
    .into_iter()
    .collect();
    for user in &users {
        let keys: std::collections::BTreeSet<&str> = user
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, expected);
    }
    for forbidden in ["password", "hash", "$argon2", "secret", "token"] {
        assert!(
            !text.to_lowercase().contains(forbidden),
            "the list must not contain {forbidden:?}: {text}"
        );
    }
}

#[sqlx::test]
async fn the_admin_can_still_reset_the_mfa_of_a_listed_user(pool: PgPool) {
    let server = spawn_server(pool).await;
    let invited = server.invited("alice", "alice@example.com").await;
    let token = token_of(&activation_link(&server.mailer.delivered()[0].html));
    assert_eq!(server.activate(&token, NEW_PASSWORD).await.status(), 204);
    server.enrolled("alice", NEW_PASSWORD).await;
    assert_eq!(server.listed("alice").await["mfaEnabled"], json!(true));

    let id = invited["user"]["id"].as_str().unwrap();
    let res = server
        .client
        .delete(server.url(&format!("/admin/users/{id}/mfa")))
        .bearer_auth(&server.admin)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 204);
    let alice = server.listed("alice").await;
    assert_eq!(alice["mfaEnabled"], json!(false));
    assert_eq!(alice["state"], "active");
    let login: Value = server
        .login("alice", NEW_PASSWORD)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        login["mfaSetupRequired"],
        json!(true),
        "the next login is a first enrolment again"
    );
}

#[sqlx::test]
async fn activation_is_throttled_per_ip_on_the_eleventh_attempt(pool: PgPool) {
    let server = spawn_server(pool).await;
    server.invited("bob", "bob@example.com").await;
    let token = token_of(&activation_link(&server.mailer.delivered()[0].html));

    for attempt in 1..=10 {
        assert_eq!(
            server
                .activate(&"f".repeat(64), NEW_PASSWORD)
                .await
                .status(),
            400,
            "attempt {attempt}"
        );
    }
    assert_eq!(
        server
            .activate(&"f".repeat(64), NEW_PASSWORD)
            .await
            .status(),
        429,
        "the 11th attempt is throttled"
    );
    assert_eq!(
        server.activate(&token, NEW_PASSWORD).await.status(),
        429,
        "even the right token is throttled"
    );
    assert_eq!(
        server.listed("bob").await["state"],
        "invited",
        "the throttled attempts activated nothing"
    );
}
