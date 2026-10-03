// Free registration: the account is created inactive and its owner gets a link by e-mail to confirm the address and
// pick a password. Nobody is signed in by registering, and `mfa_enforced` stays true: the first sign-in goes through
// the mandatory MFA setup.

mod common;

use common::{ADMIN_PASSWORD, RecordingEmail, activation_link, token_of, totp_code};

use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;
use std::sync::Arc;

const PASSWORD: &str = "correct-horse-battery";
const SUBJECT: &str = "Confirmez votre inscription à FerrisGit";

struct Server {
    addr: SocketAddr,
    pool: PgPool,
    client: reqwest::Client,
    mailer: Arc<RecordingEmail>,
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

    async fn register(&self, username: &str, email: &str) -> reqwest::Response {
        self.post(
            "/auth/register",
            json!({ "username": username, "email": email }),
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

    async fn activate(&self, token: &str, password: &str) -> reqwest::Response {
        self.post(
            "/auth/activate",
            json!({ "token": token, "password": password }),
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

    async fn configure_smtp(&self, admin_session: &str) {
        let res = self
            .client
            .put(self.url("/admin/settings/smtp"))
            .bearer_auth(admin_session)
            .json(&json!({
                "host": "smtp.example.com",
                "port": 587,
                "security": "starttls",
                "fromAddress": "noreply@example.com",
                "fromName": "FerrisGit"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    }

    async fn switch_registration_on(&self, admin_session: &str) {
        let res = self
            .put_settings(admin_session, json!({ "registrationEnabled": true }))
            .await;
        assert_eq!(res.status(), 200);
    }

    /// Registration on, with mail configured: what an instance needs before anyone can sign up.
    async fn enable_registration(&self, admin_session: &str) {
        self.configure_smtp(admin_session).await;
        self.switch_registration_on(admin_session).await;
    }

    async fn user_count(&self) -> i64 {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM users")
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    /// The token of the most recent confirmation mail.
    fn latest_token(&self) -> String {
        let mails = self.mailer.attempted_with_subject(SUBJECT);
        token_of(&activation_link(
            &mails.last().expect("no confirmation mail").html,
        ))
    }
}

async fn spawn_server(pool: PgPool) -> Server {
    let started = common::spawn_server(pool).await;
    Server {
        addr: started.addr,
        pool: started.pool,
        client: started.client,
        mailer: started.mailer,
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
async fn the_sign_up_page_is_not_offered_while_mail_is_not_configured(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;

    server.switch_registration_on(&admin).await;
    assert_eq!(
        server.get_config().await["registrationEnabled"],
        json!(false),
        "the switch is on but nobody could receive a link"
    );

    server.configure_smtp(&admin).await;
    assert_eq!(
        server.get_config().await["registrationEnabled"],
        json!(true)
    );
}

#[sqlx::test]
async fn registering_without_configured_mail_is_a_503_and_creates_no_user(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;
    server.switch_registration_on(&admin).await;
    let before = server.user_count().await;

    let res = server.register("alice", "alice@example.com").await;

    assert_eq!(res.status(), 503);
    assert_eq!(
        res.json::<Value>().await.unwrap(),
        json!({ "error": "registration needs e-mail to be configured" })
    );
    assert_eq!(server.user_count().await, before);
    assert_eq!(server.mailer.attempts(), 0);
}

#[sqlx::test]
async fn registering_while_disabled_is_a_400_and_creates_no_user(pool: PgPool) {
    let server = spawn_server(pool).await;
    let before = server.user_count().await;

    let res = server.register("alice", "alice@example.com").await;

    assert_eq!(res.status(), 400);
    assert_eq!(server.user_count().await, before);
    assert_eq!(server.mailer.attempts(), 0);
}

/// With registration off the answer is the same whatever is submitted, so it can't be used to probe accounts (a 409
/// for a taken name or e-mail, a different 400 for invalid input).
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

    // Ten cases at most (the register throttle is 10 per IP), so a 429 can't hide a difference.
    for (username, email, why) in [
        ("alice", "fresh@example.com", "taken username"),
        ("ALICE", "fresh@example.com", "taken username, other casing"),
        ("bob", "alice@example.com", "taken e-mail"),
        ("bob", "ALICE@example.com", "taken e-mail, other casing"),
        ("ab", "bob@example.com", "invalid username"),
        ("login", "bob@example.com", "reserved username"),
        ("bob", "not-an-email", "invalid e-mail"),
        ("bob", "bob@example.com", "valid and free"),
    ] {
        let res = server.register(username, email).await;
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
async fn registering_creates_an_inactive_account_and_mails_the_activation_link(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;
    server.enable_registration(&admin).await;

    let res = server.register("Alice_1", "Alice@Example.com").await;

    assert_eq!(res.status(), 204);
    assert!(
        res.text().await.unwrap().is_empty(),
        "no session and no token in the response"
    );
    let mails = server.mailer.delivered();
    assert_eq!(mails.len(), 1);
    assert_eq!(mails[0].to, "Alice@Example.com");
    assert_eq!(mails[0].subject, SUBJECT);
    assert!(
        !activation_link(&mails[0].html).contains('?'),
        "the token is in the URL fragment"
    );
    assert!(mails[0].text.contains("alice_1"));

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
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM user_invitations i JOIN users u ON u.id = i.user_id WHERE u.username = 'alice_1'",
    )
    .fetch_one(&server.pool)
    .await
    .unwrap();
    assert_eq!(pending, 1, "the account waits for its activation");
}

#[sqlx::test]
async fn nobody_can_sign_in_to_a_registered_account_before_activating_it(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;
    server.enable_registration(&admin).await;
    assert_eq!(
        server.register("alice", "alice@example.com").await.status(),
        204
    );

    for password in [PASSWORD, "", "hashed:", "alice"] {
        assert_eq!(
            server.login("alice", password).await.status(),
            401,
            "password {password:?}"
        );
    }
}

#[sqlx::test]
async fn a_registered_user_activates_signs_in_and_completes_the_mfa_setup(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;
    server.enable_registration(&admin).await;
    assert_eq!(
        server
            .register("Alice_1", "alice@example.com")
            .await
            .status(),
        204
    );

    let activated = server.activate(&server.latest_token(), PASSWORD).await;
    assert_eq!(activated.status(), 204);

    for typed in ["Alice_1", "ALICE_1", "alice_1", "  Alice_1 "] {
        let res = server.login(typed, PASSWORD).await;
        assert_eq!(res.status(), 200, "signing in as {typed:?}");
        let body: Value = res.json().await.unwrap();
        assert!(body["token"].is_null());
        assert_eq!(body["mfaSetupRequired"], json!(true), "{body}");
    }
    assert_eq!(
        server.login("Alice_1", "not-the-password").await.status(),
        401
    );

    let body: Value = server
        .login("alice_1", PASSWORD)
        .await
        .json()
        .await
        .unwrap();
    let mfa_token = body["mfaToken"].as_str().unwrap();
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

    let me = server
        .client
        .get(server.url("/auth/me"))
        .bearer_auth(confirmed["token"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(me.status(), 200);
    let me: Value = me.json().await.unwrap();
    assert_eq!(me["username"], "alice_1");
    assert_eq!(me["email"], "alice@example.com");
    assert_eq!(me["isAdmin"], json!(false));
}

#[sqlx::test]
async fn an_activation_link_works_once(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;
    server.enable_registration(&admin).await;
    server.register("alice", "alice@example.com").await;
    let token = server.latest_token();

    assert_eq!(server.activate(&token, PASSWORD).await.status(), 204);

    assert_eq!(
        server.activate(&token, "another-password-1").await.status(),
        400
    );
    assert_eq!(
        server.login("alice", PASSWORD).await.status(),
        200,
        "the second attempt did not change the password"
    );
}

#[sqlx::test]
async fn registering_again_with_the_same_name_and_address_sends_a_new_link_and_kills_the_old_one(
    pool: PgPool,
) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;
    server.enable_registration(&admin).await;
    server.register("alice", "alice@example.com").await;
    let first = server.latest_token();

    let again = server.register("ALICE", "Alice@Example.com").await;

    assert_eq!(again.status(), 204, "a lost mail must not lock the name");
    assert_eq!(server.mailer.attempted_with_subject(SUBJECT).len(), 2);
    assert_eq!(server.user_count().await, 2, "the admin and one alice");
    let second = server.latest_token();
    assert_ne!(first, second);
    assert_eq!(
        server.activate(&first, PASSWORD).await.status(),
        400,
        "the previous link stops working"
    );
    assert_eq!(server.activate(&second, PASSWORD).await.status(), 204);
}

#[sqlx::test]
async fn a_name_or_address_that_belongs_to_someone_else_is_a_409(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;
    server.enable_registration(&admin).await;
    assert_eq!(
        server.register("alice", "alice@example.com").await.status(),
        204
    );
    let mails_before = server.mailer.attempts();

    for (username, email, status, why) in [
        (
            "alice",
            "other@example.com",
            409,
            "same username, new address",
        ),
        (
            "ALICE",
            "other@example.com",
            409,
            "same username, other casing",
        ),
        (
            "bob",
            "alice@example.com",
            409,
            "same address, new username",
        ),
        (
            "bob",
            "ALICE@Example.COM",
            409,
            "same address, other casing",
        ),
        ("Admin", "other@example.com", 400, "`admin` is reserved"),
    ] {
        assert_eq!(
            server.register(username, email).await.status(),
            status,
            "{why}"
        );
    }
    assert_eq!(server.user_count().await, 2, "only the admin and alice");
    assert_eq!(
        server.mailer.attempts(),
        mails_before,
        "refused registrations send nothing, so nobody can be spammed through them"
    );

    // Once alice has activated, her own name and address are a plain conflict, not a resend.
    server.activate(&server.latest_token(), PASSWORD).await;
    assert_eq!(
        server.register("alice", "alice@example.com").await.status(),
        409
    );
    assert_eq!(server.mailer.attempts(), mails_before);
}

#[sqlx::test]
async fn invalid_input_is_a_400_and_creates_nothing(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;
    server.enable_registration(&admin).await;
    let before = server.user_count().await;

    for (username, email, why) in [
        ("ab", "a@example.com", "username too short"),
        ("1alice", "a@example.com", "username starting with a digit"),
        ("ali ce", "a@example.com", "username with a space"),
        ("alice.git", "a@example.com", "username with a dot"),
        ("alice", "not-an-email", "invalid e-mail"),
        ("alice", "", "empty e-mail"),
        ("login", "a@example.com", "reserved name (SPA route)"),
        (
            "API",
            "a@example.com",
            "reserved name (infrastructure), any casing",
        ),
        (
            "register",
            "a@example.com",
            "reserved name (this very route)",
        ),
    ] {
        let res = server.register(username, email).await;
        assert_eq!(res.status(), 400, "{why}");
    }
    assert_eq!(server.user_count().await, before);
    assert_eq!(server.mailer.attempts(), 0);
}

#[sqlx::test]
async fn a_mail_that_cannot_be_sent_is_a_generic_503_and_the_account_can_be_retried(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;
    server.enable_registration(&admin).await;
    server.mailer.fail_with("550 relay refused for secret-host");

    let res = server.register("alice", "alice@example.com").await;

    assert_eq!(res.status(), 503);
    let text = res.text().await.unwrap();
    assert!(
        !text.contains("secret-host") && !text.contains("550"),
        "the SMTP error must not reach an anonymous caller: {text}"
    );
    assert!(text.contains("could not be sent"), "{text}");

    // The account was created before the send failed, so the same registration resends instead of conflicting.
    server.mailer.recover();
    assert_eq!(
        server.register("alice", "alice@example.com").await.status(),
        204
    );
    assert_eq!(server.mailer.delivered().len(), 1);
    assert_eq!(
        server
            .activate(&server.latest_token(), PASSWORD)
            .await
            .status(),
        204
    );
}

#[sqlx::test]
async fn registration_is_throttled_per_ip_on_the_eleventh_attempt(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;
    server.enable_registration(&admin).await;

    // Invalid attempts count too, the limit is per connection.
    for attempt in 1..=10 {
        assert_eq!(
            server.register("ab", "a@example.com").await.status(),
            400,
            "attempt {attempt}"
        );
    }
    assert_eq!(
        server.register("ab", "a@example.com").await.status(),
        429,
        "the 11th attempt is throttled"
    );
    assert_eq!(
        server.register("alice", "alice@example.com").await.status(),
        429,
        "a valid registration is throttled too"
    );
    assert_eq!(
        server.user_count().await,
        1,
        "nobody was created through the throttled attempts"
    );
    assert_eq!(server.mailer.attempts(), 0);
}

#[sqlx::test]
async fn the_register_and_activate_routes_refuse_a_body_over_16_kib(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.admin_session().await;
    server.enable_registration(&admin).await;
    let huge = "x".repeat(20 * 1024);

    let res = server.register("alice", &huge).await;
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
    assert_eq!(
        server.register("alice", "alice@example.com").await.status(),
        204
    );
    assert_eq!(
        server
            .activate(&server.latest_token(), PASSWORD)
            .await
            .status(),
        204
    );
    let body: Value = server.login("alice", PASSWORD).await.json().await.unwrap();
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
