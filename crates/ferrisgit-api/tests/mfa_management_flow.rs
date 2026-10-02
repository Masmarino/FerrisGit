// Self-service MFA management (`/api/me/mfa/...`) and the admin reset. MFA stays enforced.

mod common;

use common::{
    ADMIN_PASSWORD, RecordingEmail, USER_PASSWORD, WRONG_PASSWORD, codes_of, now_unix,
    settled_attempts, totp_code, wait_for_attempts, wrong_code,
};

use ferrisgit_api::state::AppState;
use ferrisgit_application::email_templates;
use ferrisgit_application::mfa_crypto::generate_code_at;
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

struct Enrolled {
    id: String,
    session: String,
    secret: String,
    backup_codes: Vec<String>,
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

    async fn post_as(&self, session: &str, path: &str, body: Value) -> reqwest::Response {
        self.client
            .post(self.url(path))
            .bearer_auth(session)
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    async fn get_as(&self, session: &str, path: &str) -> reqwest::Response {
        self.client
            .get(self.url(path))
            .bearer_auth(session)
            .send()
            .await
            .unwrap()
    }

    async fn login(&self, username: &str, password: &str) -> Value {
        let res = self
            .post(
                "/auth/login",
                json!({ "username": username, "password": password }),
            )
            .await;
        assert_eq!(res.status(), 200);
        res.json().await.unwrap()
    }

    async fn mfa_token(&self, username: &str, password: &str) -> String {
        self.login(username, password).await["mfaToken"]
            .as_str()
            .expect("an mfaToken")
            .to_string()
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
        self.get_as(bearer, "/auth/me").await
    }

    async fn enrolled(&self, username: &str, password: &str) -> Enrolled {
        let mfa_token = self.mfa_token(username, password).await;
        let enrol: Value = self
            .post(
                "/auth/mfa/setup/totp/enroll",
                json!({ "mfaToken": mfa_token }),
            )
            .await
            .json()
            .await
            .unwrap();
        let secret = enrol["secret"].as_str().unwrap().to_string();
        let res = self
            .post(
                "/auth/mfa/setup/totp/confirm",
                json!({ "mfaToken": mfa_token, "code": totp_code(&secret) }),
            )
            .await;
        assert_eq!(res.status(), 200, "the first login must complete");
        let body: Value = res.json().await.unwrap();
        Enrolled {
            id: self
                .state
                .users
                .find_by_username(username)
                .await
                .unwrap()
                .unwrap()
                .id
                .to_string(),
            session: body["token"].as_str().unwrap().to_string(),
            secret,
            backup_codes: codes_of(&body),
        }
    }

    async fn create_user(&self, admin_session: &str, username: &str, email: &str) {
        let res = self
            .client
            .post(self.url("/admin/users"))
            .bearer_auth(admin_session)
            .json(&json!({ "username": username, "email": email, "password": USER_PASSWORD }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    }

    async fn set_email(&self, username: &str, email: &str) {
        let id = self
            .state
            .users
            .find_by_username(username)
            .await
            .unwrap()
            .unwrap()
            .id;
        self.state
            .users
            .update_email(id, email.to_string())
            .await
            .unwrap();
    }

    async fn status(&self, session: &str) -> Value {
        let res = self.get_as(session, "/me/mfa").await;
        assert_eq!(res.status(), 200);
        res.json().await.unwrap()
    }

    async fn enroll_self(&self, session: &str, password: &str) -> reqwest::Response {
        self.post_as(
            session,
            "/me/mfa/totp/enroll",
            json!({ "currentPassword": password }),
        )
        .await
    }

    async fn confirm_self(&self, session: &str, code: &str) -> reqwest::Response {
        self.post_as(session, "/me/mfa/totp/confirm", json!({ "code": code }))
            .await
    }

    async fn regenerate(&self, session: &str, password: &str) -> reqwest::Response {
        self.post_as(
            session,
            "/me/mfa/backup-codes/regenerate",
            json!({ "currentPassword": password }),
        )
        .await
    }

    async fn disable(&self, session: &str, password: &str) -> reqwest::Response {
        self.post_as(
            session,
            "/me/mfa/totp/disable",
            json!({ "currentPassword": password }),
        )
        .await
    }

    /// Forged by removing the factor without the epoch bump, since the API can no longer produce a session without one. This keeps the account-level enrol and confirm endpoints tested as defence in depth.
    async fn without_factor(&self, user: &Enrolled) -> String {
        self.state
            .mfa
            .reset(user.id.parse().unwrap())
            .await
            .unwrap();
        user.session.clone()
    }

    async fn reenrol_from_the_account(
        &self,
        session: &str,
        password: &str,
    ) -> (String, Vec<String>) {
        let enrol = self.enroll_self(session, password).await;
        assert_eq!(enrol.status(), 200);
        let secret = enrol.json::<Value>().await.unwrap()["secret"]
            .as_str()
            .unwrap()
            .to_string();
        let confirm = self.confirm_self(session, &totp_code(&secret)).await;
        assert_eq!(confirm.status(), 200);
        (secret, codes_of(&confirm.json::<Value>().await.unwrap()))
    }

    async fn admin_reset(&self, session: &str, user_id: &str) -> reqwest::Response {
        self.client
            .delete(self.url(&format!("/admin/users/{user_id}/mfa")))
            .bearer_auth(session)
            .send()
            .await
            .unwrap()
    }

    async fn credential_rows(&self) -> i64 {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM totp_credentials")
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn backup_code_rows(&self, user_id: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mfa_backup_codes WHERE user_id = $1::uuid",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    async fn backup_code_hashes(&self, user_id: &str) -> Vec<String> {
        sqlx::query_scalar::<_, String>(
            "SELECT code_hash FROM mfa_backup_codes WHERE user_id = $1::uuid ORDER BY code_hash",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
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

    async fn actor_of(&self, user_id: &str, event_type: &str) -> Option<String> {
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT actor_id::text FROM domain_events WHERE aggregate_type = 'Security' AND aggregate_id = $1 AND event_type = $2 ORDER BY version DESC LIMIT 1",
        )
        .bind(user_id)
        .bind(event_type)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }
}

fn keys(value: &Value) -> Vec<String> {
    value
        .as_object()
        .expect("an object")
        .keys()
        .cloned()
        .collect()
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
async fn status_reflects_enrolment_and_code_usage(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;

    assert_eq!(
        server.status(&admin.session).await,
        json!({ "totpEnabled": true, "backupCodesRemaining": 10, "passkeys": [] })
    );

    let session = server.without_factor(&admin).await;
    assert_eq!(
        server.status(&session).await,
        json!({ "totpEnabled": false, "backupCodesRemaining": 0, "passkeys": [] })
    );

    let (_secret, codes) = server
        .reenrol_from_the_account(&session, ADMIN_PASSWORD)
        .await;
    assert_eq!(
        server.status(&session).await,
        json!({ "totpEnabled": true, "backupCodesRemaining": 10, "passkeys": [] })
    );
    let mfa_token = server.mfa_token("admin", ADMIN_PASSWORD).await;
    let login = server.verify_backup(&mfa_token, &codes[0]).await;
    assert_eq!(login.status(), 200);
    let fresh = login.json::<Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        server.status(&fresh).await,
        json!({ "totpEnabled": true, "backupCodesRemaining": 9, "passkeys": [] })
    );
}

#[sqlx::test]
async fn every_self_service_endpoint_needs_a_session(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;

    assert_eq!(
        server
            .client
            .get(server.url("/me/mfa"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    for path in [
        "/me/mfa/totp/enroll",
        "/me/mfa/totp/disable",
        "/me/mfa/backup-codes/regenerate",
    ] {
        let res = server
            .client
            .post(server.url(path))
            .json(&json!({ "currentPassword": ADMIN_PASSWORD }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 401, "{path}");
    }
    let res = server
        .client
        .post(server.url("/me/mfa/totp/confirm"))
        .json(&json!({ "code": "123456" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
    let mfa_token = server.mfa_token("admin", ADMIN_PASSWORD).await;
    assert_eq!(server.get_as(&mfa_token, "/me/mfa").await.status(), 401);
    assert_eq!(
        server.regenerate(&mfa_token, ADMIN_PASSWORD).await.status(),
        401
    );
    assert_eq!(
        server.status(&admin.session).await["backupCodesRemaining"],
        json!(10)
    );
}

#[sqlx::test]
async fn enroll_requires_the_current_password_and_is_refused_when_enrolled(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;

    let refused = server.enroll_self(&admin.session, ADMIN_PASSWORD).await;
    assert_eq!(refused.status(), 400);
    assert!(
        refused.json::<Value>().await.unwrap()["error"]
            .as_str()
            .unwrap()
            .contains("already enrolled")
    );
    let mfa_token = server.mfa_token("admin", ADMIN_PASSWORD).await;
    let step_after = generate_code_at(&admin.secret, now_unix() + 30);
    assert_eq!(
        server.verify_code(&mfa_token, &step_after).await.status(),
        200,
        "the original secret is untouched"
    );

    let session = server.without_factor(&admin).await;
    assert_eq!(
        server.enroll_self(&session, WRONG_PASSWORD).await.status(),
        400
    );
    assert_eq!(
        server.credential_rows().await,
        0,
        "a refused enrolment must not leave a credential behind"
    );

    let ok = server.enroll_self(&session, ADMIN_PASSWORD).await;
    assert_eq!(ok.status(), 200);
    let body: Value = ok.json().await.unwrap();
    assert_eq!(body["secret"].as_str().unwrap().len(), 32);
    let url = body["otpauthUrl"].as_str().unwrap();
    assert!(
        url.starts_with("otpauth://totp/")
            && url.contains("issuer=FerrisGit")
            && url.contains("admin"),
        "{url}"
    );
    assert_eq!(
        server.status(&session).await,
        json!({ "totpEnabled": false, "backupCodesRemaining": 0, "passkeys": [] })
    );
}

#[sqlx::test]
async fn confirm_with_a_wrong_code_is_400(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;
    let session = server.without_factor(&admin).await;
    let secret = server
        .enroll_self(&session, ADMIN_PASSWORD)
        .await
        .json::<Value>()
        .await
        .unwrap()["secret"]
        .as_str()
        .unwrap()
        .to_string();

    let wrong = server.confirm_self(&session, &wrong_code(&secret)).await;

    assert_eq!(wrong.status(), 400);
    let body: Value = wrong.json().await.unwrap();
    assert!(body.get("backupCodes").is_none(), "{body}");
    assert_eq!(server.status(&session).await["totpEnabled"], json!(false));
    let right = server.confirm_self(&session, &totp_code(&secret)).await;
    assert_eq!(right.status(), 200);
    let codes = codes_of(&right.json::<Value>().await.unwrap());
    assert_eq!(codes.len(), 10);
    for code in &codes {
        assert!(
            code.len() == 32 && code.chars().all(|c| c.is_ascii_hexdigit()),
            "{code}"
        );
    }
    assert_eq!(
        server.status(&session).await,
        json!({ "totpEnabled": true, "backupCodesRemaining": 10, "passkeys": [] })
    );
}

#[sqlx::test]
async fn confirm_without_an_enrolment_is_400(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;
    let session = server.without_factor(&admin).await;

    assert_eq!(server.confirm_self(&session, "123456").await.status(), 400);
}

#[sqlx::test]
async fn enroll_confirm_from_the_account_replaces_nothing_when_enrolled(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;

    assert_eq!(
        server
            .enroll_self(&admin.session, ADMIN_PASSWORD)
            .await
            .status(),
        400
    );
    // A confirm with a code of the live secret must not re-confirm nor hand out a second set of codes.
    let confirm = server
        .confirm_self(
            &admin.session,
            &generate_code_at(&admin.secret, now_unix() + 30),
        )
        .await;
    assert_eq!(confirm.status(), 400);
    assert!(
        confirm
            .json::<Value>()
            .await
            .unwrap()
            .get("backupCodes")
            .is_none()
    );

    assert_eq!(
        server.status(&admin.session).await,
        json!({ "totpEnabled": true, "backupCodesRemaining": 10, "passkeys": [] })
    );
    let mfa_token = server.mfa_token("admin", ADMIN_PASSWORD).await;
    assert_eq!(
        server
            .verify_backup(&mfa_token, &admin.backup_codes[0])
            .await
            .status(),
        200
    );
}

#[sqlx::test]
async fn confirming_from_the_account_publishes_the_event_and_mails_a_valid_address(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;
    server
        .create_user(&admin.session, "dave", "dave@localhost")
        .await;
    server.set_email("dave", "x@example.com").await;
    let dave = server.enrolled("dave", USER_PASSWORD).await;
    wait_for_attempts(&server.mailer, 1).await; // the first-login enrolment mail

    let session = server.without_factor(&dave).await;
    server
        .reenrol_from_the_account(&session, USER_PASSWORD)
        .await;

    wait_for_attempts(&server.mailer, 2).await;
    let sent = server.mailer.sent();
    assert_eq!(sent.len(), 2, "{sent:?}");
    assert_eq!(sent[1].0, "x@example.com");
    assert_eq!(
        sent[1].1,
        email_templates::mfa_enrolled("dave", "une application d'authentification (TOTP)").subject
    );
    let events = server.security_events_of(&dave.id).await;
    assert_eq!(
        events.iter().filter(|e| *e == "MfaEnrolled").count(),
        2,
        "{events:?}"
    );
    assert_eq!(
        server.actor_of(&dave.id, "MfaEnrolled").await,
        Some(dave.id.clone())
    );
}

#[sqlx::test]
async fn confirming_from_the_account_sends_nothing_to_an_undeliverable_address(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await; // `admin@localhost`
    let session = server.without_factor(&admin).await;

    server
        .reenrol_from_the_account(&session, ADMIN_PASSWORD)
        .await;

    assert_eq!(settled_attempts(&server.mailer).await, 0);
}

#[sqlx::test]
async fn regenerate_replaces_all_codes(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;

    let res = server.regenerate(&admin.session, ADMIN_PASSWORD).await;

    assert_eq!(res.status(), 200);
    let fresh = codes_of(&res.json::<Value>().await.unwrap());
    assert_eq!(fresh.len(), 10);
    assert!(
        fresh.iter().all(|c| !admin.backup_codes.contains(c)),
        "the new codes must all differ from the old"
    );
    assert_eq!(
        server.status(&admin.session).await["backupCodesRemaining"],
        json!(10)
    );
    assert_eq!(
        server.backup_code_rows(&admin.id).await,
        10,
        "the old rows are replaced, not accumulated"
    );

    let mfa_token = server.mfa_token("admin", ADMIN_PASSWORD).await;
    assert_eq!(
        server
            .verify_backup(&mfa_token, &admin.backup_codes[0])
            .await
            .status(),
        401,
        "an old code is dead"
    );
    assert_eq!(
        server.verify_backup(&mfa_token, &fresh[0]).await.status(),
        200,
        "a new code works"
    );
    assert_eq!(server.me(&admin.session).await.status(), 200);
}

#[sqlx::test]
async fn regenerate_needs_password_and_a_factor(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;

    let wrong = server.regenerate(&admin.session, WRONG_PASSWORD).await;
    assert_eq!(wrong.status(), 400);
    assert!(
        wrong
            .json::<Value>()
            .await
            .unwrap()
            .get("backupCodes")
            .is_none()
    );
    let mfa_token = server.mfa_token("admin", ADMIN_PASSWORD).await;
    assert_eq!(
        server
            .verify_backup(&mfa_token, &admin.backup_codes[0])
            .await
            .status(),
        200
    );

    let session = server.without_factor(&admin).await;
    assert_eq!(
        server.regenerate(&session, ADMIN_PASSWORD).await.status(),
        400
    );
    assert_eq!(server.backup_code_rows(&admin.id).await, 0);
}

#[sqlx::test]
async fn disable_needs_password(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;

    let res = server.disable(&admin.session, WRONG_PASSWORD).await;

    assert_eq!(res.status(), 400);
    assert!(res.json::<Value>().await.unwrap().get("token").is_none());
    assert_eq!(
        server.status(&admin.session).await,
        json!({ "totpEnabled": true, "backupCodesRemaining": 10, "passkeys": [] })
    );
    assert_eq!(
        server.me(&admin.session).await.status(),
        200,
        "a refused disable must not revoke the session"
    );
    assert_eq!(server.credential_rows().await, 1);
}

#[sqlx::test]
async fn disable_signs_the_user_out_and_removes_the_factor(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;
    let other_login = server.mfa_token("admin", ADMIN_PASSWORD).await;
    let other = server
        .verify_code(
            &other_login,
            &generate_code_at(&admin.secret, now_unix() + 30),
        )
        .await;
    assert_eq!(other.status(), 200);
    let other_session = other.json::<Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(server.me(&other_session).await.status(), 200);

    let res = server.disable(&admin.session, ADMIN_PASSWORD).await;

    // MFA is mandatory: no session may outlive its factor.
    assert_eq!(res.status(), 204);
    assert!(
        res.bytes().await.unwrap().is_empty(),
        "204 has an empty body"
    );
    assert_eq!(
        server.me(&admin.session).await.status(),
        401,
        "the calling session dies with the factor"
    );
    assert_eq!(
        server.me(&other_session).await.status(),
        401,
        "and so does every other one"
    );
    assert_eq!(server.credential_rows().await, 0);
    assert_eq!(server.backup_code_rows(&admin.id).await, 0);
    let events = server.security_events_of(&admin.id).await;
    assert!(events.contains(&"MfaDisabled".to_string()), "{events:?}");
    assert_eq!(
        server.actor_of(&admin.id, "MfaDisabled").await,
        Some(admin.id.clone())
    );

    let login = server.login("admin", ADMIN_PASSWORD).await;
    assert_eq!(login["mfaSetupRequired"], json!(true));
    assert_eq!(login["mfaHasTotp"], json!(false));
    assert!(login["token"].is_null());
    let mfa_token = login["mfaToken"].as_str().unwrap();
    assert_eq!(
        server
            .verify_code(mfa_token, &generate_code_at(&admin.secret, now_unix() + 30))
            .await
            .status(),
        401
    );
    assert_eq!(
        server
            .verify_backup(mfa_token, &admin.backup_codes[0])
            .await
            .status(),
        401
    );
}

#[sqlx::test]
async fn disable_kills_the_pending_tokens_issued_before_it(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;
    let old_pending = server.mfa_token("admin", ADMIN_PASSWORD).await;

    assert_eq!(
        server
            .disable(&admin.session, ADMIN_PASSWORD)
            .await
            .status(),
        204
    );

    assert_eq!(
        server
            .verify_code(
                &old_pending,
                &generate_code_at(&admin.secret, now_unix() + 30)
            )
            .await
            .status(),
        401
    );
}

#[sqlx::test]
async fn admin_reset_requires_admin(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;
    server
        .create_user(&admin.session, "bob", "bob@localhost")
        .await;
    let bob = server.enrolled("bob", USER_PASSWORD).await;

    let anonymous = server
        .client
        .delete(server.url(&format!("/admin/users/{}/mfa", admin.id)))
        .send()
        .await
        .unwrap();
    assert_eq!(anonymous.status(), 401);
    assert_eq!(
        server.admin_reset(&bob.session, &admin.id).await.status(),
        401
    );
    assert_eq!(
        server.admin_reset(&bob.session, &bob.id).await.status(),
        401,
        "not even on oneself"
    );

    assert_eq!(
        server.status(&admin.session).await["totpEnabled"],
        json!(true)
    );
    assert_eq!(
        server.status(&bob.session).await["totpEnabled"],
        json!(true)
    );
}

#[sqlx::test]
async fn admin_reset_clears_the_factor_revokes_sessions_and_forces_re_enrolment(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;
    server
        .create_user(&admin.session, "bob", "bob@localhost")
        .await;
    server
        .create_user(&admin.session, "carol", "carol@localhost")
        .await;
    let bob = server.enrolled("bob", USER_PASSWORD).await;
    let carol = server.enrolled("carol", USER_PASSWORD).await;
    let bob_pending = server.mfa_token("bob", USER_PASSWORD).await;

    let res = server.admin_reset(&admin.session, &bob.id).await;

    assert_eq!(res.status(), 204);
    assert_eq!(server.me(&bob.session).await.status(), 401);
    assert_eq!(
        server
            .verify_backup(&bob_pending, &bob.backup_codes[0])
            .await
            .status(),
        401
    );
    assert_eq!(
        server
            .verify_code(
                &bob_pending,
                &generate_code_at(&bob.secret, now_unix() + 30)
            )
            .await
            .status(),
        401
    );
    assert_eq!(server.backup_code_rows(&bob.id).await, 0);
    let login = server.login("bob", USER_PASSWORD).await;
    assert_eq!(login["mfaSetupRequired"], json!(true));
    assert_eq!(login["mfaHasTotp"], json!(false));
    assert!(login["token"].is_null());
    assert_eq!(
        server
            .verify_backup(login["mfaToken"].as_str().unwrap(), &bob.backup_codes[0])
            .await
            .status(),
        401
    );
    assert!(
        server
            .security_events_of(&bob.id)
            .await
            .contains(&"MfaResetByAdmin".to_string())
    );
    assert_eq!(
        server.actor_of(&bob.id, "MfaResetByAdmin").await,
        Some(admin.id.clone())
    );
    assert_eq!(server.me(&admin.session).await.status(), 200);
    assert_eq!(server.me(&carol.session).await.status(), 200);
    assert_eq!(
        server.status(&carol.session).await,
        json!({ "totpEnabled": true, "backupCodesRemaining": 10, "passkeys": [] })
    );
    assert_eq!(server.backup_code_rows(&carol.id).await, 10);

    let bob_again = server.enrolled("bob", USER_PASSWORD).await;
    assert_eq!(
        server.status(&bob_again.session).await,
        json!({ "totpEnabled": true, "backupCodesRemaining": 10, "passkeys": [] })
    );
    assert_ne!(bob_again.secret, bob.secret);
}

#[sqlx::test]
async fn admin_reset_of_an_unknown_user_is_404(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;

    assert_eq!(
        server
            .admin_reset(&admin.session, &uuid::Uuid::new_v4().to_string())
            .await
            .status(),
        404
    );
    assert_eq!(server.me(&admin.session).await.status(), 200);
}

#[sqlx::test]
async fn admin_reset_of_a_user_without_a_factor_is_a_204(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;
    server
        .create_user(&admin.session, "bob", "bob@localhost")
        .await;
    let bob_id = server
        .state
        .users
        .find_by_username("bob")
        .await
        .unwrap()
        .unwrap()
        .id
        .to_string();

    assert_eq!(
        server.admin_reset(&admin.session, &bob_id).await.status(),
        204
    );
}

#[sqlx::test]
async fn admin_reset_sends_the_reset_mail_to_a_valid_address(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;
    server
        .create_user(&admin.session, "bob", "bob@localhost")
        .await;
    server
        .create_user(&admin.session, "carol", "carol@localhost")
        .await;
    server.set_email("bob", "bob@example.com").await;
    let bob = server.enrolled("bob", USER_PASSWORD).await;
    let carol = server.enrolled("carol", USER_PASSWORD).await;
    wait_for_attempts(&server.mailer, 1).await; // Bob's enrolment mail; `@localhost` addresses get none
    assert_eq!(server.mailer.attempts(), 1);

    assert_eq!(
        server.admin_reset(&admin.session, &bob.id).await.status(),
        204
    );

    wait_for_attempts(&server.mailer, 2).await;
    let sent = server.mailer.sent();
    assert_eq!(sent.len(), 2, "{sent:?}");
    assert_eq!(
        sent[1],
        (
            "bob@example.com".to_string(),
            email_templates::mfa_reset("bob").subject
        )
    );

    assert_eq!(
        server.admin_reset(&admin.session, &carol.id).await.status(),
        204
    );
    assert_eq!(settled_attempts(&server.mailer).await, 2);
    assert_eq!(server.me(&carol.session).await.status(), 401);
}

#[sqlx::test]
async fn self_service_endpoints_are_rate_limited(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await; // 2 attempts already used by the enrolment
    server
        .create_user(&admin.session, "bob", "bob@localhost")
        .await;

    for _ in 0..3 {
        assert_eq!(
            server
                .regenerate(&admin.session, WRONG_PASSWORD)
                .await
                .status(),
            400
        );
        assert_eq!(
            server
                .enroll_self(&admin.session, WRONG_PASSWORD)
                .await
                .status(),
            400
        );
    }
    for _ in 0..2 {
        assert_eq!(
            server
                .disable(&admin.session, WRONG_PASSWORD)
                .await
                .status(),
            400
        );
    }
    let hashes_before = server.backup_code_hashes(&admin.id).await;
    assert_eq!(hashes_before.len(), 10);
    let limited = server.regenerate(&admin.session, ADMIN_PASSWORD).await;
    assert_eq!(limited.status(), 429);
    assert!(
        limited
            .json::<Value>()
            .await
            .unwrap()
            .get("backupCodes")
            .is_none()
    );
    assert_eq!(
        server
            .disable(&admin.session, ADMIN_PASSWORD)
            .await
            .status(),
        429
    );
    assert_eq!(
        server
            .enroll_self(&admin.session, ADMIN_PASSWORD)
            .await
            .status(),
        429
    );
    assert_eq!(
        server
            .confirm_self(&admin.session, &totp_code(&admin.secret))
            .await
            .status(),
        429
    );
    // Reading the status is not a guessing surface and stays available.
    assert_eq!(
        server.status(&admin.session).await,
        json!({ "totpEnabled": true, "backupCodesRemaining": 10, "passkeys": [] })
    );
    assert_eq!(
        server.backup_code_hashes(&admin.id).await,
        hashes_before,
        "a refused regeneration must not have replaced the codes: the limiter runs before the business call"
    );
    assert_eq!(server.me(&admin.session).await.status(), 200);
    let mfa_token_is_limited_too = server.mfa_token("admin", ADMIN_PASSWORD).await;
    assert_eq!(
        server
            .verify_backup(&mfa_token_is_limited_too, &admin.backup_codes[0])
            .await
            .status(),
        429,
        "the budget is shared with the login challenge"
    );

    let bob = server.enrolled("bob", USER_PASSWORD).await;
    assert_eq!(
        server
            .regenerate(&bob.session, ADMIN_PASSWORD)
            .await
            .status(),
        400
    );
}

#[sqlx::test]
async fn secrets_never_appear_outside_enroll_and_confirm_responses(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await;
    let secret = admin.secret.as_str();
    let leaks = |body: &str| {
        body.contains(secret)
            || admin.backup_codes.iter().any(|c| body.contains(c.as_str()))
            || body.contains("otpauth")
    };

    let status = server
        .get_as(&admin.session, "/me/mfa")
        .await
        .text()
        .await
        .unwrap();
    assert!(!leaks(&status), "{status}");
    assert_eq!(
        keys(&serde_json::from_str::<Value>(&status).unwrap()),
        ["backupCodesRemaining", "passkeys", "totpEnabled"]
    );

    for res in [
        server.regenerate(&admin.session, WRONG_PASSWORD).await,
        server.enroll_self(&admin.session, ADMIN_PASSWORD).await,
        server.confirm_self(&admin.session, "123456").await,
        server.disable(&admin.session, WRONG_PASSWORD).await,
    ] {
        let body = res.text().await.unwrap();
        assert!(!leaks(&body), "{body}");
    }

    let regenerated = server
        .regenerate(&admin.session, ADMIN_PASSWORD)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        !leaks(&regenerated),
        "the old codes and the secret must not appear: {regenerated}"
    );
    let regenerated: Value = serde_json::from_str(&regenerated).unwrap();
    assert_eq!(keys(&regenerated), ["backupCodes"]);
    assert_eq!(codes_of(&regenerated).len(), 10);

    let session = server.without_factor(&admin).await;
    let enrol: Value = server
        .enroll_self(&session, ADMIN_PASSWORD)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(keys(&enrol), ["otpauthUrl", "secret"]);
    let new_secret = enrol["secret"].as_str().unwrap().to_string();
    let confirm: Value = server
        .confirm_self(&session, &totp_code(&new_secret))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(keys(&confirm), ["backupCodes"]);
    let status = server
        .get_as(&session, "/me/mfa")
        .await
        .text()
        .await
        .unwrap();
    assert!(
        !status.contains(&new_secret)
            && !codes_of(&confirm)
                .iter()
                .any(|c| status.contains(c.as_str())),
        "{status}"
    );

    let stored: Vec<u8> = sqlx::query_scalar("SELECT encrypted_secret FROM totp_credentials")
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert!(!String::from_utf8_lossy(&stored).contains(&new_secret));

    let disabled = server.disable(&session, ADMIN_PASSWORD).await;
    assert_eq!(disabled.status(), 204);
    assert!(disabled.bytes().await.unwrap().is_empty());
}

#[sqlx::test]
async fn change_password_is_limited_like_the_other_password_gated_endpoints(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server.enrolled("admin", ADMIN_PASSWORD).await; // 2 attempts already used by the enrolment
    server
        .create_user(&admin.session, "bob", "bob@localhost")
        .await;
    let change = |session: String, current: &'static str, new: &'static str| {
        let server = &server;
        async move {
            server
                .post_as(
                    &session,
                    "/auth/me/password",
                    json!({ "currentPassword": current, "newPassword": new }),
                )
                .await
        }
    };

    for _ in 0..8 {
        assert_eq!(
            change(
                admin.session.clone(),
                WRONG_PASSWORD,
                "another-password-123"
            )
            .await
            .status(),
            400
        );
    }
    let limited = change(
        admin.session.clone(),
        ADMIN_PASSWORD,
        "another-password-123",
    )
    .await;
    assert_eq!(limited.status(), 429);
    assert!(server.login("admin", ADMIN_PASSWORD).await["mfaToken"].is_string());
    assert_eq!(
        server.me(&admin.session).await.status(),
        200,
        "a refused change must not revoke the session"
    );

    let bob = server.enrolled("bob", USER_PASSWORD).await;
    assert_eq!(
        change(bob.session.clone(), WRONG_PASSWORD, "another-password-123")
            .await
            .status(),
        400
    );
}
