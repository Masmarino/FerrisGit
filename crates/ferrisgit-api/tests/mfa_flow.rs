// Mandatory-MFA login: login only returns an `mfa-pending` token. The session comes from verify or the first-enrolment confirm.
// `mfa_enforced` stays true here, unlike the other flow files.

mod common;

use common::{
    ADMIN_PASSWORD, RecordingEmail, next_step_code, totp_code, wait_for_attempts, wrong_code,
};

use ferrisgit_api::state::AppState;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;
use std::sync::Arc;

struct Server {
    addr: SocketAddr,
    state: AppState,
    pool: PgPool,
    mailer: Arc<RecordingEmail>,
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

    async fn login(&self, username: &str, password: &str) -> reqwest::Response {
        self.post(
            "/auth/login",
            json!({ "username": username, "password": password }),
        )
        .await
    }

    async fn mfa_token(&self, username: &str, password: &str) -> String {
        let res = self.login(username, password).await;
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert!(
            body["token"].is_null(),
            "a local account must never get a session from the password step: {body}"
        );
        body["mfaToken"].as_str().expect("an mfaToken").to_string()
    }

    async fn enroll(&self, mfa_token: &str) -> reqwest::Response {
        self.post(
            "/auth/mfa/setup/totp/enroll",
            json!({ "mfaToken": mfa_token }),
        )
        .await
    }

    async fn confirm(&self, mfa_token: &str, code: &str) -> reqwest::Response {
        self.post(
            "/auth/mfa/setup/totp/confirm",
            json!({ "mfaToken": mfa_token, "code": code }),
        )
        .await
    }

    async fn verify_code(&self, mfa_token: &str, code: &str) -> reqwest::Response {
        self.post(
            "/auth/mfa/verify",
            json!({ "mfaToken": mfa_token, "code": code }),
        )
        .await
    }

    async fn verify_backup(&self, mfa_token: &str, backup_code: &str) -> reqwest::Response {
        self.post(
            "/auth/mfa/verify",
            json!({ "mfaToken": mfa_token, "backupCode": backup_code }),
        )
        .await
    }

    async fn me(&self, bearer: &str) -> reqwest::Response {
        self.client
            .get(self.url("/auth/me"))
            .bearer_auth(bearer)
            .send()
            .await
            .unwrap()
    }

    async fn enrolled(&self, username: &str, password: &str) -> Enrolled {
        let mfa_token = self.mfa_token(username, password).await;
        let secret = self.enroll(&mfa_token).await.json::<Value>().await.unwrap()["secret"]
            .as_str()
            .unwrap()
            .to_string();
        let enrolment_code = totp_code(&secret);
        let res = self.confirm(&mfa_token, &enrolment_code).await;
        assert_eq!(res.status(), 200, "the first login must complete");
        let body: Value = res.json().await.unwrap();
        Enrolled {
            secret,
            enrolment_code,
            session: body["token"].as_str().unwrap().to_string(),
            backup_codes: body["backupCodes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_str().unwrap().to_string())
                .collect(),
        }
    }

    async fn create_user(&self, admin_session: &str, username: &str, email: &str) {
        let res = self
            .client
            .post(self.url("/admin/users"))
            .bearer_auth(admin_session)
            .json(&json!({ "username": username, "email": email, "password": "password12345" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    }

    async fn last_used_steps(&self) -> Vec<Option<i64>> {
        sqlx::query_scalar::<_, Option<i64>>(
            "SELECT last_used_step FROM totp_credentials ORDER BY user_id",
        )
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }

    async fn used_backup_codes(&self) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mfa_backup_codes WHERE used_at IS NOT NULL",
        )
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    async fn security_events_of(&self, user_id: &str) -> Vec<String> {
        sqlx::query_scalar::<_, String>("SELECT event_type FROM domain_events WHERE aggregate_type = 'Security' AND aggregate_id = $1 ORDER BY version")
            .bind(user_id)
            .fetch_all(&self.pool)
            .await
            .unwrap()
    }
}

struct Enrolled {
    secret: String,
    enrolment_code: String,
    session: String,
    backup_codes: Vec<String>,
}

async fn spawn_server(pool: PgPool) -> Server {
    let started = common::spawn_server(pool).await;
    Server {
        addr: started.addr,
        state: started.state,
        pool: started.pool,
        mailer: started.mailer,
        client: started.client,
    }
}

#[sqlx::test]
async fn login_never_returns_a_session_for_a_local_account(pool: PgPool) {
    let server = spawn_server(pool).await;

    let res = server.login("admin", ADMIN_PASSWORD).await;

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(
        body["token"].is_null(),
        "no session from the password step: {body}"
    );
    assert!(
        body["mfaToken"].as_str().is_some_and(|t| !t.is_empty()),
        "an mfaToken is expected: {body}"
    );
    assert_eq!(body["mfaSetupRequired"], json!(true));
    assert_eq!(body["mfaHasTotp"], json!(false));
    assert_eq!(body["mfaHasPasskey"], json!(false));

    let wrong = server.login("admin", "not-the-password").await;
    assert_eq!(wrong.status(), 401);
    let wrong_body: Value = wrong.json().await.unwrap();
    assert!(
        wrong_body.get("mfaToken").is_none() && wrong_body.get("token").is_none(),
        "{wrong_body}"
    );
}

#[sqlx::test]
async fn login_is_still_rate_limited_per_ip_in_front_of_the_mfa_step(pool: PgPool) {
    let server = spawn_server(pool).await;

    let mut statuses = Vec::new();
    for _ in 0..15 {
        statuses.push(
            server
                .login("admin", "wrong-password")
                .await
                .status()
                .as_u16(),
        );
    }

    assert!(
        statuses.contains(&401) && statuses.contains(&429),
        "expected 401s then 429s, got {statuses:?}"
    );
    // The gate is the connection, not the outcome: the right password is refused too.
    assert_eq!(server.login("admin", ADMIN_PASSWORD).await.status(), 429);
}

#[sqlx::test]
async fn an_mfa_token_is_not_a_session(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;
    let mfa_token = server.mfa_token("admin", ADMIN_PASSWORD).await;

    assert_eq!(server.me(&mfa_token).await.status(), 401);
    assert_eq!(server.enroll(&admin.session).await.status(), 401);
    assert_eq!(server.confirm(&admin.session, "123456").await.status(), 401);
    assert_eq!(
        server.verify_code(&admin.session, "123456").await.status(),
        401
    );
    assert_eq!(server.enroll("not-a-token").await.status(), 401);
    assert_eq!(server.me(&admin.session).await.status(), 200);
}

#[sqlx::test]
async fn setup_enroll_returns_a_secret_and_an_otpauth_url(pool: PgPool) {
    let server = spawn_server(pool).await;
    let mfa_token = server.mfa_token("admin", ADMIN_PASSWORD).await;

    let res = server.enroll(&mfa_token).await;

    assert_eq!(res.status(), 200);
    let first: Value = res.json().await.unwrap();
    let secret = first["secret"].as_str().unwrap();
    assert_eq!(secret.len(), 32, "a 20-byte secret in base32");
    let url = first["otpauthUrl"].as_str().unwrap();
    assert!(
        url.starts_with("otpauth://totp/")
            && url.contains("issuer=FerrisGit")
            && url.contains("admin"),
        "{url}"
    );

    let second: Value = server.enroll(&mfa_token).await.json().await.unwrap();
    assert_ne!(
        second["secret"].as_str().unwrap(),
        secret,
        "a second enrolment must issue a NEW secret"
    );
    assert_eq!(
        server
            .confirm(&mfa_token, &totp_code(secret))
            .await
            .status(),
        400
    );
    assert_eq!(
        server
            .confirm(&mfa_token, &totp_code(second["secret"].as_str().unwrap()))
            .await
            .status(),
        200
    );
}

#[sqlx::test]
async fn setup_confirm_completes_the_first_login(pool: PgPool) {
    let server = spawn_server(pool).await;
    let mfa_token = server.mfa_token("admin", ADMIN_PASSWORD).await;
    let secret = server
        .enroll(&mfa_token)
        .await
        .json::<Value>()
        .await
        .unwrap()["secret"]
        .as_str()
        .unwrap()
        .to_string();

    let res = server.confirm(&mfa_token, &totp_code(&secret)).await;

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let codes = body["backupCodes"].as_array().unwrap();
    assert_eq!(codes.len(), 10);
    for code in codes {
        let code = code.as_str().unwrap();
        assert!(
            code.len() == 32 && code.chars().all(|c| c.is_ascii_hexdigit()),
            "{code}"
        );
    }
    let session = body["token"].as_str().unwrap();
    let me = server.me(session).await;
    assert_eq!(me.status(), 200);
    assert_eq!(
        me.json::<Value>().await.unwrap()["username"],
        json!("admin")
    );

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert_eq!(
        server.mailer.attempts(),
        0,
        "no enrolment mail may be attempted for an invalid mailbox"
    );
}

#[sqlx::test]
async fn confirm_sends_the_enrolled_mail_when_the_address_is_a_valid_mailbox(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;
    // `PATCH /auth/me` needs a session a not-yet-enrolled user cannot have, hence the direct repository call.
    server
        .create_user(&admin.session, "dave", "dave@localhost")
        .await;
    let dave_id = server
        .state
        .users
        .find_by_username("dave")
        .await
        .unwrap()
        .unwrap()
        .id;
    server
        .state
        .users
        .update_email(dave_id, "x@example.com".to_string())
        .await
        .unwrap();
    let token = server.mfa_token("dave", "password12345").await;
    let secret = server.enroll(&token).await.json::<Value>().await.unwrap()["secret"]
        .as_str()
        .unwrap()
        .to_string();

    let res = server.confirm(&token, &totp_code(&secret)).await;

    assert_eq!(res.status(), 200);
    wait_for_attempts(&server.mailer, 1).await;
    let sent = server.mailer.sent();
    assert_eq!(sent.len(), 1, "exactly one enrolment mail: {sent:?}");
    assert_eq!(sent[0].0, "x@example.com");
    assert!(!sent[0].1.is_empty());
}

#[sqlx::test]
async fn a_pending_enrolment_cannot_be_used_at_the_challenge(pool: PgPool) {
    let server = spawn_server(pool).await;
    let mfa_token = server.mfa_token("admin", ADMIN_PASSWORD).await;
    let secret = server
        .enroll(&mfa_token)
        .await
        .json::<Value>()
        .await
        .unwrap()["secret"]
        .as_str()
        .unwrap()
        .to_string();

    let res = server.verify_code(&mfa_token, &totp_code(&secret)).await;
    assert_eq!(res.status(), 401);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["error"], json!("invalid code"));
    assert!(body.get("token").is_none(), "{body}");
    let confirmed: Vec<bool> = sqlx::query_scalar("SELECT confirmed FROM totp_credentials")
        .fetch_all(&server.pool)
        .await
        .unwrap();
    assert_eq!(
        confirmed,
        vec![false],
        "a verify must not confirm the enrolment either"
    );
    // Backup codes are only a recovery path for a confirmed factor.
    assert_eq!(
        server
            .verify_backup(&mfa_token, "0123456789abcdef0123456789abcdef")
            .await
            .status(),
        401
    );
    let login: Value = server
        .login("admin", ADMIN_PASSWORD)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(login["mfaSetupRequired"], json!(true));
}

#[sqlx::test]
async fn confirm_with_a_wrong_code_is_refused_and_keeps_the_token_usable(pool: PgPool) {
    let server = spawn_server(pool).await;
    let mfa_token = server.mfa_token("admin", ADMIN_PASSWORD).await;
    let secret = server
        .enroll(&mfa_token)
        .await
        .json::<Value>()
        .await
        .unwrap()["secret"]
        .as_str()
        .unwrap()
        .to_string();

    let wrong = server.confirm(&mfa_token, &wrong_code(&secret)).await;
    assert_eq!(wrong.status(), 400);
    let wrong_body: Value = wrong.json().await.unwrap();
    assert!(
        wrong_body.get("token").is_none() && wrong_body.get("backupCodes").is_none(),
        "{wrong_body}"
    );

    let right = server.confirm(&mfa_token, &totp_code(&secret)).await;
    assert_eq!(right.status(), 200);
    assert!(right.json::<Value>().await.unwrap()["token"].is_string());
}

#[sqlx::test]
async fn the_mfa_token_is_single_use_after_success(pool: PgPool) {
    let server = spawn_server(pool).await;
    let mfa_token = server.mfa_token("admin", ADMIN_PASSWORD).await;
    let secret = server
        .enroll(&mfa_token)
        .await
        .json::<Value>()
        .await
        .unwrap()["secret"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        server
            .confirm(&mfa_token, &totp_code(&secret))
            .await
            .status(),
        200
    );

    assert_eq!(
        server
            .verify_code(&mfa_token, &next_step_code(&secret))
            .await
            .status(),
        401
    );
    assert_eq!(server.enroll(&mfa_token).await.status(), 401);
    assert_eq!(
        server
            .confirm(&mfa_token, &totp_code(&secret))
            .await
            .status(),
        401
    );

    // The refusal of a spent token did not burn the legitimate next code: a fresh login can use it.
    let fresh = server.mfa_token("admin", ADMIN_PASSWORD).await;
    assert_eq!(
        server
            .verify_code(&fresh, &next_step_code(&secret))
            .await
            .status(),
        200
    );
}

#[sqlx::test]
async fn next_login_requires_a_challenge(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;

    let login: Value = server
        .login("admin", ADMIN_PASSWORD)
        .await
        .json()
        .await
        .unwrap();
    assert!(login["token"].is_null());
    assert_eq!(login["mfaSetupRequired"], json!(false));
    assert_eq!(login["mfaHasTotp"], json!(true));
    assert_eq!(login["mfaHasPasskey"], json!(false));
    let mfa_token = login["mfaToken"].as_str().unwrap();

    assert_eq!(
        server
            .verify_code(mfa_token, &wrong_code(&admin.secret))
            .await
            .status(),
        401
    );

    assert_eq!(
        server
            .verify_code(mfa_token, &admin.enrolment_code)
            .await
            .status(),
        401
    );

    let code = next_step_code(&admin.secret);
    let ok = server.verify_code(mfa_token, &code).await;
    assert_eq!(ok.status(), 200);
    let session = ok.json::<Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(server.me(&session).await.status(), 200);

    let second = server.mfa_token("admin", ADMIN_PASSWORD).await;
    assert_eq!(server.verify_code(&second, &code).await.status(), 401);

    let third = server.mfa_token("admin", ADMIN_PASSWORD).await;
    let enroll = server.enroll(&third).await;
    assert_eq!(enroll.status(), 400);
    assert!(
        enroll.json::<Value>().await.unwrap()["error"]
            .as_str()
            .unwrap()
            .contains("already enrolled")
    );
    assert_eq!(
        server
            .confirm(&third, &next_step_code(&admin.secret))
            .await
            .status(),
        400
    );
}

#[sqlx::test]
async fn a_backup_code_logs_in_once(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;
    let backup = admin.backup_codes[0].clone();

    let first = server.mfa_token("admin", ADMIN_PASSWORD).await;
    let ok = server.verify_backup(&first, &backup).await;
    assert_eq!(ok.status(), 200);
    assert!(ok.json::<Value>().await.unwrap()["token"].is_string());

    let second = server.mfa_token("admin", ADMIN_PASSWORD).await;
    assert_eq!(
        server.verify_backup(&second, &backup).await.status(),
        401,
        "a backup code is single use"
    );
    assert_eq!(
        server
            .verify_backup(&second, &admin.backup_codes[1])
            .await
            .status(),
        200
    );

    let third = server.mfa_token("admin", ADMIN_PASSWORD).await;
    let both = server.post("/auth/mfa/verify", json!({ "mfaToken": third, "code": next_step_code(&admin.secret), "backupCode": admin.backup_codes[2] })).await;
    assert_eq!(both.status(), 401);
    let neither = server
        .post("/auth/mfa/verify", json!({ "mfaToken": third }))
        .await;
    assert_eq!(neither.status(), 401);
    assert_eq!(
        server
            .verify_backup(&third, &admin.backup_codes[2])
            .await
            .status(),
        200
    );
}

#[sqlx::test]
async fn mfa_attempts_are_rate_limited_per_user(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;
    server
        .create_user(&admin.session, "bob", "bob@example.com")
        .await;
    let mfa_token = server.mfa_token("admin", ADMIN_PASSWORD).await;
    let step_before = server.last_used_steps().await;
    assert_eq!(step_before.len(), 1);
    assert!(step_before[0].is_some(), "the enrolment claimed a step");
    assert_eq!(server.used_backup_codes().await, 0);

    let mut refused = 0;
    let mut first_429_after = None;
    for _ in 0..12 {
        match server
            .verify_code(&mfa_token, &wrong_code(&admin.secret))
            .await
            .status()
            .as_u16()
        {
            401 => refused += 1,
            429 => {
                first_429_after = Some(refused);
                break;
            }
            other => panic!("unexpected status {other}"),
        }
    }
    assert_eq!(
        first_429_after,
        Some(8),
        "10 attempts per user: 2 used by the enrolment, 8 wrong codes, then 429"
    );

    assert_eq!(
        server
            .verify_code(&mfa_token, &next_step_code(&admin.secret))
            .await
            .status(),
        429
    );
    assert_eq!(
        server
            .verify_backup(&mfa_token, &admin.backup_codes[0])
            .await
            .status(),
        429
    );
    // A refused attempt is refused before the factor is even looked at. The right code that was refused did not
    // burn its TOTP step, and the refused backup code was not consumed.
    assert_eq!(
        server.last_used_steps().await,
        step_before,
        "a 429 must not advance last_used_step"
    );
    assert_eq!(
        server.used_backup_codes().await,
        0,
        "a 429 must not consume a backup code"
    );

    let bob = server.enrolled("bob", "password12345").await;
    assert_eq!(server.me(&bob.session).await.status(), 200);
}

#[sqlx::test]
async fn a_password_change_invalidates_a_pending_mfa_token(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;
    server
        .create_user(&admin.session, "bob", "bob@example.com")
        .await;
    let bobs_pending = server.mfa_token("bob", "password12345").await;
    let admins_pending = server.mfa_token("admin", ADMIN_PASSWORD).await;

    let changed = server
        .client
        .post(server.url("/auth/me/password"))
        .bearer_auth(&admin.session)
        .json(&json!({ "currentPassword": ADMIN_PASSWORD, "newPassword": "a-brand-new-password" }))
        .send()
        .await
        .unwrap();
    assert_eq!(changed.status(), 200);

    assert_eq!(
        server
            .verify_code(&admins_pending, &next_step_code(&admin.secret))
            .await
            .status(),
        401,
        "the pending token predates the password change"
    );
    assert_eq!(
        server
            .verify_backup(&admins_pending, &admin.backup_codes[0])
            .await
            .status(),
        401
    );

    // Same for the enrolment path: bumping the epoch of a not-yet-enrolled user kills their pending token.
    let bob_id = server
        .state
        .users
        .find_by_username("bob")
        .await
        .unwrap()
        .unwrap()
        .id;
    server.state.users.bump_token_epoch(bob_id).await.unwrap();
    assert_eq!(server.enroll(&bobs_pending).await.status(), 401);

    let fresh = server.mfa_token("admin", "a-brand-new-password").await;
    assert_eq!(
        server
            .verify_backup(&fresh, &admin.backup_codes[0])
            .await
            .status(),
        200
    );
}

#[sqlx::test]
async fn login_response_never_contains_secrets(pool: PgPool) {
    let server = spawn_server(pool).await;
    let before = server
        .login("admin", ADMIN_PASSWORD)
        .await
        .text()
        .await
        .unwrap();
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;
    let after = server
        .login("admin", ADMIN_PASSWORD)
        .await
        .text()
        .await
        .unwrap();

    for body in [before, after] {
        let lowered = body.to_lowercase();
        for forbidden in [
            "secret",
            "hash",
            "otpauth",
            "password",
            &admin.secret.to_lowercase(),
            &admin.backup_codes[0],
        ] {
            assert!(
                !lowered.contains(forbidden),
                "the login body must not contain {forbidden:?}: {body}"
            );
        }
    }
}

#[sqlx::test]
async fn failed_challenges_and_completed_logins_leave_security_events(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;
    let admin_id = server
        .state
        .users
        .find_by_username("admin")
        .await
        .unwrap()
        .unwrap()
        .id
        .to_string();
    // The enrolment already published MfaEnrolled and LoginSucceeded.
    let events = server.security_events_of(&admin_id).await;
    assert!(
        events.contains(&"MfaEnrolled".to_string())
            && events.contains(&"LoginSucceeded".to_string()),
        "{events:?}"
    );
    assert!(!events.contains(&"MfaVerificationFailed".to_string()));

    let mfa_token = server.mfa_token("admin", ADMIN_PASSWORD).await;
    assert_eq!(
        server
            .security_events_of(&admin_id)
            .await
            .iter()
            .filter(|e| *e == "LoginSucceeded")
            .count(),
        1
    );
    assert_eq!(
        server
            .verify_code(&mfa_token, &wrong_code(&admin.secret))
            .await
            .status(),
        401
    );
    assert!(
        server
            .security_events_of(&admin_id)
            .await
            .contains(&"MfaVerificationFailed".to_string())
    );
    assert_eq!(
        server
            .verify_code(&mfa_token, &next_step_code(&admin.secret))
            .await
            .status(),
        200
    );
    assert_eq!(
        server
            .security_events_of(&admin_id)
            .await
            .iter()
            .filter(|e| *e == "LoginSucceeded")
            .count(),
        2
    );
}

#[sqlx::test]
async fn login_fails_closed_when_the_stored_totp_secret_cannot_be_read(pool: PgPool) {
    let server = spawn_server(pool.clone()).await;
    server.enrolled("admin", ADMIN_PASSWORD).await;
    // As after an encryption-key rotation: the row exists but its secret can no longer be decrypted.
    sqlx::query("UPDATE totp_credentials SET encrypted_secret = $1")
        .bind(vec![0u8; 4])
        .execute(&pool)
        .await
        .unwrap();

    let res = server.login("admin", ADMIN_PASSWORD).await;

    // A 5xx, never "no factor, please enrol" (which would let the user enrol around the existing one).
    assert_eq!(res.status(), 500);
    let body = res.text().await.unwrap();
    assert!(!body.contains("mfaToken"), "{body}");
}

#[sqlx::test]
async fn the_unauthenticated_mfa_endpoints_have_a_small_body_limit(pool: PgPool) {
    let server = spawn_server(pool).await;
    let huge = "a".repeat(64 * 1024);

    let res = server
        .post(
            "/auth/mfa/verify",
            json!({ "mfaToken": huge, "code": "123456" }),
        )
        .await;

    assert_eq!(res.status(), 413);
}
