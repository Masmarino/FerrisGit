// Free registration: a fresh registration gets no session, only the `mfaToken` leading to the mandatory TOTP setup.
// `mfa_enforced` stays true.

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
use std::sync::{Arc, Mutex};

const ADMIN_PASSWORD: &str = "adminpassword123";
const PASSWORD: &str = "correct-horse-battery";

struct RecordingEmail {
    sent: Mutex<Vec<(String, String)>>,
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
        self.sent
            .lock()
            .unwrap()
            .push((to.to_string(), subject.to_string()));
        Ok(())
    }
}

struct Server {
    addr: SocketAddr,
    pool: PgPool,
    client: reqwest::Client,
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

    async fn register(&self, username: &str, email: &str, password: &str) -> reqwest::Response {
        self.post(
            "/auth/register",
            json!({ "username": username, "email": email, "password": password }),
        )
        .await
    }

    async fn login(&self, username: &str, password: &str) -> reqwest::Response {
        self.post(
            "/auth/login",
            json!({ "username": username, "password": password }),
        )
        .await
    }

    async fn get_config(&self) -> Value {
        let res = self
            .client
            .get(self.url("/auth/config"))
            .send()
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            200,
            "the public config needs no authentication"
        );
        res.json().await.unwrap()
    }

    async fn admin_session(&self) -> String {
        let body: Value = self
            .login("admin", ADMIN_PASSWORD)
            .await
            .json()
            .await
            .unwrap();
        let mfa_token = body["mfaToken"].as_str().unwrap().to_string();
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

    async fn put_settings(&self, admin_session: &str, body: Value) -> reqwest::Response {
        self.client
            .put(self.url("/admin/settings"))
            .bearer_auth(admin_session)
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    async fn enable_registration(&self, admin_session: &str) {
        let res = self
            .put_settings(admin_session, json!({ "registrationEnabled": true }))
            .await;
        assert_eq!(res.status(), 200);
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
        public_url: "http://localhost:4200".to_string(),
        trusted_proxy_cidrs: vec![],
    };

    let mut state = AppState::new(pool.clone(), config.clone()).await;
    assert!(
        state.mfa_enforced,
        "AppState::new must enforce MFA: production has no way to turn it off"
    );
    state.mailer = Arc::new(Mailer::new(Arc::new(RecordingEmail {
        sent: Mutex::new(Vec::new()),
    })));
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
    Server {
        addr,
        pool,
        client: reqwest::Client::new(),
    }
}

#[sqlx::test]
async fn registration_is_disabled_by_default_and_the_admin_switch_turns_it_on_and_off(
    pool: PgPool,
) {
    let server = spawn_server(pool).await;
    assert_eq!(
        server.get_config().await,
        json!({ "registrationEnabled": false, "passkeysAvailable": true, "publicPagesEnabled": true })
    );
    let admin = server.admin_session().await;

    server.enable_registration(&admin).await;
    assert_eq!(
        server.get_config().await,
        json!({ "registrationEnabled": true, "passkeysAvailable": true, "publicPagesEnabled": true })
    );

    let res = server
        .put_settings(&admin, json!({ "registrationEnabled": false }))
        .await;
    assert_eq!(res.status(), 200);
    assert_eq!(
        server.get_config().await,
        json!({ "registrationEnabled": false, "passkeysAvailable": true, "publicPagesEnabled": true })
    );
}

#[sqlx::test]
async fn registering_while_disabled_is_a_400_and_creates_no_user(pool: PgPool) {
    let server = spawn_server(pool).await;
    let before = server.user_count().await;

    let res = server
        .register("alice", "alice@example.com", PASSWORD)
        .await;

    assert_eq!(res.status(), 400);
    assert_eq!(server.user_count().await, before);
    assert_eq!(server.login("alice", PASSWORD).await.status(), 401);
}

/// A disabled instance answers the same whatever is submitted: it must not become an account-probing oracle
/// (a 409 for a taken name or e-mail, a different 400 for invalid input) while registration is off.
#[sqlx::test]
async fn a_disabled_instance_answers_the_same_whatever_is_submitted(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;
    let res = server
        .client
        .post(server.url("/admin/users"))
        .bearer_auth(&admin)
        .json(&json!({ "username": "alice", "email": "alice@example.com", "password": PASSWORD }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let before = server.user_count().await;

    // Ten cases at most (the register throttle is 10 attempts per IP), so no 429 can hide a difference.
    for (username, email, password, why) in [
        ("alice", "fresh@example.com", PASSWORD, "taken username"),
        (
            "ALICE",
            "fresh@example.com",
            PASSWORD,
            "taken username, other casing",
        ),
        ("bob", "alice@example.com", PASSWORD, "taken e-mail"),
        (
            "bob",
            "ALICE@example.com",
            PASSWORD,
            "taken e-mail, other casing",
        ),
        ("ab", "bob@example.com", PASSWORD, "invalid username"),
        ("login", "bob@example.com", PASSWORD, "reserved username"),
        ("bob", "not-an-email", PASSWORD, "invalid e-mail"),
        ("bob", "bob@example.com", "short", "invalid password"),
        ("bob", "bob@example.com", PASSWORD, "valid and free"),
    ] {
        let res = server.register(username, email, password).await;
        assert_eq!(res.status(), 400, "{why}");
        assert_eq!(
            res.json::<Value>().await.unwrap(),
            json!({ "error": "registration is disabled" }),
            "{why}"
        );
    }
    assert_eq!(server.user_count().await, before, "nobody was created");
}

#[sqlx::test]
async fn registration_hands_out_the_login_response_and_no_session(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;
    server.enable_registration(&admin).await;

    let res = server
        .register("Alice_1", "Alice@Example.com", PASSWORD)
        .await;

    assert_eq!(res.status(), 200);
    let text = res.text().await.unwrap();
    assert!(
        !text.contains(PASSWORD),
        "the password must never be echoed: {text}"
    );
    let body: Value = serde_json::from_str(&text).unwrap();
    assert!(
        body["token"].is_null(),
        "a registration must not give a session: {body}"
    );
    assert!(
        body["mfaToken"].as_str().is_some_and(|t| !t.is_empty()),
        "an mfaToken is expected: {body}"
    );
    assert_eq!(body["mfaSetupRequired"], json!(true));
    assert_eq!(body["mfaHasTotp"], json!(false));
    assert_eq!(body["mfaHasPasskey"], json!(false));

    let row: (String, String, bool) = sqlx::query_as(
        "SELECT username, email, is_admin FROM users WHERE lower(username) = 'alice_1'",
    )
    .fetch_one(&server.pool)
    .await
    .unwrap();
    assert_eq!(row.0, "alice_1", "usernames are stored lower-cased");
    assert_eq!(
        row.1, "Alice@Example.com",
        "the e-mail keeps the case that was typed"
    );
    assert!(!row.2, "a registered user is never an admin");
}

#[sqlx::test]
async fn a_registered_user_signs_in_with_the_casing_typed_at_sign_up(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;
    server.enable_registration(&admin).await;
    assert_eq!(
        server
            .register("Alice_1", "alice@example.com", PASSWORD)
            .await
            .status(),
        200
    );

    for typed in ["Alice_1", "ALICE_1", "alice_1", "  Alice_1 "] {
        let res = server.login(typed, PASSWORD).await;
        assert_eq!(res.status(), 200, "signing in as {typed:?}");
        let body: Value = res.json().await.unwrap();
        assert!(body["token"].is_null());
        assert!(
            body["mfaToken"].as_str().is_some_and(|t| !t.is_empty()),
            "{body}"
        );
    }
    assert_eq!(
        server.login("Alice_1", "not-the-password").await.status(),
        401
    );
}

#[sqlx::test]
async fn a_registered_user_completes_the_mfa_setup_and_reaches_me(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;
    server.enable_registration(&admin).await;

    let body: Value = server
        .register("alice", "alice@example.com", PASSWORD)
        .await
        .json()
        .await
        .unwrap();
    let mfa_token = body["mfaToken"].as_str().unwrap();
    let me = server
        .client
        .get(server.url("/auth/me"))
        .bearer_auth(mfa_token)
        .send()
        .await
        .unwrap();
    assert_eq!(me.status(), 401);

    let enrol = server
        .post(
            "/auth/mfa/setup/totp/enroll",
            json!({ "mfaToken": mfa_token }),
        )
        .await;
    assert_eq!(enrol.status(), 200);
    let secret = enrol.json::<Value>().await.unwrap()["secret"]
        .as_str()
        .unwrap()
        .to_string();
    let confirm = server
        .post(
            "/auth/mfa/setup/totp/confirm",
            json!({ "mfaToken": mfa_token, "code": totp_code(&secret) }),
        )
        .await;
    assert_eq!(confirm.status(), 200);
    let confirmed: Value = confirm.json().await.unwrap();
    assert_eq!(confirmed["backupCodes"].as_array().unwrap().len(), 10);

    let session = confirmed["token"].as_str().unwrap();
    let me = server
        .client
        .get(server.url("/auth/me"))
        .bearer_auth(session)
        .send()
        .await
        .unwrap();
    assert_eq!(me.status(), 200);
    let me: Value = me.json().await.unwrap();
    assert_eq!(me["username"], "alice");
    assert_eq!(me["email"], "alice@example.com");
    assert_eq!(me["isAdmin"], json!(false));

    assert_eq!(
        server.login("alice", "not-the-password").await.status(),
        401
    );
    let next: Value = server.login("alice", PASSWORD).await.json().await.unwrap();
    assert!(next["token"].is_null());
    assert_eq!(
        next["mfaSetupRequired"],
        json!(false),
        "the factor is enrolled: the next login is a challenge"
    );
    assert_eq!(next["mfaHasTotp"], json!(true));
}

#[sqlx::test]
async fn a_taken_username_or_email_is_a_409_whatever_the_casing(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;
    server.enable_registration(&admin).await;
    assert_eq!(
        server
            .register("alice", "alice@example.com", PASSWORD)
            .await
            .status(),
        200
    );

    assert_eq!(
        server
            .register("alice", "other@example.com", PASSWORD)
            .await
            .status(),
        409,
        "same username"
    );
    assert_eq!(
        server
            .register("ALICE", "other@example.com", PASSWORD)
            .await
            .status(),
        409,
        "same username, other casing"
    );
    assert_eq!(
        server
            .register("Admin", "other@example.com", PASSWORD)
            .await
            .status(),
        400,
        "`admin` is a reserved name anyway"
    );
    assert_eq!(
        server
            .register("bob", "alice@example.com", PASSWORD)
            .await
            .status(),
        409,
        "same e-mail"
    );
    assert_eq!(
        server
            .register("bob", "ALICE@Example.COM", PASSWORD)
            .await
            .status(),
        409,
        "same e-mail, other casing"
    );
    assert_eq!(
        server.user_count().await,
        2,
        "only the admin and alice exist"
    );
}

#[sqlx::test]
async fn invalid_input_is_a_400_and_creates_nothing(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;
    server.enable_registration(&admin).await;
    let before = server.user_count().await;

    for (username, email, password, why) in [
        ("ab", "a@example.com", PASSWORD, "username too short"),
        (
            "1alice",
            "a@example.com",
            PASSWORD,
            "username starting with a digit",
        ),
        ("ali ce", "a@example.com", PASSWORD, "username with a space"),
        (
            "alice.git",
            "a@example.com",
            PASSWORD,
            "username with a dot",
        ),
        ("alice", "not-an-email", PASSWORD, "invalid e-mail"),
        (
            "alice",
            "a@example.com",
            "short",
            "password shorter than 8 characters",
        ),
        ("alice", "a@example.com", "", "empty password"),
        (
            "login",
            "a@example.com",
            PASSWORD,
            "reserved name (SPA route)",
        ),
        (
            "API",
            "a@example.com",
            PASSWORD,
            "reserved name (infrastructure), any casing",
        ),
        (
            "register",
            "a@example.com",
            PASSWORD,
            "reserved name (this very route)",
        ),
    ] {
        let res = server.register(username, email, password).await;
        assert_eq!(res.status(), 400, "{why}");
        let text = res.text().await.unwrap();
        assert!(
            !text.contains(PASSWORD),
            "{why}: the password must not be echoed in an error: {text}"
        );
    }
    assert_eq!(server.user_count().await, before);
}

#[sqlx::test]
async fn registration_is_throttled_per_ip_on_the_eleventh_attempt(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;
    server.enable_registration(&admin).await;

    // Invalid attempts count too: the gate is the connection, not the outcome.
    for attempt in 1..=10 {
        assert_eq!(
            server
                .register("ab", "a@example.com", PASSWORD)
                .await
                .status(),
            400,
            "attempt {attempt}"
        );
    }
    assert_eq!(
        server
            .register("ab", "a@example.com", PASSWORD)
            .await
            .status(),
        429,
        "the 11th attempt is throttled"
    );
    assert_eq!(
        server
            .register("alice", "alice@example.com", PASSWORD)
            .await
            .status(),
        429,
        "a valid registration is throttled too"
    );
    assert_eq!(
        server.user_count().await,
        1,
        "nobody was created through the throttled attempts"
    );
}

#[sqlx::test]
async fn the_register_and_activate_routes_refuse_a_body_over_16_kib(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;
    server.enable_registration(&admin).await;
    let huge = "x".repeat(20 * 1024);

    let res = server.register("alice", "alice@example.com", &huge).await;
    assert_eq!(res.status(), 413);
    let res = server
        .post("/auth/activate", json!({ "token": "t", "password": huge }))
        .await;
    assert_eq!(res.status(), 413);
    assert_eq!(server.user_count().await, 1);
}

#[sqlx::test]
async fn the_admin_settings_round_trip_the_switch_without_touching_the_other_fields(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;
    let get = || async {
        let res = server
            .client
            .get(server.url("/admin/settings"))
            .bearer_auth(&admin)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        res.json::<Value>().await.unwrap()
    };

    let initial = get().await;
    assert_eq!(initial["registrationEnabled"], json!(false));

    let res = server
        .put_settings(&admin, json!({ "registrationEnabled": true }))
        .await;
    assert_eq!(res.status(), 200);
    let after_switch: Value = res.json().await.unwrap();
    assert_eq!(after_switch["registrationEnabled"], json!(true));
    let mut expected = initial.clone();
    expected["registrationEnabled"] = json!(true);
    assert_eq!(after_switch, expected);
    assert_eq!(get().await, expected);

    let res = server
        .put_settings(&admin, json!({ "jwtTtlHours": 6 }))
        .await;
    assert_eq!(res.status(), 200);
    let after_ttl = get().await;
    assert_eq!(
        after_ttl["registrationEnabled"],
        json!(true),
        "an update without the field must not reset it"
    );
    assert_eq!(after_ttl["jwtTtlHours"], json!(6));

    let res = server
        .put_settings(
            &admin,
            json!({ "registrationEnabled": false, "maxPushSizeMb": 100 }),
        )
        .await;
    assert_eq!(res.status(), 200);
    let both = get().await;
    assert_eq!(both["registrationEnabled"], json!(false));
    assert_eq!(both["maxPushSizeMb"], json!(100));
    assert_eq!(both["jwtTtlHours"], json!(6));

    let res = server
        .put_settings(
            &admin,
            json!({ "registrationEnabled": true, "jwtTtlHours": 0 }),
        )
        .await;
    assert_eq!(res.status(), 400);
    assert_eq!(get().await["registrationEnabled"], json!(false));
}

#[sqlx::test]
async fn only_an_admin_can_read_or_change_the_switch(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;
    server.enable_registration(&admin).await;
    let body: Value = server
        .register("alice", "alice@example.com", PASSWORD)
        .await
        .json()
        .await
        .unwrap();
    let mfa_token = body["mfaToken"].as_str().unwrap();
    let secret = server
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
    let session = server
        .post(
            "/auth/mfa/setup/totp/confirm",
            json!({ "mfaToken": mfa_token, "code": totp_code(&secret) }),
        )
        .await
        .json::<Value>()
        .await
        .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();

    let res = server
        .put_settings(&session, json!({ "registrationEnabled": false }))
        .await;
    assert_eq!(res.status(), 401);
    let res = server
        .client
        .put(server.url("/admin/settings"))
        .json(&json!({ "registrationEnabled": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
    let res = server
        .client
        .get(server.url("/admin/settings"))
        .bearer_auth(&session)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
    assert_eq!(
        server.get_config().await["registrationEnabled"],
        json!(true),
        "the refused updates changed nothing"
    );
}
