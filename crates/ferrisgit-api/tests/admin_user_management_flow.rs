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
const USER_PASSWORD: &str = "password12345";
const NEW_PASSWORD: &str = "a-brand-new-password";
const PUBLIC_URL: &str = "http://localhost:4200";
const RESET_SUBJECT: &str = "Réinitialisation de votre mot de passe FerrisGit";
const PASSWORD_CHANGED_SUBJECT: &str = "Votre mot de passe FerrisGit a été modifié";

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

    fn attempted_with_subject(&self, subject: &str) -> Vec<Mail> {
        self.attempted()
            .into_iter()
            .filter(|m| m.subject == subject)
            .collect()
    }

    async fn wait_for_subject(&self, subject: &str, count: usize) -> Vec<Mail> {
        for _ in 0..100 {
            let mails = self.attempted_with_subject(subject);
            if mails.len() >= count {
                return mails;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!(
            "expected {count} mail(s) with subject {subject:?}, saw {:?}",
            self.attempted_with_subject(subject)
        );
    }

    /// Gives a background task the time it would need to (wrongly) send something, then reads the mails.
    async fn settled_with_subject(&self, subject: &str) -> Vec<Mail> {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        self.attempted_with_subject(subject)
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
    state: AppState,
    pool: PgPool,
    mailer: Arc<RecordingEmail>,
    client: reqwest::Client,
    admin: String,
    admin_id: String,
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

    async fn login(&self, username: &str, password: &str) -> reqwest::Response {
        self.post(
            "/auth/login",
            json!({ "username": username, "password": password }),
        )
        .await
    }

    async fn enrolled(&self, username: &str, password: &str) -> (String, String) {
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
                json!({ "mfaToken": mfa_token, "code": totp_code(&secret, 0) }),
            )
            .await;
        assert_eq!(res.status(), 200);
        (
            res.json::<Value>().await.unwrap()["token"]
                .as_str()
                .unwrap()
                .to_string(),
            secret,
        )
    }

    /// Uses the next step's code, so it never collides with the code the enrolment already spent.
    async fn signed_in(&self, username: &str, password: &str, secret: &str) -> String {
        let body: Value = self.login(username, password).await.json().await.unwrap();
        let mfa_token = body["mfaToken"].as_str().expect("an mfaToken");
        let res = self
            .post(
                "/auth/mfa/verify",
                json!({ "mfaToken": mfa_token, "code": totp_code(secret, 30) }),
            )
            .await;
        assert_eq!(res.status(), 200);
        res.json::<Value>().await.unwrap()["token"]
            .as_str()
            .unwrap()
            .to_string()
    }

    async fn create_user(&self, username: &str, email: &str) -> String {
        let res = self
            .client
            .post(self.url("/admin/users"))
            .bearer_auth(&self.admin)
            .json(&json!({ "username": username, "email": email, "password": USER_PASSWORD }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        res.json::<Value>().await.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    async fn reset_password_as(&self, session: Option<&str>, user_id: &str) -> reqwest::Response {
        let mut req = self
            .client
            .post(self.url(&format!("/admin/users/{user_id}/reset-password")))
            .json(&json!({}));
        if let Some(session) = session {
            req = req.bearer_auth(session);
        }
        req.send().await.unwrap()
    }

    async fn admin_reset(&self, user_id: &str) -> Value {
        let res = self.reset_password_as(Some(&self.admin), user_id).await;
        assert_eq!(res.status(), 200);
        res.json().await.unwrap()
    }

    async fn use_reset_link(&self, token: &str, password: &str) -> reqwest::Response {
        self.post(
            "/auth/reset-password",
            json!({ "token": token, "password": password }),
        )
        .await
    }

    async fn set_admin_as(
        &self,
        session: Option<&str>,
        user_id: &str,
        is_admin: bool,
    ) -> reqwest::Response {
        let mut req = self
            .client
            .put(self.url(&format!("/admin/users/{user_id}/admin")))
            .json(&json!({ "isAdmin": is_admin }));
        if let Some(session) = session {
            req = req.bearer_auth(session);
        }
        req.send().await.unwrap()
    }

    async fn me(&self, session: &str) -> reqwest::StatusCode {
        self.client
            .get(self.url("/auth/me"))
            .bearer_auth(session)
            .send()
            .await
            .unwrap()
            .status()
    }

    async fn can_administer(&self, session: &str) -> bool {
        let status = self
            .client
            .get(self.url("/admin/users"))
            .bearer_auth(session)
            .send()
            .await
            .unwrap()
            .status();
        assert!(status == 200 || status == 401, "{status}");
        status == 200
    }

    async fn is_admin(&self, user_id: &str) -> bool {
        sqlx::query_scalar::<_, bool>("SELECT is_admin FROM users WHERE id = $1::uuid")
            .bind(user_id)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn reset_rows(&self) -> i64 {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM password_reset_tokens")
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn delete_user_as(&self, session: Option<&str>, user_id: &str) -> reqwest::Response {
        let mut req = self
            .client
            .delete(self.url(&format!("/admin/users/{user_id}")));
        if let Some(session) = session {
            req = req.bearer_auth(session);
        }
        req.send().await.unwrap()
    }

    async fn user_repositories_as(
        &self,
        session: Option<&str>,
        user_id: &str,
    ) -> reqwest::Response {
        let mut req = self
            .client
            .get(self.url(&format!("/admin/users/{user_id}/repositories")));
        if let Some(session) = session {
            req = req.bearer_auth(session);
        }
        req.send().await.unwrap()
    }

    async fn create_repository(&self, session: &str, name: &str) -> (String, std::path::PathBuf) {
        let res = self
            .client
            .post(self.url("/repositories"))
            .bearer_auth(session)
            .json(&json!({ "name": name, "visibility": "private" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let id = res.json::<Value>().await.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let disk_path: String =
            sqlx::query_scalar("SELECT disk_path FROM repositories WHERE id = $1::uuid")
                .bind(&id)
                .fetch_one(&self.pool)
                .await
                .unwrap();
        (
            id,
            std::path::Path::new(&self.state.config.storage_root).join(disk_path),
        )
    }

    async fn user_exists(&self, user_id: &str) -> bool {
        sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM users WHERE id = $1::uuid)")
            .bind(user_id)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn actors_of(&self, user_id: &str, event_type: &str) -> Vec<Option<String>> {
        sqlx::query_scalar::<_, Option<String>>("SELECT actor_id::text FROM domain_events WHERE aggregate_type = 'Security' AND aggregate_id = $1 AND event_type = $2 ORDER BY version")
            .bind(user_id)
            .bind(event_type)
            .fetch_all(&self.pool)
            .await
            .unwrap()
    }
}

fn totp_code(secret: &str, offset_secs: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    generate_code_at(secret, now + offset_secs)
}

/// The token is in the URL fragment (never sent to a server, so absent from access logs), not in the query string.
fn reset_link(html: &str) -> String {
    let prefix = format!("{PUBLIC_URL}/reset-password#token=");
    let start = html
        .find(&prefix)
        .unwrap_or_else(|| panic!("no reset link in the mail: {html}"));
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

    let app = build_router(state.clone(), &static_dir);
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
    let admin_id = state
        .users
        .find_by_username("admin")
        .await
        .unwrap()
        .unwrap()
        .id
        .to_string();
    let mut server = Server {
        addr,
        state,
        pool,
        mailer,
        client: reqwest::Client::new(),
        admin: String::new(),
        admin_id,
    };
    server.admin = server.enrolled("admin", ADMIN_PASSWORD).await.0;
    server
}

#[sqlx::test]
async fn both_admin_routes_refuse_anonymous_callers_and_non_admins(pool: PgPool) {
    let server = spawn_server(pool).await;
    let alice_id = server.create_user("alice", "alice@example.com").await;
    let (alice, _) = server.enrolled("alice", USER_PASSWORD).await;

    for bearer in [None, Some(alice.as_str())] {
        let who = if bearer.is_some() {
            "a non-admin"
        } else {
            "anonymous"
        };
        for target in [&alice_id, &server.admin_id] {
            assert_eq!(
                server.reset_password_as(bearer, target).await.status(),
                401,
                "reset-password as {who}"
            );
            assert_eq!(
                server.set_admin_as(bearer, target, true).await.status(),
                401,
                "promotion as {who}"
            );
            assert_eq!(
                server.set_admin_as(bearer, target, false).await.status(),
                401,
                "demotion as {who}"
            );
        }
    }

    assert!(
        !server.is_admin(&alice_id).await,
        "a non-admin cannot promote themselves"
    );
    assert!(server.is_admin(&server.admin_id).await);
    assert_eq!(server.reset_rows().await, 0, "no link was issued");
    assert!(
        server
            .mailer
            .attempted_with_subject(RESET_SUBJECT)
            .is_empty()
    );
    assert_eq!(
        server.me(&alice).await,
        200,
        "and nobody's session was revoked"
    );
    assert_eq!(server.me(&server.admin).await, 200);
}

#[sqlx::test]
async fn an_admin_reset_mails_a_one_hour_link_and_revokes_the_targets_sessions_at_once(
    pool: PgPool,
) {
    let server = spawn_server(pool).await;
    let alice_id = server.create_user("alice", "alice@example.com").await;
    let (alice, _) = server.enrolled("alice", USER_PASSWORD).await;
    assert_eq!(server.me(&alice).await, 200);

    let res = server
        .reset_password_as(Some(&server.admin), &alice_id)
        .await;

    assert_eq!(res.status(), 200);
    let text = res.text().await.unwrap();
    let body: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        body,
        json!({ "emailSent": true }),
        "the link is only handed to the admin when the mail could not be sent"
    );
    assert_eq!(server.me(&alice).await, 401);
    assert_eq!(
        server.login("alice", USER_PASSWORD).await.status(),
        401,
        "the current password is disabled at once"
    );
    assert_eq!(server.me(&server.admin).await, 200);

    let mails = server.mailer.attempted_with_subject(RESET_SUBJECT);
    assert_eq!(mails.len(), 1);
    assert_eq!(mails[0].to, "alice@example.com");
    assert!(mails[0].html.contains("<strong>alice</strong>"));
    let link = reset_link(&mails[0].html);
    assert!(
        !text.contains(&token_of(&link)),
        "the token must not be in the API response when the mail went out"
    );

    let (stored, expires_at): (String, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as("SELECT token_hash, expires_at FROM password_reset_tokens")
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_ne!(stored, token_of(&link));
    let remaining = expires_at - chrono::Utc::now();
    assert!(
        remaining > chrono::Duration::minutes(58) && remaining <= chrono::Duration::hours(1),
        "the link lives 1 h, got {remaining}"
    );

    assert_eq!(
        server.actors_of(&alice_id, "PasswordResetByAdmin").await,
        vec![Some(server.admin_id.clone())]
    );
}

#[sqlx::test]
async fn the_reset_link_sets_the_new_password_once_and_keeps_the_mfa(pool: PgPool) {
    let server = spawn_server(pool).await;
    let alice_id = server.create_user("alice", "alice@example.com").await;
    server.enrolled("alice", USER_PASSWORD).await;
    server.admin_reset(&alice_id).await;
    let token = token_of(&reset_link(
        &server.mailer.attempted_with_subject(RESET_SUBJECT)[0].html,
    ));

    let res = server.use_reset_link(&token, NEW_PASSWORD).await;

    assert_eq!(res.status(), 204);
    assert!(
        res.bytes().await.unwrap().is_empty(),
        "no body and no session"
    );
    assert_eq!(server.reset_rows().await, 0, "the link is consumed");
    assert_eq!(
        server.login("alice", USER_PASSWORD).await.status(),
        401,
        "the old password no longer works"
    );
    let login: Value = server
        .login("alice", NEW_PASSWORD)
        .await
        .json()
        .await
        .unwrap();
    assert!(login["token"].is_null(), "{login}");
    assert!(login["mfaToken"].is_string());
    assert_eq!(
        login["mfaSetupRequired"],
        json!(false),
        "a password reset leaves the MFA factors in place"
    );
    assert_eq!(login["mfaHasTotp"], json!(true));
    let notified = server
        .mailer
        .wait_for_subject(PASSWORD_CHANGED_SUBJECT, 1)
        .await;
    assert_eq!(notified.len(), 1);
    assert_eq!(notified[0].to, "alice@example.com");

    assert_eq!(
        server
            .use_reset_link(&token, "another-password-1")
            .await
            .status(),
        400
    );
    assert_eq!(
        server
            .use_reset_link(&"0".repeat(64), NEW_PASSWORD)
            .await
            .status(),
        400
    );
    assert_eq!(server.use_reset_link("", NEW_PASSWORD).await.status(), 400);
    assert_eq!(
        server.login("alice", "another-password-1").await.status(),
        401,
        "the second use changed nothing"
    );
}

/// The account stays locked from the admin's action until the link is used. There is no window where the old password can open a session or enrol a factor after an MFA reset.
#[sqlx::test]
async fn the_old_password_is_refused_from_the_admin_reset_until_the_link_sets_a_new_one(
    pool: PgPool,
) {
    let server = spawn_server(pool).await;
    let alice_id = server.create_user("alice", "alice@example.com").await;
    let (_, secret) = server.enrolled("alice", USER_PASSWORD).await;
    server.admin_reset(&alice_id).await;
    let token = token_of(&reset_link(
        &server.mailer.attempted_with_subject(RESET_SUBJECT)[0].html,
    ));

    let refused = server.login("alice", USER_PASSWORD).await;
    assert_eq!(refused.status(), 401);
    assert!(
        refused
            .json::<Value>()
            .await
            .unwrap()
            .get("mfaToken")
            .is_none(),
        "not even the MFA step is reachable"
    );

    assert_eq!(
        server.use_reset_link(&token, NEW_PASSWORD).await.status(),
        204
    );

    let session = server.signed_in("alice", NEW_PASSWORD, &secret).await;
    assert_eq!(server.me(&session).await, 200);
    assert_eq!(server.login("alice", USER_PASSWORD).await.status(), 401);
}

/// Combined with an MFA reset: whoever still holds the old password cannot enrol a factor of their own.
#[sqlx::test]
async fn after_an_mfa_reset_and_a_password_reset_the_old_password_cannot_enrol_a_factor(
    pool: PgPool,
) {
    let server = spawn_server(pool).await;
    let alice_id = server.create_user("alice", "alice@example.com").await;
    server.enrolled("alice", USER_PASSWORD).await;
    let mfa_reset = server
        .client
        .delete(server.url(&format!("/admin/users/{alice_id}/mfa")))
        .bearer_auth(&server.admin)
        .send()
        .await
        .unwrap();
    assert_eq!(mfa_reset.status(), 204);
    server.admin_reset(&alice_id).await;

    let attacker = server.login("alice", USER_PASSWORD).await;

    assert_eq!(attacker.status(), 401);
    assert!(
        attacker
            .json::<Value>()
            .await
            .unwrap()
            .get("mfaToken")
            .is_none(),
        "no mfaToken, so no enrolment of a foreign factor"
    );
}

#[sqlx::test]
async fn an_admin_cannot_reset_their_own_password_this_way(pool: PgPool) {
    let server = spawn_server(pool).await;

    let res = server
        .reset_password_as(Some(&server.admin), &server.admin_id)
        .await;

    assert_eq!(res.status(), 400);
    assert_eq!(
        res.json::<Value>().await.unwrap(),
        json!({ "error": "use your account settings to change your own password" })
    );
    assert_eq!(
        server.me(&server.admin).await,
        200,
        "the session is untouched"
    );
    assert_eq!(
        server.login("admin", ADMIN_PASSWORD).await.status(),
        200,
        "and so is the password"
    );
    assert_eq!(server.reset_rows().await, 0);
    assert!(
        server
            .actors_of(&server.admin_id, "PasswordResetByAdmin")
            .await
            .is_empty()
    );
}

#[sqlx::test]
async fn an_account_still_pending_activation_cannot_get_a_reset_link(pool: PgPool) {
    let server = spawn_server(pool).await;
    let invited = server
        .client
        .post(server.url("/admin/users/invite"))
        .bearer_auth(&server.admin)
        .json(&json!({ "username": "bob", "email": "bob@example.com" }))
        .send()
        .await
        .unwrap();
    assert_eq!(invited.status(), 200);
    let bob_id = invited.json::<Value>().await.unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let res = server.reset_password_as(Some(&server.admin), &bob_id).await;

    assert_eq!(res.status(), 400);
    assert_eq!(
        res.json::<Value>().await.unwrap(),
        json!({ "error": "the user has not activated their account yet; resend the invitation instead" })
    );
    assert_eq!(server.reset_rows().await, 0);
    assert!(
        server
            .mailer
            .attempted_with_subject(RESET_SUBJECT)
            .is_empty()
    );
    let invitations: i64 = sqlx::query_scalar("SELECT count(*) FROM user_invitations")
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(invitations, 1, "the invitation is untouched");
}

#[sqlx::test]
async fn a_weak_password_is_refused_before_the_link_is_consumed(pool: PgPool) {
    let server = spawn_server(pool).await;
    let alice_id = server.create_user("alice", "alice@example.com").await;
    server.admin_reset(&alice_id).await;
    let token = token_of(&reset_link(
        &server.mailer.attempted_with_subject(RESET_SUBJECT)[0].html,
    ));

    let weak = server.use_reset_link(&token, "short").await;

    assert_eq!(weak.status(), 400);
    assert_eq!(
        weak.json::<Value>().await.unwrap(),
        json!({ "error": "password must be at least 8 characters" })
    );
    assert_eq!(
        server.reset_rows().await,
        1,
        "the link survives the rejected attempt"
    );
    assert_eq!(
        server.use_reset_link(&token, NEW_PASSWORD).await.status(),
        204
    );
}

#[sqlx::test]
async fn an_expired_link_is_refused_and_changes_nothing(pool: PgPool) {
    let server = spawn_server(pool).await;
    let alice_id = server.create_user("alice", "alice@example.com").await;
    server.admin_reset(&alice_id).await;
    let token = token_of(&reset_link(
        &server.mailer.attempted_with_subject(RESET_SUBJECT)[0].html,
    ));
    sqlx::query("UPDATE password_reset_tokens SET expires_at = now() - interval '1 minute'")
        .execute(&server.pool)
        .await
        .unwrap();

    let res = server.use_reset_link(&token, NEW_PASSWORD).await;

    assert_eq!(res.status(), 400);
    assert_eq!(
        res.json::<Value>().await.unwrap(),
        json!({ "error": "invalid or expired password reset link" })
    );
    assert_eq!(server.login("alice", NEW_PASSWORD).await.status(), 401);
    assert_eq!(
        server.login("alice", USER_PASSWORD).await.status(),
        401,
        "still locked: only a new admin reset gets the account back"
    );
}

#[sqlx::test]
async fn a_second_admin_reset_kills_the_first_link(pool: PgPool) {
    let server = spawn_server(pool).await;
    let alice_id = server.create_user("alice", "alice@example.com").await;
    server.admin_reset(&alice_id).await;
    server.admin_reset(&alice_id).await;
    let mails = server.mailer.attempted_with_subject(RESET_SUBJECT);
    assert_eq!(mails.len(), 2);
    let (first, second) = (
        token_of(&reset_link(&mails[0].html)),
        token_of(&reset_link(&mails[1].html)),
    );
    assert_ne!(first, second);
    assert_eq!(server.reset_rows().await, 1, "one live link per user");

    assert_eq!(
        server.use_reset_link(&first, NEW_PASSWORD).await.status(),
        400,
        "the first link is dead"
    );
    assert_eq!(
        server.use_reset_link(&second, NEW_PASSWORD).await.status(),
        204
    );
    assert_eq!(
        server
            .actors_of(&alice_id, "PasswordResetByAdmin")
            .await
            .len(),
        2
    );
}

#[sqlx::test]
async fn a_failing_smtp_still_resets_and_hands_the_link_to_the_admin(pool: PgPool) {
    let server = spawn_server(pool).await;
    let alice_id = server.create_user("alice", "alice@example.com").await;
    let (alice, _) = server.enrolled("alice", USER_PASSWORD).await;
    server.mailer.fail.store(true, Ordering::SeqCst);

    let body = server.admin_reset(&alice_id).await;

    assert_eq!(body["emailSent"], json!(false));
    assert!(
        body["emailError"]
            .as_str()
            .is_some_and(|e| e.contains("smtp connection refused")),
        "{body}"
    );
    let url = body["resetUrl"]
        .as_str()
        .expect("the link is handed over when the mail did not go out");
    let attempts = server.mailer.attempted_with_subject(RESET_SUBJECT);
    assert_eq!(attempts.len(), 1);
    assert_eq!(
        url,
        reset_link(&attempts[0].html),
        "the same link a successful send would have contained"
    );
    assert!(
        server
            .mailer
            .delivered()
            .iter()
            .all(|m| m.subject != RESET_SUBJECT)
    );
    assert_eq!(
        server.me(&alice).await,
        401,
        "the sessions are revoked all the same"
    );

    assert_eq!(
        server
            .use_reset_link(&token_of(url), NEW_PASSWORD)
            .await
            .status(),
        204
    );
    assert_eq!(server.login("alice", NEW_PASSWORD).await.status(), 200);
}

#[sqlx::test]
async fn an_undeliverable_address_is_not_mailed_and_the_link_goes_to_the_admin(pool: PgPool) {
    let server = spawn_server(pool).await;
    let bob_id = server.create_user("bob", "bob@localhost").await;

    let body = server.admin_reset(&bob_id).await;

    assert_eq!(body["emailSent"], json!(false));
    assert_eq!(
        body["emailError"],
        json!("the user has no deliverable e-mail address")
    );
    let url = body["resetUrl"].as_str().unwrap();
    assert!(
        url.starts_with(&format!("{PUBLIC_URL}/reset-password#token=")),
        "{url}"
    );
    assert!(
        server
            .mailer
            .attempted_with_subject(RESET_SUBJECT)
            .is_empty(),
        "no attempt to mail an undeliverable address"
    );
    assert_eq!(
        server
            .use_reset_link(&token_of(url), NEW_PASSWORD)
            .await
            .status(),
        204
    );
    assert!(
        server
            .mailer
            .settled_with_subject(PASSWORD_CHANGED_SUBJECT)
            .await
            .is_empty(),
        "nor the confirmation"
    );
}

#[sqlx::test]
async fn an_admin_reset_of_an_unknown_user_is_404_and_sends_nothing(pool: PgPool) {
    let server = spawn_server(pool).await;

    assert_eq!(
        server
            .reset_password_as(Some(&server.admin), &uuid::Uuid::new_v4().to_string())
            .await
            .status(),
        404
    );
    assert_eq!(server.reset_rows().await, 0);
    assert!(
        server
            .mailer
            .attempted_with_subject(RESET_SUBJECT)
            .is_empty()
    );
}

#[sqlx::test]
async fn the_reset_endpoint_is_throttled_per_ip_and_shares_the_activation_budget(pool: PgPool) {
    let server = spawn_server(pool).await;
    let alice_id = server.create_user("alice", "alice@example.com").await;
    server.admin_reset(&alice_id).await;
    let token = token_of(&reset_link(
        &server.mailer.attempted_with_subject(RESET_SUBJECT)[0].html,
    ));

    for attempt in 1..=10 {
        assert_eq!(
            server
                .use_reset_link(&"f".repeat(64), NEW_PASSWORD)
                .await
                .status(),
            400,
            "attempt {attempt}"
        );
    }
    assert_eq!(
        server
            .use_reset_link(&"f".repeat(64), NEW_PASSWORD)
            .await
            .status(),
        429,
        "the 11th attempt is throttled"
    );
    assert_eq!(
        server.use_reset_link(&token, NEW_PASSWORD).await.status(),
        429,
        "even the right token is throttled"
    );
    assert_eq!(
        server
            .post(
                "/auth/activate",
                json!({ "token": "f".repeat(64), "password": NEW_PASSWORD })
            )
            .await
            .status(),
        429,
        "one budget for both endpoints"
    );
    assert_eq!(
        server.reset_rows().await,
        1,
        "the throttled attempts consumed nothing"
    );
    assert_eq!(
        server.login("alice", NEW_PASSWORD).await.status(),
        401,
        "the throttled right token set nothing"
    );
}

#[sqlx::test]
async fn promoting_a_user_gives_their_existing_session_admin_access_and_is_audited(pool: PgPool) {
    let server = spawn_server(pool).await;
    let alice_id = server.create_user("alice", "alice@example.com").await;
    let (alice, _) = server.enrolled("alice", USER_PASSWORD).await;
    assert!(!server.can_administer(&alice).await);

    let res = server
        .set_admin_as(Some(&server.admin), &alice_id, true)
        .await;

    assert_eq!(res.status(), 204);
    assert!(res.bytes().await.unwrap().is_empty());
    assert!(server.is_admin(&alice_id).await);
    assert!(
        server.can_administer(&alice).await,
        "the flag is read on every request: no new login needed"
    );
    assert_eq!(
        server.actors_of(&alice_id, "AdminGranted").await,
        vec![Some(server.admin_id.clone())]
    );

    assert_eq!(
        server
            .set_admin_as(Some(&server.admin), &alice_id, true)
            .await
            .status(),
        204
    );
    assert!(server.is_admin(&alice_id).await);
    assert_eq!(
        server.actors_of(&alice_id, "AdminGranted").await.len(),
        1,
        "a no-op leaves no audit trace"
    );
}

#[sqlx::test]
async fn demoting_a_non_admin_is_a_204_that_leaves_no_audit_trace(pool: PgPool) {
    let server = spawn_server(pool).await;
    let alice_id = server.create_user("alice", "alice@example.com").await;

    assert_eq!(
        server
            .set_admin_as(Some(&server.admin), &alice_id, false)
            .await
            .status(),
        204
    );

    assert!(!server.is_admin(&alice_id).await);
    assert!(server.actors_of(&alice_id, "AdminRevoked").await.is_empty());
}

#[sqlx::test]
async fn demoting_an_admin_who_is_not_the_last_takes_effect_on_their_next_request(pool: PgPool) {
    let server = spawn_server(pool).await;
    let alice_id = server.create_user("alice", "alice@example.com").await;
    let (alice, _) = server.enrolled("alice", USER_PASSWORD).await;
    assert_eq!(
        server
            .set_admin_as(Some(&server.admin), &alice_id, true)
            .await
            .status(),
        204
    );
    assert!(server.can_administer(&alice).await);

    assert_eq!(
        server
            .set_admin_as(Some(&server.admin), &alice_id, false)
            .await
            .status(),
        204
    );

    assert!(!server.is_admin(&alice_id).await);
    assert!(!server.can_administer(&alice).await);
    assert_eq!(
        server.me(&alice).await,
        200,
        "a demotion revokes admin access, not the session"
    );
    assert_eq!(
        server.actors_of(&alice_id, "AdminRevoked").await,
        vec![Some(server.admin_id.clone())]
    );
}

#[sqlx::test]
async fn demoting_the_last_admin_is_a_409_and_changes_nothing(pool: PgPool) {
    let server = spawn_server(pool).await;
    server.create_user("alice", "alice@example.com").await;

    let res = server
        .set_admin_as(Some(&server.admin), &server.admin_id, false)
        .await;

    assert_eq!(res.status(), 409);
    assert_eq!(
        res.json::<Value>().await.unwrap(),
        json!({ "error": "cannot remove the last administrator" })
    );
    assert!(server.is_admin(&server.admin_id).await);
    assert!(server.can_administer(&server.admin).await);
    assert!(
        server
            .actors_of(&server.admin_id, "AdminRevoked")
            .await
            .is_empty(),
        "a refused demotion is not audited as one"
    );
}

/// A pending admin cannot sign in and her link may lapse, so root is still the last usable admin.
#[sqlx::test]
async fn an_invited_admin_who_never_activated_does_not_count_toward_the_floor(pool: PgPool) {
    let server = spawn_server(pool).await;
    let invited = server
        .client
        .post(server.url("/admin/users/invite"))
        .bearer_auth(&server.admin)
        .json(&json!({ "username": "carol", "email": "carol@example.com", "isAdmin": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(invited.status(), 200);
    let carol_id = invited.json::<Value>().await.unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(server.is_admin(&carol_id).await);

    let res = server
        .set_admin_as(Some(&server.admin), &server.admin_id, false)
        .await;

    assert_eq!(res.status(), 409);
    assert!(server.is_admin(&server.admin_id).await);
    assert_eq!(
        server
            .set_admin_as(Some(&server.admin), &carol_id, false)
            .await
            .status(),
        204
    );
    assert!(!server.is_admin(&carol_id).await);
}

/// Until carol uses her reset link nobody else can sign in as an admin (and a lapsed link leaves nobody to issue another), so root is refused.
#[sqlx::test]
async fn an_admin_whose_password_reset_is_pending_does_not_count_toward_the_floor(pool: PgPool) {
    let server = spawn_server(pool).await;
    let carol_id = server.create_user("carol", "carol@example.com").await;
    let (_, carol_secret) = server.enrolled("carol", USER_PASSWORD).await;
    assert_eq!(
        server
            .set_admin_as(Some(&server.admin), &carol_id, true)
            .await
            .status(),
        204
    );
    server.admin_reset(&carol_id).await;
    let token = token_of(&reset_link(
        &server.mailer.attempted_with_subject(RESET_SUBJECT)[0].html,
    ));

    let refused = server
        .set_admin_as(Some(&server.admin), &server.admin_id, false)
        .await;

    assert_eq!(refused.status(), 409);
    assert_eq!(
        refused.json::<Value>().await.unwrap(),
        json!({ "error": "cannot remove the last administrator" })
    );
    assert!(server.is_admin(&server.admin_id).await);

    assert_eq!(
        server.use_reset_link(&token, NEW_PASSWORD).await.status(),
        204
    );
    assert_eq!(
        server
            .set_admin_as(Some(&server.admin), &server.admin_id, false)
            .await
            .status(),
        204
    );
    let carol = server.signed_in("carol", NEW_PASSWORD, &carol_secret).await;
    assert!(
        server.can_administer(&carol).await,
        "the admin left is a usable one"
    );
}

#[sqlx::test]
async fn an_admin_can_demote_themselves_while_another_admin_remains(pool: PgPool) {
    let server = spawn_server(pool).await;
    let carol_id = server.create_user("carol", "carol@example.com").await;
    let (carol, _) = server.enrolled("carol", USER_PASSWORD).await;
    assert_eq!(
        server
            .set_admin_as(Some(&server.admin), &carol_id, true)
            .await
            .status(),
        204
    );

    assert_eq!(
        server
            .set_admin_as(Some(&server.admin), &server.admin_id, false)
            .await
            .status(),
        204
    );

    assert!(!server.is_admin(&server.admin_id).await);
    assert!(
        !server.can_administer(&server.admin).await,
        "the self-demotion applies to the very next request"
    );
    assert_eq!(
        server.actors_of(&server.admin_id, "AdminRevoked").await,
        vec![Some(server.admin_id.clone())]
    );

    let res = server.set_admin_as(Some(&carol), &carol_id, false).await;
    assert_eq!(res.status(), 409);
    assert!(server.is_admin(&carol_id).await);
    assert_eq!(
        server
            .set_admin_as(Some(&carol), &server.admin_id, true)
            .await
            .status(),
        204
    );
    assert!(server.can_administer(&server.admin).await);
}

#[sqlx::test]
async fn setting_the_flag_of_an_unknown_user_is_404_and_a_malformed_body_is_refused(pool: PgPool) {
    let server = spawn_server(pool).await;
    let alice_id = server.create_user("alice", "alice@example.com").await;

    for is_admin in [true, false] {
        assert_eq!(
            server
                .set_admin_as(
                    Some(&server.admin),
                    &uuid::Uuid::new_v4().to_string(),
                    is_admin
                )
                .await
                .status(),
            404
        );
    }
    for body in [
        json!({}),
        json!({ "is_admin": true }),
        json!({ "isAdmin": "yes" }),
    ] {
        let res = server
            .client
            .put(server.url(&format!("/admin/users/{alice_id}/admin")))
            .bearer_auth(&server.admin)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert!(res.status().is_client_error(), "{body}: {}", res.status());
    }
    assert!(!server.is_admin(&alice_id).await);
    assert!(
        server
            .state
            .users
            .find_by_username("alice")
            .await
            .unwrap()
            .is_some_and(|u| !u.is_admin)
    );
}

/// The exact key set is the contract, so a field that would let the admin page reach into the repository cannot slip in unnoticed.
#[sqlx::test]
async fn an_admin_lists_a_users_personal_repositories_with_their_size_and_nothing_else(
    pool: PgPool,
) {
    let server = spawn_server(pool).await;
    let alice_id = server.create_user("alice", "alice@example.com").await;
    let (alice, _) = server.enrolled("alice", USER_PASSWORD).await;
    server.create_repository(&alice, "first").await;
    let (second_id, second_dir) = server.create_repository(&alice, "second").await;
    std::fs::remove_dir_all(&second_dir).unwrap();
    server.create_repository(&server.admin, "admins-own").await;

    let res = server
        .user_repositories_as(Some(&server.admin), &alice_id)
        .await;

    assert_eq!(res.status(), 200);
    let rows: Vec<Value> = res.json().await.unwrap();
    assert_eq!(rows.len(), 2, "alice's repositories only: {rows:?}");
    for row in &rows {
        let mut keys: Vec<&str> = row
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "createdAt",
                "description",
                "id",
                "name",
                "sizeBytes",
                "visibility"
            ],
            "{row}"
        );
        assert_eq!(row["visibility"], "private");
    }
    let first = rows.iter().find(|r| r["name"] == "first").unwrap();
    assert!(
        first["sizeBytes"].as_u64().is_some_and(|size| size > 0),
        "{first}"
    );
    let second = rows.iter().find(|r| r["name"] == "second").unwrap();
    assert_eq!(second["id"], json!(second_id));
    assert_eq!(second["sizeBytes"], Value::Null);
}

#[sqlx::test]
async fn a_user_without_repositories_lists_none_and_an_unknown_user_is_404(pool: PgPool) {
    let server = spawn_server(pool).await;
    let alice_id = server.create_user("alice", "alice@example.com").await;

    let res = server
        .user_repositories_as(Some(&server.admin), &alice_id)
        .await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.json::<Value>().await.unwrap(), json!([]));

    assert_eq!(
        server
            .user_repositories_as(Some(&server.admin), &uuid::Uuid::new_v4().to_string())
            .await
            .status(),
        404
    );
}

#[sqlx::test]
async fn the_deletion_and_repository_routes_refuse_anonymous_callers_and_non_admins(pool: PgPool) {
    let server = spawn_server(pool).await;
    let alice_id = server.create_user("alice", "alice@example.com").await;
    let (alice, _) = server.enrolled("alice", USER_PASSWORD).await;
    let bob_id = server.create_user("bob", "bob@example.com").await;

    for bearer in [None, Some(alice.as_str())] {
        let who = if bearer.is_some() {
            "a non-admin"
        } else {
            "anonymous"
        };
        for target in [&alice_id, &bob_id, &server.admin_id] {
            assert_eq!(
                server.delete_user_as(bearer, target).await.status(),
                401,
                "deletion as {who}"
            );
            assert_eq!(
                server.user_repositories_as(bearer, target).await.status(),
                401,
                "repositories as {who}"
            );
        }
    }

    for id in [&alice_id, &bob_id, &server.admin_id] {
        assert!(server.user_exists(id).await);
    }
    assert!(
        server
            .actors_of(&bob_id, "UserDeletedByAdmin")
            .await
            .is_empty()
    );
}

#[sqlx::test]
async fn an_admin_deletes_a_user_with_their_repositories_on_disk_and_their_sessions(pool: PgPool) {
    let server = spawn_server(pool).await;
    let alice_id = server.create_user("alice", "alice@example.com").await;
    let (alice, _) = server.enrolled("alice", USER_PASSWORD).await;
    let (first, first_dir) = server.create_repository(&alice, "first").await;
    let (second, second_dir) = server.create_repository(&alice, "second").await;
    let (admins, admins_dir) = server.create_repository(&server.admin, "admins-own").await;
    assert!(first_dir.exists() && second_dir.exists());

    let res = server.delete_user_as(Some(&server.admin), &alice_id).await;

    assert_eq!(res.status(), 204);
    assert!(res.bytes().await.unwrap().is_empty());
    assert!(!server.user_exists(&alice_id).await);
    let remaining: Vec<String> = sqlx::query_scalar("SELECT id::text FROM repositories")
        .fetch_all(&server.pool)
        .await
        .unwrap();
    assert_eq!(
        remaining,
        vec![admins.clone()],
        "alice's repositories ({first}, {second}) are gone, nobody else's"
    );
    assert!(
        !first_dir.exists() && !second_dir.exists(),
        "their git directories are removed from disk"
    );
    assert!(admins_dir.exists());
    assert_eq!(
        server.me(&alice).await,
        401,
        "a deleted user's session is signed out, not a 404"
    );
    assert_eq!(server.login("alice", USER_PASSWORD).await.status(), 401);
    assert_eq!(server.me(&server.admin).await, 200);
    assert_eq!(
        server.actors_of(&alice_id, "UserDeletedByAdmin").await,
        vec![Some(server.admin_id.clone())]
    );
    let payload: Value = sqlx::query_scalar("SELECT payload FROM domain_events WHERE aggregate_id = $1 AND event_type = 'UserDeletedByAdmin'").bind(&alice_id).fetch_one(&server.pool).await.unwrap();
    assert_eq!(payload["username"], "alice");
    let mut destroyed: Vec<String> =
        serde_json::from_value(payload["deleted_repositories"].clone()).unwrap();
    destroyed.sort();
    assert_eq!(destroyed, vec!["first".to_string(), "second".to_string()]);

    assert_eq!(
        server
            .delete_user_as(Some(&server.admin), &alice_id)
            .await
            .status(),
        404
    );
    assert_eq!(
        server
            .actors_of(&alice_id, "UserDeletedByAdmin")
            .await
            .len(),
        1
    );
}

#[sqlx::test]
async fn an_admin_cannot_delete_their_own_account(pool: PgPool) {
    let server = spawn_server(pool).await;
    let carol_id = server.create_user("carol", "carol@example.com").await;
    assert_eq!(
        server
            .set_admin_as(Some(&server.admin), &carol_id, true)
            .await
            .status(),
        204
    );

    let res = server
        .delete_user_as(Some(&server.admin), &server.admin_id)
        .await;

    assert_eq!(res.status(), 400);
    assert_eq!(
        res.json::<Value>().await.unwrap(),
        json!({ "error": "you cannot delete your own account" })
    );
    assert!(server.user_exists(&server.admin_id).await);
    assert_eq!(server.me(&server.admin).await, 200);
    assert!(
        server
            .actors_of(&server.admin_id, "UserDeletedByAdmin")
            .await
            .is_empty()
    );
}

#[sqlx::test]
async fn deleting_an_unknown_user_is_404(pool: PgPool) {
    let server = spawn_server(pool).await;

    assert_eq!(
        server
            .delete_user_as(Some(&server.admin), &uuid::Uuid::new_v4().to_string())
            .await
            .status(),
        404
    );
}

#[sqlx::test]
async fn an_admin_deletes_another_admin_while_they_remain_one(pool: PgPool) {
    let server = spawn_server(pool).await;
    let carol_id = server.create_user("carol", "carol@example.com").await;
    let (carol, _) = server.enrolled("carol", USER_PASSWORD).await;
    assert_eq!(
        server
            .set_admin_as(Some(&server.admin), &carol_id, true)
            .await
            .status(),
        204
    );

    assert_eq!(
        server
            .delete_user_as(Some(&server.admin), &carol_id)
            .await
            .status(),
        204
    );

    assert!(!server.user_exists(&carol_id).await);
    assert_eq!(server.me(&carol).await, 401);
    assert!(server.can_administer(&server.admin).await);
}

/// The acting admin has a pending password reset (row inserted by hand, session left alive), so carol is the last admin who can sign in: 409, like her demotion.
#[sqlx::test]
async fn deleting_the_last_active_admin_is_a_409_and_changes_nothing(pool: PgPool) {
    let server = spawn_server(pool).await;
    let carol_id = server.create_user("carol", "carol@example.com").await;
    let (carol, _) = server.enrolled("carol", USER_PASSWORD).await;
    assert_eq!(
        server
            .set_admin_as(Some(&server.admin), &carol_id, true)
            .await
            .status(),
        204
    );
    let (carols, carols_dir) = server.create_repository(&carol, "infra").await;
    sqlx::query("INSERT INTO password_reset_tokens (user_id, token_hash, expires_at) VALUES ($1::uuid, 'x', now() + interval '1 hour')").bind(&server.admin_id).execute(&server.pool).await.unwrap();

    let res = server.delete_user_as(Some(&server.admin), &carol_id).await;

    assert_eq!(res.status(), 409);
    assert_eq!(
        res.json::<Value>().await.unwrap(),
        json!({ "error": "cannot remove the last administrator" })
    );
    assert!(server.user_exists(&carol_id).await);
    assert_eq!(server.me(&carol).await, 200);
    let still_there: i64 =
        sqlx::query_scalar("SELECT count(*) FROM repositories WHERE id = $1::uuid")
            .bind(&carols)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(
        still_there, 1,
        "the refusal comes before any repository is touched"
    );
    assert!(carols_dir.exists());
    assert!(
        server
            .actors_of(&carol_id, "UserDeletedByAdmin")
            .await
            .is_empty()
    );
}

/// Deleting alice would leave the group with nobody able to manage it (removing her from the group is refused too). Once bob is promoted she can go.
#[sqlx::test]
async fn deleting_the_last_maintainer_of_a_group_is_a_409_until_another_member_is_promoted(
    pool: PgPool,
) {
    let server = spawn_server(pool).await;
    let alice_id = server.create_user("alice", "alice@example.com").await;
    let (alice, _) = server.enrolled("alice", USER_PASSWORD).await;
    let bob_id = server.create_user("bob", "bob@example.com").await;
    let created = server
        .client
        .post(server.url("/groups"))
        .bearer_auth(&alice)
        .json(&json!({ "name": "acme" }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 200);
    let group_id = created.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let added = server
        .client
        .post(server.url(&format!("/groups/{group_id}/members")))
        .bearer_auth(&alice)
        .json(&json!({ "username": "bob", "role": "reader" }))
        .send()
        .await
        .unwrap();
    assert!(added.status().is_success(), "{}", added.status());

    let refused = server.delete_user_as(Some(&server.admin), &alice_id).await;

    assert_eq!(refused.status(), 409);
    assert_eq!(
        refused.json::<Value>().await.unwrap(),
        json!({ "error": "the user is the last maintainer of the group acme; promote another member first" })
    );
    assert!(server.user_exists(&alice_id).await);
    assert!(
        server
            .actors_of(&alice_id, "UserDeletedByAdmin")
            .await
            .is_empty()
    );

    let promoted = server
        .client
        .patch(server.url(&format!("/groups/{group_id}/members/bob")))
        .bearer_auth(&alice)
        .json(&json!({ "role": "maintainer" }))
        .send()
        .await
        .unwrap();
    assert!(promoted.status().is_success(), "{}", promoted.status());
    assert_eq!(
        server
            .delete_user_as(Some(&server.admin), &alice_id)
            .await
            .status(),
        204
    );

    let (created_by, bob_role): (Option<uuid::Uuid>, String) =
        sqlx::query_as("SELECT g.created_by, m.role FROM groups g JOIN group_members m ON m.group_id = g.id WHERE g.id = $1::uuid AND m.user_id = $2::uuid").bind(&group_id).bind(&bob_id).fetch_one(&server.pool).await.unwrap();
    assert_eq!((created_by, bob_role.as_str()), (None, "maintainer"));
}

/// Two last admins deleting each other concurrently: exactly one deletion happens, the other gets 409 (floor) or 401 (author gone).
#[sqlx::test]
async fn two_admins_deleting_each_other_at_once_leave_exactly_one(pool: PgPool) {
    let server = spawn_server(pool).await;
    let carol_id = server.create_user("carol", "carol@example.com").await;
    let (carol, _) = server.enrolled("carol", USER_PASSWORD).await;
    assert_eq!(
        server
            .set_admin_as(Some(&server.admin), &carol_id, true)
            .await
            .status(),
        204
    );
    let (admins_repo, admins_dir) = server.create_repository(&server.admin, "admins-own").await;
    let (carols_repo, carols_dir) = server.create_repository(&carol, "carols-own").await;

    let (a, b) = tokio::join!(
        server.delete_user_as(Some(&server.admin), &carol_id),
        server.delete_user_as(Some(&carol), &server.admin_id)
    );

    let statuses = [a.status().as_u16(), b.status().as_u16()];
    assert_eq!(
        statuses.iter().filter(|s| **s == 204).count(),
        1,
        "{statuses:?}"
    );
    assert!(
        statuses.iter().all(|s| [204, 401, 409].contains(s)),
        "{statuses:?}"
    );
    let admins: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE is_admin")
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(admins, 1);
    let (survivor_repo, survivor_dir, gone_repo, gone_dir) =
        if server.user_exists(&server.admin_id).await {
            (admins_repo, admins_dir, carols_repo, carols_dir)
        } else {
            (carols_repo, carols_dir, admins_repo, admins_dir)
        };
    let remaining: Vec<String> = sqlx::query_scalar("SELECT id::text FROM repositories")
        .fetch_all(&server.pool)
        .await
        .unwrap();
    assert_eq!(
        remaining,
        vec![survivor_repo],
        "{gone_repo} is the deleted admin's"
    );
    assert!(survivor_dir.exists());
    assert!(!gone_dir.exists());
}

/// A repository created inside a group belongs to the group: it survives its creator's deletion.
#[sqlx::test]
async fn a_group_repository_the_user_created_survives_with_the_admin_as_its_owner(pool: PgPool) {
    let server = spawn_server(pool).await;
    let alice_id = server.create_user("alice", "alice@example.com").await;
    let (alice, _) = server.enrolled("alice", USER_PASSWORD).await;
    server.create_user("bob", "bob@example.com").await;
    let (bob, _) = server.enrolled("bob", USER_PASSWORD).await;
    let created = server
        .client
        .post(server.url("/groups"))
        .bearer_auth(&alice)
        .json(&json!({ "name": "acme" }))
        .send()
        .await
        .unwrap();
    let group_id = created.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    // Bob co-maintains the group, or alice would be its last Maintainer and the deletion refused.
    let added = server
        .client
        .post(server.url(&format!("/groups/{group_id}/members")))
        .bearer_auth(&alice)
        .json(&json!({ "username": "bob", "role": "maintainer" }))
        .send()
        .await
        .unwrap();
    assert!(added.status().is_success(), "{}", added.status());
    let res = server
        .client
        .post(server.url("/repositories"))
        .bearer_auth(&alice)
        .json(&json!({ "name": "api", "visibility": "private", "groupPath": "acme" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let repository_id = res.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let disk_path: String =
        sqlx::query_scalar("SELECT disk_path FROM repositories WHERE id = $1::uuid")
            .bind(&repository_id)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    let dir = std::path::Path::new(&server.state.config.storage_root).join(disk_path);
    assert!(dir.exists());

    assert_eq!(
        server
            .delete_user_as(Some(&server.admin), &alice_id)
            .await
            .status(),
        204
    );

    let owner: String =
        sqlx::query_scalar("SELECT owner_id::text FROM repositories WHERE id = $1::uuid")
            .bind(&repository_id)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(owner, server.admin_id);
    assert!(dir.exists(), "its git directory is untouched");
    let shown = server
        .client
        .get(server.url(&format!("/repositories/by-id/{repository_id}")))
        .bearer_auth(&bob)
        .send()
        .await
        .unwrap();
    assert_eq!(shown.status(), 200);
    let shown: Value = shown.json().await.unwrap();
    assert_eq!(
        (shown["owner"].clone(), shown["path"].clone()),
        (json!("admin"), json!(["acme", "api"]))
    );
    let payload: Value = sqlx::query_scalar("SELECT payload FROM domain_events WHERE aggregate_id = $1 AND event_type = 'UserDeletedByAdmin'").bind(&alice_id).fetch_one(&server.pool).await.unwrap();
    assert_eq!(
        payload["deleted_repositories"],
        json!([]),
        "a group repository is not one of hers to destroy"
    );
}

/// The author (and author id) is `null`, never a 500.
#[sqlx::test]
async fn content_a_deleted_user_wrote_elsewhere_still_reads_with_a_null_author(pool: PgPool) {
    let server = spawn_server(pool).await;
    let alice_id = server.create_user("alice", "alice@example.com").await;
    let bob_id = server.create_user("bob", "bob@example.com").await;
    let (bob, _) = server.enrolled("bob", USER_PASSWORD).await;
    let (repository_id, _) = server.create_repository(&bob, "hello").await;
    // Seeded directly and closed, so no live diff is computed against branches that do not exist.
    let merge_request_id: String = sqlx::query_scalar(
        "INSERT INTO merge_requests (repository_id, author_id, source_branch, target_branch, title, status, closed_at) VALUES ($1::uuid, $2::uuid, 'feature', 'main', 'by alice', 'closed', now()) RETURNING id::text",
    )
    .bind(&repository_id)
    .bind(&alice_id)
    .fetch_one(&server.pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO merge_request_comments (merge_request_id, author_id, body) VALUES ($1::uuid, $2::uuid, 'from alice'), ($1::uuid, $3::uuid, 'from bob')")
        .bind(&merge_request_id)
        .bind(&alice_id)
        .bind(&bob_id)
        .execute(&server.pool)
        .await
        .unwrap();
    let release_id: String = sqlx::query_scalar("INSERT INTO releases (repository_id, tag_name, title, author_id, published_at) VALUES ($1::uuid, 'v1', 'v1', $2::uuid, now()) RETURNING id::text")
        .bind(&repository_id)
        .bind(&alice_id)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO release_assets (release_id, filename, content_type, size_bytes, disk_path, uploaded_by) VALUES ($1::uuid, 'a.txt', 'text/plain', 1, 'x', $2::uuid)")
        .bind(&release_id)
        .bind(&alice_id)
        .execute(&server.pool)
        .await
        .unwrap();

    assert_eq!(
        server
            .delete_user_as(Some(&server.admin), &alice_id)
            .await
            .status(),
        204
    );

    let get = |path: String| {
        let req = server.client.get(server.url(&path)).bearer_auth(&bob);
        async move {
            let res = req.send().await.unwrap();
            let status = res.status();
            let body = res.text().await.unwrap();
            assert_eq!(status, 200, "{path}: {body}");
            serde_json::from_str::<Value>(&body).unwrap()
        }
    };
    let listed = get(format!("/repositories/{repository_id}/merge-requests")).await;
    assert_eq!(
        (listed[0]["title"].clone(), listed[0]["author"].clone()),
        (json!("by alice"), Value::Null)
    );
    assert_eq!(
        get(format!("/merge-requests/{merge_request_id}")).await["author"],
        Value::Null
    );
    let comments = get(format!("/merge-requests/{merge_request_id}/comments")).await;
    let from_alice = comments
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["body"] == "from alice")
        .unwrap();
    assert_eq!(
        (from_alice["authorId"].clone(), from_alice["author"].clone()),
        (Value::Null, Value::Null)
    );
    let from_bob = comments
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["body"] == "from bob")
        .unwrap();
    assert_eq!(
        from_bob["author"]["username"], "bob",
        "a comment by someone who still exists is unchanged"
    );
    let timeline = get(format!("/merge-requests/{merge_request_id}/timeline")).await;
    assert_eq!(timeline["author"], Value::Null);
    let timeline_alice = timeline["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["body"] == "from alice")
        .unwrap();
    assert_eq!(timeline_alice["author"], Value::Null);
    let releases = get(format!("/repositories/{repository_id}/releases")).await;
    assert_eq!(
        (
            releases[0]["authorId"].clone(),
            releases[0]["author"].clone()
        ),
        (Value::Null, Value::Null)
    );
    let release = get(format!("/repositories/{repository_id}/releases/v1")).await;
    assert_eq!(
        (release["authorId"].clone(), release["author"].clone()),
        (Value::Null, Value::Null)
    );
    assert_eq!(
        (
            release["assets"][0]["uploadedBy"].clone(),
            release["assets"][0]["uploader"].clone()
        ),
        (Value::Null, Value::Null)
    );
}
