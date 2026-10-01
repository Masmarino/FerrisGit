// End-to-end passkey (WebAuthn) MFA factor with a real software authenticator.
// MFA is enforced (the default).

mod common;

use common::*;
use ferrisgit_application::email_templates;
use serde_json::{Value, json};
use sqlx::PgPool;

const INVALID_CODE: &str = "invalid code";
const INVALID_TOKEN: &str = "invalid or expired token";
const TOO_MANY: &str = "too many attempts, try again later";
const ALREADY_SET_UP: &str = "MFA is already set up";
const UNAVAILABLE: &str = "passkeys are not available on this server";

/// Same length as the original, so it still parses but cannot verify.
fn tampered(credential: &Value) -> Value {
    let mut tampered = credential.clone();
    let signature = tampered["response"]["signature"]
        .as_str()
        .expect("an assertion has a signature")
        .to_string();
    tampered["response"]["signature"] = json!("A".repeat(signature.len()));
    tampered
}

async fn assert_refused(res: reqwest::Response) {
    assert_error(res, 401, INVALID_CODE).await;
}

#[sqlx::test]
async fn first_setup_with_a_passkey_gives_a_session_backup_codes_and_the_enrolment_mail(
    pool: PgPool,
) {
    let server = spawn_server(pool).await;
    server.new_user("alice").await;
    let mut device = Device::new();

    let login = server.login("alice", USER_PASSWORD).await;
    assert_eq!(
        keys(&login),
        key_set(&[
            "token",
            "mfaToken",
            "mfaSetupRequired",
            "mfaHasTotp",
            "mfaHasPasskey"
        ])
    );
    assert_eq!(
        (
            login["mfaSetupRequired"].clone(),
            login["mfaHasTotp"].clone(),
            login["mfaHasPasskey"].clone()
        ),
        (json!(true), json!(false), json!(false))
    );
    let mfa_token = login["mfaToken"].as_str().unwrap().to_string();

    let start = server.setup_start(&mfa_token).await;
    assert_eq!(start.status(), 200);
    let start: Value = start.json().await.unwrap();
    assert_eq!(keys(&start), key_set(&["challengeId", "publicKey"]));
    let public_key = &start["publicKey"];
    assert!(
        uuid::Uuid::parse_str(start["challengeId"].as_str().unwrap()).is_ok(),
        "the challenge id is a UUID"
    );
    assert!(
        public_key["challenge"]
            .as_str()
            .is_some_and(|c| !c.is_empty())
    );
    assert!(
        public_key.get("publicKey").is_none(),
        "the inner publicKey object, not the double nesting: {public_key}"
    );
    assert_eq!(public_key["rp"]["id"], json!("localhost"));
    assert_eq!(public_key["rp"]["name"], json!("FerrisGit"));
    assert_eq!(public_key["user"]["name"], json!("alice"));

    let credential = device.register(public_key);
    let finish = server
        .setup_finish(
            &mfa_token,
            start["challengeId"].as_str().unwrap(),
            &credential,
            "MacBook de Florian",
        )
        .await;
    assert_eq!(finish.status(), 200);
    let body: Value = finish.json().await.unwrap();
    assert_eq!(keys(&body), key_set(&["token", "backupCodes"]));
    let codes = codes_of(&body);
    assert_eq!(codes.len(), 10);
    assert_eq!(
        codes.iter().collect::<std::collections::HashSet<_>>().len(),
        10,
        "ten distinct codes"
    );
    let session = body["token"].as_str().unwrap();

    assert_eq!(server.me(session).await.status(), 200, "the session works");
    assert_error(
        server
            .post(
                "/auth/mfa/verify",
                json!({ "mfaToken": mfa_token, "backupCode": codes[0] }),
            )
            .await,
        401,
        INVALID_TOKEN,
    )
    .await;
    assert_error(server.passkey_start(&mfa_token).await, 401, INVALID_TOKEN).await;
    assert_error(server.setup_start(&mfa_token).await, 401, INVALID_TOKEN).await;
    let alice = server.user_id("alice").await;
    assert_eq!(server.passkey_rows(&alice).await, 1);
    assert_eq!(server.backup_code_rows(&alice).await, 10);
    assert!(
        server
            .security_events_of(&alice)
            .await
            .contains(&"PasskeyAdded".to_string())
    );
    let status = server.status(session).await;
    assert_eq!(status["totpEnabled"], json!(false));
    assert_eq!(status["backupCodesRemaining"], json!(10));
    assert_eq!(status["passkeys"].as_array().unwrap().len(), 1);
    assert_eq!(status["passkeys"][0]["name"], json!("MacBook de Florian"));

    wait_for_attempts(&server.mailer, 1).await;
    let expected = email_templates::mfa_enrolled("alice", PASSKEY_METHOD_LABEL);
    assert_eq!(
        server.mailer.sent(),
        vec![("alice@example.com".to_string(), expected.subject)]
    );
    assert!(
        server.mailer.texts()[0].contains("une clé d'accès (passkey)"),
        "{}",
        server.mailer.texts()[0]
    );
}

#[sqlx::test]
async fn no_enrolment_mail_goes_to_an_undeliverable_address(pool: PgPool) {
    let server = spawn_server(pool).await;
    let admin = server
        .first_setup_with_passkey(&mut Device::new(), "admin", ADMIN_PASSWORD, "Clé")
        .await;

    assert_eq!(server.me(&admin.session).await.status(), 200);
    assert_eq!(
        settled_attempts(&server.mailer).await,
        0,
        "nothing can be delivered to admin@localhost"
    );
}

#[sqlx::test]
async fn the_setup_finish_answers_400_for_an_unusable_name_and_keeps_the_ceremony(pool: PgPool) {
    let server = spawn_server(pool).await;
    server.new_user("alice").await;
    let mut device = Device::new();
    let mfa_token = server.mfa_token("alice", USER_PASSWORD).await;
    let (challenge_id, public_key) = server.start_setup_challenge(&mfa_token).await;
    let credential = device.register(&public_key);

    let too_long = "x".repeat(41);
    for bad in ["", "   ", "line\nbreak", too_long.as_str()] {
        assert_error(
            server
                .setup_finish(&mfa_token, &challenge_id, &credential, bad)
                .await,
            400,
            "invalid passkey name",
        )
        .await;
    }
    let alice = server.user_id("alice").await;
    assert_eq!(server.passkey_rows(&alice).await, 0);

    let res = server
        .setup_finish(&mfa_token, &challenge_id, &credential, "  Ma clé  ")
        .await;
    assert_eq!(res.status(), 200);
    assert_eq!(
        server
            .status(
                res.json::<Value>().await.unwrap()["token"]
                    .as_str()
                    .unwrap()
            )
            .await["passkeys"][0]["name"],
        json!("Ma clé")
    );
}

#[sqlx::test]
async fn the_next_login_is_a_passkey_challenge_and_the_mfa_token_is_single_use(pool: PgPool) {
    let server = spawn_unthrottled(pool).await;
    server.new_user("alice").await;
    let mut device = Device::new();
    let alice = server
        .first_setup_with_passkey(&mut device, "alice", USER_PASSWORD, "MacBook")
        .await;

    let login = server.login("alice", USER_PASSWORD).await;
    assert_eq!(
        keys(&login),
        key_set(&[
            "token",
            "mfaToken",
            "mfaSetupRequired",
            "mfaHasTotp",
            "mfaHasPasskey"
        ])
    );
    assert_eq!(
        (
            login["mfaSetupRequired"].clone(),
            login["mfaHasTotp"].clone(),
            login["mfaHasPasskey"].clone()
        ),
        (json!(false), json!(false), json!(true))
    );
    let mfa_token = login["mfaToken"].as_str().unwrap().to_string();

    let start = server.passkey_start(&mfa_token).await;
    assert_eq!(start.status(), 200);
    let start: Value = start.json().await.unwrap();
    assert_eq!(keys(&start), key_set(&["challengeId", "publicKey"]));
    let stored_ids = server.stored_credential_ids(&alice.id).await;
    let allowed: Vec<&str> = start["publicKey"]["allowCredentials"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        allowed,
        stored_ids.iter().map(String::as_str).collect::<Vec<_>>(),
        "the challenge lists the user's own credential, and only that"
    );
    assert_eq!(start["publicKey"]["rpId"], json!("localhost"));

    // A failed assertion is a plain 401 and must not spend the token nor burn the login.
    let genuine = device.assert(&start["publicKey"]);
    assert_refused(
        server
            .passkey_finish(
                &mfa_token,
                start["challengeId"].as_str().unwrap(),
                &tampered(&genuine),
            )
            .await,
    )
    .await;
    assert!(
        server
            .security_events_of(&alice.id)
            .await
            .contains(&"PasskeyVerificationFailed".to_string())
    );

    let (first_id, first_key) = server.start_login_challenge(&mfa_token).await;
    let (second_id, second_key) = server.start_login_challenge(&mfa_token).await;
    let first = device.assert(&first_key);
    let second = device.assert(&second_key);
    let finish = server.passkey_finish(&mfa_token, &first_id, &first).await;
    assert_eq!(finish.status(), 200);
    let body: Value = finish.json().await.unwrap();
    assert_eq!(keys(&body), key_set(&["token"]));
    assert_eq!(
        server.me(body["token"].as_str().unwrap()).await.status(),
        200,
        "the session works"
    );
    assert!(
        server
            .security_events_of(&alice.id)
            .await
            .contains(&"LoginSucceeded".to_string())
    );
    let state = server.passkey_state(&alice.id).await;
    assert!(state[0].1.is_some(), "last_used_at is recorded");

    assert_error(
        server.passkey_finish(&mfa_token, &second_id, &second).await,
        401,
        INVALID_TOKEN,
    )
    .await;
    assert_error(server.passkey_start(&mfa_token).await, 401, INVALID_TOKEN).await;
    let session = server
        .login_with_passkey(&mut device, "alice", USER_PASSWORD)
        .await;
    assert_eq!(server.me(&session).await.status(), 200);
}

#[sqlx::test]
async fn every_failed_assertion_is_the_same_401_and_changes_nothing(pool: PgPool) {
    let server = spawn_unthrottled(pool).await;
    for name in ["alice", "bob", "carol"] {
        server.new_user(name).await;
    }
    let mut alices_device = Device::new();
    let mut bobs_device = Device::new();
    let alice = server
        .first_setup_with_passkey(&mut alices_device, "alice", USER_PASSWORD, "Clé d'Alice")
        .await;
    let bob = server
        .first_setup_with_passkey(&mut bobs_device, "bob", USER_PASSWORD, "Clé de Bob")
        .await;
    let carol = server.user_id("carol").await;
    let before = server.passkey_state(&alice.id).await;
    let events_before = server.security_events_of(&alice.id).await.len();
    let bobs_id = server.stored_credential_ids(&bob.id).await.remove(0);
    let token = server.pending_token(&alice.id).await;

    // A device that registered nowhere (its registration ceremony is never finished).
    let mut stranger = Device::new();
    let carols_token = server.pending_token(&carol).await;
    let (_, carols_challenge) = server.start_setup_challenge(&carols_token).await;
    let stranger_credential = stranger.register(&carols_challenge);
    let strangers_id = credential_id_of(&stranger_credential);

    let (challenge_id, public_key) = server.start_login_challenge(&token).await;
    assert_refused(
        server
            .passkey_finish(
                &token,
                &challenge_id,
                &tampered(&alices_device.assert(&public_key)),
            )
            .await,
    )
    .await;

    // A failure spends the ceremony: the real assertion for the same challenge no longer works.
    assert_refused(
        server
            .passkey_finish(&token, &challenge_id, &alices_device.assert(&public_key))
            .await,
    )
    .await;

    let (_, public_key) = server.start_login_challenge(&token).await;
    let genuine = alices_device.assert(&public_key);
    assert_refused(
        server
            .passkey_finish(&token, &uuid::Uuid::new_v4().to_string(), &genuine)
            .await,
    )
    .await;
    assert_eq!(
        server
            .passkey_finish(&token, "not-a-uuid", &genuine)
            .await
            .status(),
        400
    );

    let (challenge_id, public_key) = server.start_login_challenge(&token).await;
    let forged = stranger.assert(&challenge_for_credential(&public_key, &strangers_id));
    assert_refused(server.passkey_finish(&token, &challenge_id, &forged).await).await;

    let (challenge_id, public_key) = server.start_login_challenge(&token).await;
    let borrowed = bobs_device.assert(&challenge_for_credential(&public_key, &bobs_id));
    assert_refused(
        server
            .passkey_finish(&token, &challenge_id, &borrowed)
            .await,
    )
    .await;

    // Another user's token cannot finish this ceremony, and does not spend it either.
    let bobs_token = server.pending_token(&bob.id).await;
    let (alices_challenge, alices_key) = server.start_login_challenge(&token).await;
    let alices_assertion = alices_device.assert(&alices_key);
    assert_refused(
        server
            .passkey_finish(&bobs_token, &alices_challenge, &alices_assertion)
            .await,
    )
    .await;

    assert_eq!(server.passkey_state(&alice.id).await, before);
    assert!(
        !server.security_events_of(&alice.id).await[events_before..]
            .contains(&"LoginSucceeded".to_string())
    );
    assert_eq!(
        server.passkey_rows(&carol).await,
        0,
        "the never-finished registration stored nothing"
    );
    let finish = server
        .passkey_finish(&token, &alices_challenge, &alices_assertion)
        .await;
    assert_eq!(
        finish.status(),
        200,
        "neither the token nor the ceremony was consumed by the failures"
    );
}

#[sqlx::test]
async fn an_old_assertion_cannot_be_replayed_against_a_new_challenge(pool: PgPool) {
    let server = spawn_unthrottled(pool).await;
    server.new_user("alice").await;
    let mut device = Device::new();
    let alice = server
        .first_setup_with_passkey(&mut device, "alice", USER_PASSWORD, "Clé")
        .await;

    let first_token = server.pending_token(&alice.id).await;
    let (challenge_id, public_key) = server.start_login_challenge(&first_token).await;
    let assertion = device.assert(&public_key);
    assert_eq!(
        server
            .passkey_finish(&first_token, &challenge_id, &assertion)
            .await
            .status(),
        200
    );

    // The captured assertion answers a different challenge (its clientDataJSON carries the old one).
    let second_token = server.pending_token(&alice.id).await;
    let (new_challenge_id, _) = server.start_login_challenge(&second_token).await;
    assert_refused(
        server
            .passkey_finish(&second_token, &new_challenge_id, &assertion)
            .await,
    )
    .await;
    assert_refused(
        server
            .passkey_finish(&second_token, &challenge_id, &assertion)
            .await,
    )
    .await;
}

#[sqlx::test]
async fn a_passkey_deleted_mid_login_is_a_failed_login_not_a_404(pool: PgPool) {
    let (server, sabotage) = spawn_sabotaged(pool).await;
    server.new_user("alice").await;
    let mut device = Device::new();
    let alice = server
        .first_setup_with_passkey(&mut device, "alice", USER_PASSWORD, "Clé")
        .await;

    let token = server.pending_token(&alice.id).await;
    let (challenge_id, public_key) = server.start_login_challenge(&token).await;
    let assertion = device.assert(&public_key);
    sabotage
        .vanish_on_update
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert_refused(
        server
            .passkey_finish(&token, &challenge_id, &assertion)
            .await,
    )
    .await;
    assert!(
        server
            .security_events_of(&alice.id)
            .await
            .contains(&"PasskeyVerificationFailed".to_string())
    );

    sabotage
        .vanish_on_update
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let (challenge_id, public_key) = server.start_login_challenge(&token).await;
    assert_eq!(
        server
            .passkey_finish(&token, &challenge_id, &device.assert(&public_key))
            .await
            .status(),
        200
    );
}

#[sqlx::test]
async fn an_expired_ceremony_is_refused_for_the_login_and_for_the_setup(pool: PgPool) {
    let server = spawn_server(pool.clone()).await;
    server.new_user("alice").await;
    server.new_user("bob").await;
    let mut device = Device::new();
    let alice = server
        .first_setup_with_passkey(&mut device, "alice", USER_PASSWORD, "Clé")
        .await;
    // Same database, same secrets, but ceremonies that are already expired when they are read.
    let expiring = spawn_with_instantly_expiring_ceremonies(pool).await;

    let token = expiring.pending_token(&alice.id).await;
    let (challenge_id, public_key) = expiring.start_login_challenge(&token).await;
    assert_refused(
        expiring
            .passkey_finish(&token, &challenge_id, &device.assert(&public_key))
            .await,
    )
    .await;

    let bobs_token = expiring.pending_token(&server.user_id("bob").await).await;
    let (challenge_id, public_key) = expiring.start_setup_challenge(&bobs_token).await;
    let credential = Device::new().register(&public_key);
    assert_error(
        expiring
            .setup_finish(&bobs_token, &challenge_id, &credential, "Clé")
            .await,
        400,
        "invalid passkey",
    )
    .await;
    assert_eq!(expiring.passkey_rows(&server.user_id("bob").await).await, 0);
}

#[sqlx::test]
async fn a_session_token_is_not_an_mfa_token_on_any_passkey_route(pool: PgPool) {
    let server = spawn_server(pool).await;
    server.new_user("alice").await;
    server.new_user("carol").await;
    let mut device = Device::new();
    let alice = server
        .first_setup_with_passkey(&mut device, "alice", USER_PASSWORD, "Clé")
        .await;
    // Well-formed credentials, so that the token is what gets refused.
    let token = server.pending_token(&alice.id).await;
    let (_, public_key) = server.start_login_challenge(&token).await;
    let assertion = device.assert(&public_key);
    let carols_token = server.pending_token(&server.user_id("carol").await).await;
    let (_, public_key) = server.start_setup_challenge(&carols_token).await;
    let attestation = Device::new().register(&public_key);

    for (path, body) in [
        (
            "/auth/mfa/passkey/start",
            json!({ "mfaToken": alice.session }),
        ),
        (
            "/auth/mfa/passkey/finish",
            json!({ "mfaToken": alice.session, "challengeId": uuid::Uuid::new_v4(), "credential": assertion }),
        ),
        (
            "/auth/mfa/setup/passkey/start",
            json!({ "mfaToken": alice.session }),
        ),
        (
            "/auth/mfa/setup/passkey/finish",
            json!({ "mfaToken": alice.session, "challengeId": uuid::Uuid::new_v4(), "credential": attestation, "name": "x" }),
        ),
    ] {
        assert_error(server.post(path, body).await, 401, INVALID_TOKEN).await;
    }
    assert_error(server.passkey_start("garbage").await, 401, INVALID_TOKEN).await;
}

#[sqlx::test]
async fn a_passkey_challenge_needs_a_passkey(pool: PgPool) {
    let server = spawn_server(pool).await;
    server.new_user("alice").await;
    let alice = server.first_setup_with_totp("alice", USER_PASSWORD).await;

    let token = server.pending_token(&alice.id).await;
    assert_error(
        server.passkey_start(&token).await,
        400,
        "no passkey is registered",
    )
    .await;
}

#[sqlx::test]
async fn setup_with_a_passkey_is_refused_when_a_factor_already_exists(pool: PgPool) {
    let server = spawn_unthrottled(pool).await;
    for name in ["totp_user", "passkey_user", "late_totp", "late_passkey"] {
        server.new_user(name).await;
    }
    let totp_user = server
        .first_setup_with_totp("totp_user", USER_PASSWORD)
        .await;
    let passkey_user = server
        .first_setup_with_passkey(&mut Device::new(), "passkey_user", USER_PASSWORD, "Clé")
        .await;

    for user in [&totp_user, &passkey_user] {
        let token = server.pending_token(&user.id).await;
        assert_error(server.setup_start(&token).await, 400, ALREADY_SET_UP).await;
    }

    // Race: the ceremony starts while the user has no factor, a TOTP appears, then the finish arrives.
    let late_totp = server.user_id("late_totp").await;
    let slow_token = server.pending_token(&late_totp).await;
    let mut slow_device = Device::new();
    let (challenge_id, public_key) = server.start_setup_challenge(&slow_token).await;
    let credential = slow_device.register(&public_key);
    let with_totp = server
        .first_setup_with_totp("late_totp", USER_PASSWORD)
        .await;
    let hashes = server.backup_code_hashes(&late_totp).await;
    assert_error(
        server
            .setup_finish(&slow_token, &challenge_id, &credential, "Trop tard")
            .await,
        400,
        ALREADY_SET_UP,
    )
    .await;
    assert_eq!(
        server.passkey_rows(&late_totp).await,
        0,
        "nothing registered"
    );
    assert_eq!(
        server.backup_code_hashes(&late_totp).await,
        hashes,
        "the TOTP user's backup codes are untouched"
    );
    assert_eq!(server.me(&with_totp.session).await.status(), 200);

    let late_passkey = server.user_id("late_passkey").await;
    let slow_token = server.pending_token(&late_passkey).await;
    let (challenge_id, public_key) = server.start_setup_challenge(&slow_token).await;
    let credential = Device::new().register(&public_key);
    let with_passkey = server
        .first_setup_with_passkey(
            &mut Device::new(),
            "late_passkey",
            USER_PASSWORD,
            "Première",
        )
        .await;
    let hashes = server.backup_code_hashes(&late_passkey).await;
    assert_error(
        server
            .setup_finish(&slow_token, &challenge_id, &credential, "Trop tard")
            .await,
        400,
        ALREADY_SET_UP,
    )
    .await;
    assert_eq!(server.passkey_rows(&late_passkey).await, 1);
    assert_eq!(
        server.backup_code_hashes(&late_passkey).await,
        hashes,
        "no new backup codes were minted"
    );
    assert_eq!(
        server.status(&with_passkey.session).await["backupCodesRemaining"],
        json!(10)
    );
}

#[sqlx::test]
async fn a_pending_token_cannot_bolt_a_totp_onto_a_passkey_user(pool: PgPool) {
    // The mfaToken only proves the password. If the TOTP setup routes accepted it for a user whose factor is a
    // passkey, anybody with the password could enrol their own authenticator and skip the passkey entirely.
    let server = spawn_unthrottled(pool).await;
    server.new_user("alice").await;
    server.new_user("bob").await;
    let alice = server
        .first_setup_with_passkey(&mut Device::new(), "alice", USER_PASSWORD, "Clé")
        .await;
    let token = server.pending_token(&alice.id).await;

    assert_error(
        server
            .post("/auth/mfa/setup/totp/enroll", json!({ "mfaToken": token }))
            .await,
        400,
        ALREADY_SET_UP,
    )
    .await;
    assert_eq!(
        server.totp_rows(&alice.id).await,
        0,
        "no TOTP row was created"
    );

    let bob = server.user_id("bob").await;
    let bobs_token = server.pending_token(&bob).await;
    let enrol: Value = server
        .post(
            "/auth/mfa/setup/totp/enroll",
            json!({ "mfaToken": bobs_token }),
        )
        .await
        .json()
        .await
        .unwrap();
    let secret = enrol["secret"].as_str().unwrap().to_string();
    let bobs_passkey = server
        .first_setup_with_passkey(&mut Device::new(), "bob", USER_PASSWORD, "Clé")
        .await;
    let hashes = server.backup_code_hashes(&bob).await;
    assert_error(
        server
            .post(
                "/auth/mfa/setup/totp/confirm",
                json!({ "mfaToken": bobs_token, "code": totp_code(&secret) }),
            )
            .await,
        400,
        ALREADY_SET_UP,
    )
    .await;
    assert_eq!(
        server.totp_rows(&bob).await,
        0,
        "the pending TOTP was deleted by the passkey setup, never confirmed"
    );
    assert_eq!(server.backup_code_hashes(&bob).await, hashes);
    assert_eq!(
        server.status(&bobs_passkey.session).await["totpEnabled"],
        json!(false)
    );
}

#[sqlx::test]
async fn backup_codes_work_once_each_for_a_passkey_only_user_and_a_totp_code_does_not(
    pool: PgPool,
) {
    let server = spawn_server(pool).await;
    server.new_user("alice").await;
    let alice = server
        .first_setup_with_passkey(&mut Device::new(), "alice", USER_PASSWORD, "Clé")
        .await;

    let token = server.mfa_token("alice", USER_PASSWORD).await;
    assert_error(
        server
            .post(
                "/auth/mfa/verify",
                json!({ "mfaToken": token, "code": "123456" }),
            )
            .await,
        401,
        INVALID_CODE,
    )
    .await;
    let res = server
        .post(
            "/auth/mfa/verify",
            json!({ "mfaToken": token, "backupCode": alice.backup_codes[0] }),
        )
        .await;
    assert_eq!(res.status(), 200);
    let session = res.json::<Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(server.me(&session).await.status(), 200);
    assert_eq!(
        server.status(&session).await["backupCodesRemaining"],
        json!(9)
    );

    let token = server.mfa_token("alice", USER_PASSWORD).await;
    assert_error(
        server
            .post(
                "/auth/mfa/verify",
                json!({ "mfaToken": token, "backupCode": alice.backup_codes[0] }),
            )
            .await,
        401,
        INVALID_CODE,
    )
    .await;
    assert_eq!(
        server
            .post(
                "/auth/mfa/verify",
                json!({ "mfaToken": token, "backupCode": alice.backup_codes[1] })
            )
            .await
            .status(),
        200
    );
}

#[sqlx::test]
async fn either_factor_satisfies_the_challenge_when_a_user_has_both(pool: PgPool) {
    let server = spawn_server(pool).await;
    server.new_user("alice").await;
    let alice = server.first_setup_with_totp("alice", USER_PASSWORD).await;
    let secret = alice.secret.clone().unwrap();
    let mut device = Device::new();
    server
        .add_passkey(&alice.session, &mut device, USER_PASSWORD, "Clé")
        .await;

    let login = server.login("alice", USER_PASSWORD).await;
    assert_eq!(
        (
            login["mfaSetupRequired"].clone(),
            login["mfaHasTotp"].clone(),
            login["mfaHasPasskey"].clone()
        ),
        (json!(false), json!(true), json!(true))
    );
    let token = login["mfaToken"].as_str().unwrap();
    let (challenge_id, public_key) = server.start_login_challenge(token).await;
    assert_eq!(
        server
            .passkey_finish(token, &challenge_id, &device.assert(&public_key))
            .await
            .status(),
        200
    );

    let token = server.mfa_token("alice", USER_PASSWORD).await;
    assert_eq!(
        server
            .post(
                "/auth/mfa/verify",
                json!({ "mfaToken": token, "code": next_step_code(&secret) })
            )
            .await
            .status(),
        200
    );
}

#[sqlx::test]
async fn two_passkey_setups_racing_on_one_token_leave_the_codes_that_were_shown_alive(
    pool: PgPool,
) {
    let server = spawn_unthrottled(pool).await;
    for round in 0..4 {
        let name = format!("racer{round}");
        let id = server.new_user(&name).await;
        let token = server.pending_token(&id).await;
        let (first_id, first_key) = server.start_setup_challenge(&token).await;
        let (second_id, second_key) = server.start_setup_challenge(&token).await;
        let first = Device::new().register(&first_key);
        let second = Device::new().register(&second_key);

        let (a, b) = tokio::join!(
            server.setup_finish(&token, &first_id, &first, "Une"),
            server.setup_finish(&token, &second_id, &second, "Deux")
        );

        let mut statuses = [a.status().as_u16(), b.status().as_u16()];
        statuses.sort();
        assert_eq!(
            statuses,
            [200, 401],
            "round {round}: exactly one setup completes, the other is a spent token"
        );
        let winner: Value = if a.status() == 200 {
            a.json().await.unwrap()
        } else {
            b.json().await.unwrap()
        };
        let shown = codes_of(&winner);
        assert_eq!(
            server.passkey_rows(&id).await,
            1,
            "round {round}: the loser registered nothing"
        );
        assert_eq!(server.backup_code_rows(&id).await, 10);
        let next = server.pending_token(&id).await;
        assert_eq!(
            server
                .post(
                    "/auth/mfa/verify",
                    json!({ "mfaToken": next, "backupCode": shown[0] })
                )
                .await
                .status(),
            200,
            "round {round}"
        );
    }
}

#[sqlx::test]
async fn a_totp_confirmation_and_a_passkey_setup_racing_on_one_token_leave_exactly_one_factor(
    pool: PgPool,
) {
    let server = spawn_unthrottled(pool).await;
    for round in 0..4 {
        let name = format!("mixed{round}");
        let id = server.new_user(&name).await;
        let token = server.pending_token(&id).await;
        let enrol: Value = server
            .post("/auth/mfa/setup/totp/enroll", json!({ "mfaToken": token }))
            .await
            .json()
            .await
            .unwrap();
        let secret = enrol["secret"].as_str().unwrap().to_string();
        let (challenge_id, public_key) = server.start_setup_challenge(&token).await;
        let credential = Device::new().register(&public_key);

        let (totp, passkey) = tokio::join!(
            server.post(
                "/auth/mfa/setup/totp/confirm",
                json!({ "mfaToken": token, "code": totp_code(&secret) })
            ),
            server.setup_finish(&token, &challenge_id, &credential, "Clé")
        );

        let mut statuses = [totp.status().as_u16(), passkey.status().as_u16()];
        statuses.sort();
        assert!(
            statuses == [200, 401] || statuses == [200, 400],
            "round {round}: one winner, got {statuses:?}"
        );
        let winner: Value = if totp.status() == 200 {
            totp.json().await.unwrap()
        } else {
            passkey.json().await.unwrap()
        };
        let confirmed_totp = server.totp_rows(&id).await;
        assert_eq!(
            confirmed_totp + server.passkey_rows(&id).await,
            1,
            "round {round}: exactly one factor, no leftover pending TOTP next to the passkey"
        );
        assert_eq!(server.backup_code_rows(&id).await, 10);
        let next = server.pending_token(&id).await;
        assert_eq!(
            server
                .post(
                    "/auth/mfa/verify",
                    json!({ "mfaToken": next, "backupCode": codes_of(&winner)[0] })
                )
                .await
                .status(),
            200,
            "round {round}: the shown codes work"
        );
    }
}

#[sqlx::test]
async fn a_pending_totp_enrolment_does_not_survive_a_passkey_setup(pool: PgPool) {
    // A secret shown during the setup must not stay confirmable: a session alone (no password) could otherwise
    // confirm it later, plant a TOTP and receive brand new backup codes.
    let server = spawn_server(pool).await;
    server.new_user("alice").await;
    let id = server.user_id("alice").await;
    let token = server.mfa_token("alice", USER_PASSWORD).await;
    let enrol: Value = server
        .post("/auth/mfa/setup/totp/enroll", json!({ "mfaToken": token }))
        .await
        .json()
        .await
        .unwrap();
    let secret = enrol["secret"].as_str().unwrap().to_string();
    assert_eq!(server.totp_rows(&id).await, 1);

    let alice = server
        .first_setup_with_passkey(&mut Device::new(), "alice", USER_PASSWORD, "Clé")
        .await;
    assert_eq!(
        server.totp_rows(&id).await,
        0,
        "the pending enrolment is gone"
    );
    let hashes = server.backup_code_hashes(&id).await;

    assert_error(
        server
            .post_as(
                &alice.session,
                "/me/mfa/totp/confirm",
                json!({ "code": totp_code(&secret) }),
            )
            .await,
        400,
        "no TOTP enrolment to confirm",
    )
    .await;
    assert_error(
        server
            .post(
                "/auth/mfa/setup/totp/confirm",
                json!({ "mfaToken": token, "code": totp_code(&secret) }),
            )
            .await,
        400,
        ALREADY_SET_UP,
    )
    .await;
    assert_eq!(
        server.backup_code_hashes(&id).await,
        hashes,
        "no new backup codes were minted"
    );
    assert_eq!(
        server.status(&alice.session).await["totpEnabled"],
        json!(false)
    );
    assert_eq!(
        server
            .post_as(
                &alice.session,
                "/me/mfa/totp/enroll",
                json!({ "currentPassword": WRONG_PASSWORD })
            )
            .await
            .status(),
        400
    );
}

#[sqlx::test]
async fn the_per_user_budget_covers_the_passkey_finish_and_a_refused_call_consumes_nothing(
    pool: PgPool,
) {
    let server = spawn_server(pool).await;
    server.new_user("alice").await;
    let mut device = Device::new();
    let alice = server
        .first_setup_with_passkey(&mut device, "alice", USER_PASSWORD, "Clé")
        .await;
    // The setup itself spent some of the budget: start the count below from zero (no clock involved).
    server.reset_budget(&alice.id);

    let token = server.pending_token(&alice.id).await;
    let (challenge_id, public_key) = server.start_login_challenge(&token).await; // free: a start guesses nothing
    let genuine = device.assert(&public_key);
    for attempt in 1..=10 {
        let res = server
            .passkey_finish(&token, &uuid::Uuid::new_v4().to_string(), &genuine)
            .await;
        assert_eq!(res.status(), 401, "attempt {attempt} is inside the budget");
    }
    assert_error(
        server.passkey_finish(&token, &challenge_id, &genuine).await,
        429,
        TOO_MANY,
    )
    .await;
    assert_error(
        server
            .post(
                "/auth/mfa/verify",
                json!({ "mfaToken": token, "backupCode": alice.backup_codes[0] }),
            )
            .await,
        429,
        TOO_MANY,
    )
    .await;
    assert_eq!(server.me(&alice.session).await.status(), 200);
    assert_eq!(server.passkey_start(&token).await.status(), 200);

    server.reset_budget(&alice.id);
    let res = server.passkey_finish(&token, &challenge_id, &genuine).await;
    assert_eq!(
        res.status(),
        200,
        "the refused call spent neither the token nor the ceremony"
    );
}

#[sqlx::test]
async fn starting_challenges_does_not_spend_the_per_user_budget_so_backup_codes_stay_usable(
    pool: PgPool,
) {
    // A dismissed browser prompt and retries only start new challenges; a dozen of them must not lock the user out of every MFA path.
    let server = spawn_server(pool).await;
    server.new_user("alice").await;
    server.new_user("bob").await;
    let mut device = Device::new();
    let alice = server
        .first_setup_with_passkey(&mut device, "alice", USER_PASSWORD, "Clé")
        .await;
    server.reset_budget(&alice.id);
    let token = server.pending_token(&alice.id).await;

    let mut last = None;
    for _ in 0..12 {
        last = Some(server.start_login_challenge(&token).await);
    }
    let (challenge_id, public_key) = last.unwrap();
    assert_eq!(
        server
            .passkey_finish(&token, &challenge_id, &device.assert(&public_key))
            .await
            .status(),
        200,
        "the latest challenge completes the login"
    );
    let next = server.pending_token(&alice.id).await;
    assert_eq!(
        server
            .post(
                "/auth/mfa/verify",
                json!({ "mfaToken": next, "backupCode": alice.backup_codes[0] })
            )
            .await
            .status(),
        200,
        "the backup codes are still usable"
    );

    let bob = server.user_id("bob").await;
    let bobs_token = server.pending_token(&bob).await;
    for _ in 0..12 {
        server.start_setup_challenge(&bobs_token).await;
    }
    let (challenge_id, public_key) = server.start_setup_challenge(&bobs_token).await;
    assert_eq!(
        server
            .setup_finish(
                &bobs_token,
                &challenge_id,
                &Device::new().register(&public_key),
                "Clé"
            )
            .await
            .status(),
        200
    );
}

async fn backdate_pending_totp(server: &Server, user_id: &str, minutes: i32) {
    sqlx::query("UPDATE totp_credentials SET created_at = now() - make_interval(mins => $2) WHERE user_id = $1::uuid AND NOT confirmed")
        .bind(user_id)
        .bind(minutes)
        .execute(&server.pool)
        .await
        .unwrap();
}

#[sqlx::test]
async fn an_abandoned_account_side_enrolment_cannot_be_confirmed_by_a_session_alone(pool: PgPool) {
    // A stolen session (no password) sending the right code for an abandoned enrolment must plant nothing and not replace the backup codes.
    let server = spawn_server(pool).await;
    server.new_user("alice").await;
    let alice = server
        .first_setup_with_passkey(&mut Device::new(), "alice", USER_PASSWORD, "Clé")
        .await;
    let enrol: Value = server
        .post_as(
            &alice.session,
            "/me/mfa/totp/enroll",
            json!({ "currentPassword": USER_PASSWORD }),
        )
        .await
        .json()
        .await
        .unwrap();
    let secret = enrol["secret"].as_str().unwrap().to_string();
    let hashes = server.backup_code_hashes(&alice.id).await;
    backdate_pending_totp(&server, &alice.id, 16).await;

    assert_error(
        server
            .post_as(
                &alice.session,
                "/me/mfa/totp/confirm",
                json!({ "code": totp_code(&secret) }),
            )
            .await,
        400,
        "invalid code",
    )
    .await;
    assert_eq!(
        server.backup_code_hashes(&alice.id).await,
        hashes,
        "the user's own codes are intact"
    );
    assert_eq!(
        server.status(&alice.session).await["totpEnabled"],
        json!(false)
    );
    assert!(
        server
            .security_events_of(&alice.id)
            .await
            .iter()
            .all(|e| e != "MfaEnrolled")
    );

    let again: Value = server
        .post_as(
            &alice.session,
            "/me/mfa/totp/enroll",
            json!({ "currentPassword": USER_PASSWORD }),
        )
        .await
        .json()
        .await
        .unwrap();
    let fresh = again["secret"].as_str().unwrap().to_string();
    assert_ne!(fresh, secret);
    assert_error(
        server
            .post_as(
                &alice.session,
                "/me/mfa/totp/confirm",
                json!({ "code": totp_code(&secret) }),
            )
            .await,
        400,
        "invalid code",
    )
    .await;
    let res = server
        .post_as(
            &alice.session,
            "/me/mfa/totp/confirm",
            json!({ "code": totp_code(&fresh) }),
        )
        .await;
    assert_eq!(res.status(), 200);
    assert_eq!(codes_of(&res.json::<Value>().await.unwrap()).len(), 10);
}

#[sqlx::test]
async fn an_abandoned_setup_enrolment_expires_too_and_a_recent_one_still_confirms(pool: PgPool) {
    let server = spawn_server(pool).await;
    server.new_user("alice").await;
    server.new_user("bob").await;
    let (alice, bob) = (server.user_id("alice").await, server.user_id("bob").await);

    let token = server.pending_token(&alice).await;
    let secret = server
        .post("/auth/mfa/setup/totp/enroll", json!({ "mfaToken": token }))
        .await
        .json::<Value>()
        .await
        .unwrap()["secret"]
        .as_str()
        .unwrap()
        .to_string();
    backdate_pending_totp(&server, &alice, 16).await;
    assert_error(
        server
            .post(
                "/auth/mfa/setup/totp/confirm",
                json!({ "mfaToken": token, "code": totp_code(&secret) }),
            )
            .await,
        400,
        "invalid code",
    )
    .await;
    assert_eq!(server.backup_code_rows(&alice).await, 0);

    let token = server.pending_token(&bob).await;
    let secret = server
        .post("/auth/mfa/setup/totp/enroll", json!({ "mfaToken": token }))
        .await
        .json::<Value>()
        .await
        .unwrap()["secret"]
        .as_str()
        .unwrap()
        .to_string();
    backdate_pending_totp(&server, &bob, 14).await;
    assert_eq!(
        server
            .post(
                "/auth/mfa/setup/totp/confirm",
                json!({ "mfaToken": token, "code": totp_code(&secret) })
            )
            .await
            .status(),
        200
    );
}

#[sqlx::test]
async fn the_two_start_routes_share_a_per_ip_budget_of_30_per_client(pool: PgPool) {
    let server = spawn_with(
        pool,
        Options {
            trusted_proxy_cidrs: "127.0.0.0/8".to_string(),
            ..Options::default()
        },
        |_| {},
    )
    .await;
    let mut ids = Vec::new();
    for name in ["u1", "u2", "u3", "u4", "u5"] {
        ids.push(server.new_user(name).await);
    }
    let passkey_user = server
        .first_setup_with_passkey(&mut Device::new(), "u5", USER_PASSWORD, "Clé")
        .await;
    ids.pop();
    let start_as = |client: &'static str, path: &'static str, token: String| {
        let request = server
            .client
            .post(server.url(path))
            .header("X-Forwarded-For", client)
            .json(&json!({ "mfaToken": token }));
        async move { request.send().await.unwrap() }
    };

    for round in 0..30 {
        let user = &ids[round % 4];
        let res = start_as(
            "203.0.113.1",
            "/auth/mfa/setup/passkey/start",
            server.pending_token(user).await,
        )
        .await;
        assert_eq!(res.status(), 200, "start {} of the client", round + 1);
    }
    let res = start_as(
        "203.0.113.1",
        "/auth/mfa/passkey/start",
        server.pending_token(&passkey_user.id).await,
    )
    .await;
    assert_error(res, 429, TOO_MANY).await;
    // Refused for the budget before the token is looked at (a garbage token is not refused for being garbage).
    assert_error(
        start_as(
            "203.0.113.1",
            "/auth/mfa/passkey/start",
            "garbage".to_string(),
        )
        .await,
        429,
        TOO_MANY,
    )
    .await;
    assert_error(
        start_as(
            "203.0.113.2",
            "/auth/mfa/passkey/start",
            "garbage".to_string(),
        )
        .await,
        401,
        INVALID_TOKEN,
    )
    .await;
    let res = start_as(
        "203.0.113.2",
        "/auth/mfa/passkey/start",
        server.pending_token(&passkey_user.id).await,
    )
    .await;
    assert_eq!(res.status(), 200);
    // Prepending a fake address does not shake the budget off: the proxy appends the real one on the right.
    assert_error(
        start_as(
            "9.9.9.9, 203.0.113.1",
            "/auth/mfa/passkey/start",
            "garbage".to_string(),
        )
        .await,
        429,
        TOO_MANY,
    )
    .await;
}

#[sqlx::test]
async fn config_reports_whether_passkeys_are_available(pool: PgPool) {
    let server = spawn_server(pool.clone()).await;
    let res = server
        .client
        .get(server.url("/auth/config"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(
        res.json::<Value>().await.unwrap(),
        json!({ "registrationEnabled": false, "passkeysAvailable": true, "publicPagesEnabled": true })
    );

    let ip_literal = spawn_with(
        pool,
        Options {
            public_url: "http://127.0.0.1:4200".to_string(),
            ..Options::default()
        },
        |_| {},
    )
    .await;
    let res = ip_literal
        .client
        .get(ip_literal.url("/auth/config"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        res.json::<Value>().await.unwrap(),
        json!({ "registrationEnabled": false, "passkeysAvailable": false, "publicPagesEnabled": true })
    );
}

#[sqlx::test]
async fn without_a_usable_public_url_every_passkey_route_is_503_and_totp_keeps_working(
    pool: PgPool,
) {
    for public_url in [
        "http://127.0.0.1:4200",
        "http://192.168.1.10",
        "http://git.example.com",
    ] {
        let server = spawn_with(
            pool.clone(),
            Options {
                public_url: public_url.to_string(),
                ..Options::default()
            },
            |_| {},
        )
        .await;
        let name = format!("user{}", server.addr.port());
        server.new_user(&name).await;
        let id = server.user_id(&name).await;
        let token = server.pending_token(&id).await;
        let some_id = uuid::Uuid::new_v4();

        for (path, body) in [
            ("/auth/mfa/passkey/start", json!({ "mfaToken": token })),
            (
                "/auth/mfa/passkey/finish",
                json!({ "mfaToken": token, "challengeId": some_id, "credential": {} }),
            ),
            (
                "/auth/mfa/setup/passkey/start",
                json!({ "mfaToken": token }),
            ),
            (
                "/auth/mfa/setup/passkey/finish",
                json!({ "mfaToken": token, "challengeId": some_id, "credential": {}, "name": "Clé" }),
            ),
        ] {
            assert_error(server.post(path, body).await, 503, UNAVAILABLE).await;
        }

        let enrolled = server.first_setup_with_totp(&name, USER_PASSWORD).await;
        assert_eq!(
            server.me(&enrolled.session).await.status(),
            200,
            "{public_url}"
        );
        let token = server.mfa_token(&name, USER_PASSWORD).await;
        let secret = enrolled.secret.unwrap();
        assert_eq!(
            server
                .post(
                    "/auth/mfa/verify",
                    json!({ "mfaToken": token, "code": next_step_code(&secret) })
                )
                .await
                .status(),
            200
        );
        assert_error(
            server
                .register_start(&enrolled.session, USER_PASSWORD)
                .await,
            503,
            UNAVAILABLE,
        )
        .await;
        assert_error(
            server
                .register_finish(&enrolled.session, &some_id.to_string(), &json!({}), "Clé")
                .await,
            503,
            UNAVAILABLE,
        )
        .await;
        assert_eq!(
            server.status(&enrolled.session).await["passkeys"],
            json!([])
        );
        // What already exists can always be removed, even where no ceremony can run any more.
        let seeded = uuid::Uuid::new_v4();
        let user_id: uuid::Uuid = id.parse().unwrap();
        let stored = ferrisgit_domain::webauthn::StoredPasskey {
            id: seeded,
            user_id,
            name: "Ancienne clé".to_string(),
            credential_id: seeded.as_bytes().to_vec(),
            passkey_json: "{}".to_string(),
            created_at: chrono::Utc::now(),
            last_used_at: None,
        };
        assert!(
            server
                .state
                .passkey_credentials
                .insert(&stored)
                .await
                .unwrap()
        );
        assert_eq!(
            server.status(&enrolled.session).await["passkeys"][0]["name"],
            json!("Ancienne clé")
        );
        assert_eq!(
            server
                .delete_passkey(&enrolled.session, &seeded.to_string(), USER_PASSWORD)
                .await
                .status(),
            204,
            "{public_url}"
        );
        assert_eq!(server.passkey_rows(&id).await, 0);
        let config: Value = server
            .client
            .get(server.url("/auth/config"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(config["passkeysAvailable"], json!(false), "{public_url}");
    }
}

#[sqlx::test]
async fn the_unauthenticated_routes_cap_the_body_at_16_kib(pool: PgPool) {
    let server = spawn_server(pool).await;
    let big = format!("{{\"mfaToken\":\"{}\"}}", "a".repeat(17 * 1024));
    for path in [
        "/auth/mfa/passkey/start",
        "/auth/mfa/passkey/finish",
        "/auth/mfa/setup/passkey/start",
        "/auth/mfa/setup/passkey/finish",
    ] {
        let res = server.post_raw(path, big.clone().into_bytes()).await;
        assert_eq!(res.status(), 413, "{path}");
    }
}

#[sqlx::test]
async fn garbage_is_a_400_never_a_500_and_a_forged_assertion_is_a_401(pool: PgPool) {
    let server = spawn_unthrottled(pool).await;
    server.new_user("alice").await;
    let mut device = Device::new();
    let alice = server
        .first_setup_with_passkey(&mut device, "alice", USER_PASSWORD, "Clé")
        .await;
    let token = server.pending_token(&alice.id).await;
    let (challenge_id, public_key) = server.start_login_challenge(&token).await;
    let genuine = device.assert(&public_key);
    let some_id = uuid::Uuid::new_v4().to_string();

    let routes = [
        "/auth/mfa/passkey/start",
        "/auth/mfa/passkey/finish",
        "/auth/mfa/setup/passkey/start",
        "/auth/mfa/setup/passkey/finish",
    ];
    for path in routes {
        for body in [
            &b"{"[..],
            b"",
            b"[]",
            b"null",
            b"\"text\"",
            b"{}",
            b"{\"mfaToken\":42}",
            b"\xff\xfe\x00",
        ] {
            let res = server.post_raw(path, body.to_vec()).await;
            assert_eq!(
                res.status(),
                400,
                "{path} with {:?}",
                String::from_utf8_lossy(body)
            );
            assert!(res.json::<Value>().await.unwrap()["error"].is_string());
        }
        let res = server
            .client
            .post(server.url(path))
            .body("{}")
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400, "{path} without a content type");
    }
    for credential in [
        json!(null),
        json!(42),
        json!("text"),
        json!([]),
        json!({}),
        json!({ "id": 1 }),
        json!({ "id": "x", "rawId": "x", "type": "public-key" }),
        json!({ "id": "x", "rawId": "x", "response": 5, "type": "public-key" }),
    ] {
        let res = server
            .passkey_finish(&token, &challenge_id, &credential)
            .await;
        assert_eq!(res.status(), 400, "assertion credential {credential}");
        let res = server
            .setup_finish(&token, &challenge_id, &credential, "Clé")
            .await;
        assert_eq!(res.status(), 400, "registration credential {credential}");
    }
    assert_eq!(
        server
            .post(
                "/auth/mfa/passkey/finish",
                json!({ "mfaToken": token, "challengeId": "nope", "credential": genuine })
            )
            .await
            .status(),
        400
    );
    assert_eq!(server.post("/auth/mfa/setup/passkey/finish", json!({ "mfaToken": token, "challengeId": some_id, "credential": genuine, "name": 7 })).await.status(), 400);

    assert_eq!(
        server
            .passkey_finish(&token, &challenge_id, &genuine)
            .await
            .status(),
        200
    );

    let token = server.pending_token(&alice.id).await;
    let (challenge_id, public_key) = server.start_login_challenge(&token).await;
    let mut garbage = device.assert(&public_key);
    garbage["response"]["clientDataJSON"] = json!("e30"); // "{}"
    assert_refused(server.passkey_finish(&token, &challenge_id, &garbage).await).await;
    let (challenge_id, public_key) = server.start_login_challenge(&token).await;
    let mut phished = device.assert(&public_key);
    phished["response"]["authenticatorData"] = json!("AAAA");
    assert_refused(server.passkey_finish(&token, &challenge_id, &phished).await).await;
}

#[sqlx::test]
async fn a_registration_answered_for_another_challenge_is_a_400_and_stores_nothing(pool: PgPool) {
    let server = spawn_unthrottled(pool).await;
    server.new_user("alice").await;
    server.new_user("bob").await;
    let alice = server.user_id("alice").await;
    let bob = server.user_id("bob").await;
    let alices_token = server.pending_token(&alice).await;
    let bobs_token = server.pending_token(&bob).await;
    let (alices_challenge, alices_key) = server.start_setup_challenge(&alices_token).await;
    let (bobs_challenge, _) = server.start_setup_challenge(&bobs_token).await;
    let credential = Device::new().register(&alices_key);

    assert_error(
        server
            .setup_finish(&bobs_token, &bobs_challenge, &credential, "Clé")
            .await,
        400,
        "invalid passkey",
    )
    .await;
    assert_error(
        server
            .setup_finish(&bobs_token, &alices_challenge, &credential, "Clé")
            .await,
        400,
        "invalid passkey",
    )
    .await;
    assert_error(
        server
            .setup_finish(
                &alices_token,
                &uuid::Uuid::new_v4().to_string(),
                &credential,
                "Clé",
            )
            .await,
        400,
        "invalid passkey",
    )
    .await;
    assert_eq!(
        server.passkey_rows(&alice).await + server.passkey_rows(&bob).await,
        0
    );

    assert_eq!(
        server
            .setup_finish(&alices_token, &alices_challenge, &credential, "Clé")
            .await
            .status(),
        200
    );
}
