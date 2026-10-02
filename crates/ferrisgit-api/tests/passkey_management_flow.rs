// Self-service passkey management, the rules for removing a factor, and how they affect the admin reset and users list.

mod common;

use common::*;
use ferrisgit_application::email_templates;
use serde_json::{Value, json};
use sqlx::PgPool;

const INVALID_CODE: &str = "invalid code";
const INVALID_TOKEN: &str = "invalid or expired token";
const TOO_MANY: &str = "too many attempts, try again later";
const WRONG_PASSWORD_MESSAGE: &str = "current password is incorrect";

#[sqlx::test]
async fn status_lists_the_passkeys_and_last_used_at_follows_the_logins(pool: PgPool) {
    let server = spawn_server(pool).await;
    server.new_user("alice").await;
    server.new_user("bob").await;
    let mut macbook = Device::new();
    let mut iphone = Device::new();
    let alice = server
        .first_setup_with_passkey(&mut macbook, "alice", USER_PASSWORD, "MacBook")
        .await;
    server
        .add_passkey(&alice.session, &mut iphone, USER_PASSWORD, "iPhone")
        .await;

    let status = server.status(&alice.session).await;
    assert_eq!(
        keys(&status),
        key_set(&["totpEnabled", "backupCodesRemaining", "passkeys"])
    );
    assert_eq!(
        (
            status["totpEnabled"].clone(),
            status["backupCodesRemaining"].clone()
        ),
        (json!(false), json!(10))
    );
    let listed = status["passkeys"].as_array().unwrap();
    assert_eq!(
        listed
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["MacBook", "iPhone"],
        "oldest first"
    );
    for passkey in listed {
        assert_eq!(
            keys(passkey),
            key_set(&["id", "name", "createdAt", "lastUsedAt"])
        );
        assert!(
            passkey["lastUsedAt"].is_null(),
            "never used to log in yet: {passkey}"
        );
        assert!(uuid::Uuid::parse_str(passkey["id"].as_str().unwrap()).is_ok());
        assert!(
            chrono::DateTime::parse_from_rfc3339(passkey["createdAt"].as_str().unwrap()).is_ok()
        );
    }

    let session = server
        .login_with_passkey(&mut iphone, "alice", USER_PASSWORD)
        .await;
    let status = server.status(&session).await;
    assert!(
        status["passkeys"][0]["lastUsedAt"].is_null(),
        "the MacBook was not used"
    );
    assert!(
        chrono::DateTime::parse_from_rfc3339(status["passkeys"][1]["lastUsedAt"].as_str().unwrap())
            .is_ok(),
        "the iPhone was: {status}"
    );

    let bob = server.first_setup_with_totp("bob", USER_PASSWORD).await;
    assert_eq!(
        server.status(&bob.session).await,
        json!({ "totpEnabled": true, "backupCodesRemaining": 10, "passkeys": [] })
    );
}

#[sqlx::test]
async fn every_self_service_passkey_endpoint_needs_a_session(pool: PgPool) {
    let server = spawn_server(pool).await;
    server.new_user("alice").await;
    let alice = server
        .first_setup_with_passkey(&mut Device::new(), "alice", USER_PASSWORD, "Clé")
        .await;
    let pending = server.mfa_token("alice", USER_PASSWORD).await;
    let passkey_id = server.status(&alice.session).await["passkeys"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    for path in [
        "/me/mfa/passkeys/register/start".to_string(),
        "/me/mfa/passkeys/register/finish".to_string(),
        format!("/me/mfa/passkeys/{passkey_id}/delete"),
    ] {
        let body = json!({ "currentPassword": USER_PASSWORD, "challengeId": uuid::Uuid::new_v4(), "credential": {}, "name": "x" });
        assert_eq!(
            server
                .client
                .post(server.url(&path))
                .json(&body)
                .send()
                .await
                .unwrap()
                .status(),
            401,
            "{path} without a session"
        );
        assert_eq!(
            server.post_as(&pending, &path, body).await.status(),
            401,
            "{path} with an mfaToken"
        );
    }
    assert_eq!(
        server.status(&alice.session).await["passkeys"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "nothing was deleted"
    );
}

#[sqlx::test]
async fn registration_start_needs_the_password_and_the_per_user_budget_comes_first(pool: PgPool) {
    let server = spawn_server(pool).await;
    server.new_user("alice").await;
    let mut device = Device::new();
    let alice = server
        .first_setup_with_passkey(&mut device, "alice", USER_PASSWORD, "MacBook")
        .await;
    // Reset the budget by hand, so slow password hashing can't roll the window over mid-test.
    server.reset_budget(&alice.id);

    assert_error(
        server.register_start(&alice.session, WRONG_PASSWORD).await,
        400,
        WRONG_PASSWORD_MESSAGE,
    )
    .await; // attempt 1
    let start = server.register_start(&alice.session, USER_PASSWORD).await; // attempt 2
    assert_eq!(start.status(), 200);
    let start: Value = start.json().await.unwrap();
    assert_eq!(keys(&start), key_set(&["challengeId", "publicKey"]));
    assert!(
        start["publicKey"].get("publicKey").is_none(),
        "the inner publicKey object"
    );
    let excluded: Vec<&str> = start["publicKey"]["excludeCredentials"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        excluded,
        server
            .stored_credential_ids(&alice.id)
            .await
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        "the authenticators that already hold a passkey are excluded"
    );

    for _ in 3..=10 {
        assert_error(
            server.register_start(&alice.session, WRONG_PASSWORD).await,
            400,
            WRONG_PASSWORD_MESSAGE,
        )
        .await;
    }
    assert_error(
        server.register_start(&alice.session, USER_PASSWORD).await,
        429,
        TOO_MANY,
    )
    .await;
    assert_error(
        server.register_start(&alice.session, WRONG_PASSWORD).await,
        429,
        TOO_MANY,
    )
    .await;
    server.reset_budget(&alice.id);
    let (id, _) = server
        .add_passkey(&alice.session, &mut Device::new(), USER_PASSWORD, "Autre")
        .await;
    assert!(uuid::Uuid::parse_str(&id).is_ok());
}

#[sqlx::test]
async fn registration_finish_stores_the_passkey_sends_the_mail_and_refuses_replays(pool: PgPool) {
    let server = spawn_unthrottled(pool).await;
    server.new_user("alice").await;
    server.new_user("bob").await;
    let mut macbook = Device::new();
    let alice = server
        .first_setup_with_passkey(&mut macbook, "alice", USER_PASSWORD, "MacBook")
        .await;
    let bob = server
        .first_setup_with_passkey(&mut Device::new(), "bob", USER_PASSWORD, "Clé de Bob")
        .await;
    wait_for_attempts(&server.mailer, 2).await; // the two setup mails

    let start: Value = server
        .register_start(&alice.session, USER_PASSWORD)
        .await
        .json()
        .await
        .unwrap();
    let challenge_id = start["challengeId"].as_str().unwrap().to_string();
    let mut iphone = Device::new();
    let credential = iphone.register(&start["publicKey"]);

    assert_error(
        server
            .register_finish(&alice.session, &challenge_id, &credential, "  ")
            .await,
        400,
        "invalid passkey name",
    )
    .await;
    assert_error(
        server
            .register_finish(&alice.session, &challenge_id, &credential, &"x".repeat(41))
            .await,
        400,
        "invalid passkey name",
    )
    .await;
    assert_eq!(server.passkey_rows(&alice.id).await, 1);

    let res = server
        .register_finish(&alice.session, &challenge_id, &credential, " iPhone ")
        .await;
    assert_eq!(res.status(), 201);
    let created: Value = res.json().await.unwrap();
    assert_eq!(
        keys(&created),
        key_set(&["id", "name", "createdAt", "lastUsedAt"])
    );
    assert_eq!(created["name"], json!("iPhone"));
    assert!(created["lastUsedAt"].is_null());
    assert_eq!(server.passkey_rows(&alice.id).await, 2);
    assert_eq!(
        server.status(&alice.session).await["passkeys"][1]["id"],
        created["id"]
    );
    assert_eq!(
        server
            .security_events_of(&alice.id)
            .await
            .iter()
            .filter(|e| *e == "PasskeyAdded")
            .count(),
        2
    );
    assert_eq!(server.me(&alice.session).await.status(), 200);
    assert_eq!(server.backup_code_rows(&alice.id).await, 10);

    wait_for_attempts(&server.mailer, 3).await;
    let expected = email_templates::mfa_enrolled("alice", PASSKEY_METHOD_LABEL);
    assert_eq!(
        server.mailer.sent()[2],
        ("alice@example.com".to_string(), expected.subject)
    );
    assert!(server.mailer.texts()[2].contains("une clé d'accès (passkey)"));

    assert_error(
        server
            .register_finish(&alice.session, &challenge_id, &credential, "Encore")
            .await,
        400,
        "invalid passkey",
    )
    .await;
    let second: Value = server
        .register_start(&alice.session, USER_PASSWORD)
        .await
        .json()
        .await
        .unwrap();
    assert_error(
        server
            .register_finish(
                &alice.session,
                second["challengeId"].as_str().unwrap(),
                &credential,
                "Encore",
            )
            .await,
        400,
        "invalid passkey",
    )
    .await;
    let bobs: Value = server
        .register_start(&bob.session, USER_PASSWORD)
        .await
        .json()
        .await
        .unwrap();
    let fresh = iphone.register(&second["publicKey"]);
    assert_error(
        server
            .register_finish(
                &alice.session,
                bobs["challengeId"].as_str().unwrap(),
                &fresh,
                "Vol",
            )
            .await,
        400,
        "invalid passkey",
    )
    .await;
    assert_error(
        server
            .register_finish(
                &alice.session,
                &uuid::Uuid::new_v4().to_string(),
                &fresh,
                "Vol",
            )
            .await,
        400,
        "invalid passkey",
    )
    .await;
    assert_eq!(server.passkey_rows(&alice.id).await, 2);
    assert_eq!(server.passkey_rows(&bob.id).await, 1);
    assert_eq!(
        settled_attempts(&server.mailer).await,
        3,
        "refused registrations send no mail"
    );
}

#[sqlx::test]
async fn a_credential_id_that_already_exists_is_a_409_and_stores_nothing(pool: PgPool) {
    // A duplicate credential_id (same authenticator, or another user's) is a 409. The soft authenticator always makes a
    // fresh key, so the fake store is told a row with that id already exists.
    let (server, sabotage) = spawn_sabotaged(pool).await;
    server.new_user("alice").await;
    let alice = server
        .first_setup_with_passkey(&mut Device::new(), "alice", USER_PASSWORD, "MacBook")
        .await;
    wait_for_attempts(&server.mailer, 1).await;

    let start: Value = server
        .register_start(&alice.session, USER_PASSWORD)
        .await
        .json()
        .await
        .unwrap();
    let credential = Device::new().register(&start["publicKey"]);
    sabotage
        .refuse_insert
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let res = server
        .register_finish(
            &alice.session,
            start["challengeId"].as_str().unwrap(),
            &credential,
            "Copie",
        )
        .await;
    assert_error(res, 409, "this passkey is already registered").await;
    assert_eq!(server.passkey_rows(&alice.id).await, 1);
    assert_eq!(
        settled_attempts(&server.mailer).await,
        1,
        "no mail for a registration that did not happen"
    );
}

#[sqlx::test]
async fn deleting_a_passkey_needs_the_password_signs_the_user_out_and_is_scoped_to_the_owner(
    pool: PgPool,
) {
    let server = spawn_unthrottled(pool).await;
    server.new_user("alice").await;
    server.new_user("bob").await;
    let mut macbook = Device::new();
    let mut iphone = Device::new();
    let alice = server
        .first_setup_with_passkey(&mut macbook, "alice", USER_PASSWORD, "MacBook")
        .await;
    let bob = server
        .first_setup_with_passkey(&mut Device::new(), "bob", USER_PASSWORD, "Clé de Bob")
        .await;
    let (iphone_id, _) = server
        .add_passkey(&alice.session, &mut iphone, USER_PASSWORD, "iPhone")
        .await;
    let bobs_passkey = server.status(&bob.session).await["passkeys"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    assert_error(
        server
            .delete_passkey(&alice.session, &iphone_id, WRONG_PASSWORD)
            .await,
        400,
        WRONG_PASSWORD_MESSAGE,
    )
    .await;
    assert_eq!(
        server
            .post_as(
                &alice.session,
                &format!("/me/mfa/passkeys/{iphone_id}/delete"),
                json!({})
            )
            .await
            .status(),
        400,
        "no password at all"
    );
    assert_eq!(server.passkey_rows(&alice.id).await, 2);
    assert_eq!(server.me(&alice.session).await.status(), 200);

    assert_error(
        server
            .delete_passkey(&alice.session, &bobs_passkey, USER_PASSWORD)
            .await,
        404,
        "passkey",
    )
    .await;
    assert_error(
        server
            .delete_passkey(
                &alice.session,
                &uuid::Uuid::new_v4().to_string(),
                USER_PASSWORD,
            )
            .await,
        404,
        "passkey",
    )
    .await;
    assert_eq!(
        server
            .delete_passkey(&alice.session, "not-a-uuid", USER_PASSWORD)
            .await
            .status(),
        400
    );
    assert_eq!(
        server.passkey_rows(&bob.id).await,
        1,
        "Bob's passkey is intact"
    );
    assert_eq!(
        server.me(&alice.session).await.status(),
        200,
        "a refused delete does not sign anybody out"
    );
    assert_eq!(server.me(&bob.session).await.status(), 200);

    let res = server
        .delete_passkey(&alice.session, &iphone_id, USER_PASSWORD)
        .await;
    assert_eq!(res.status(), 204);
    assert!(res.bytes().await.unwrap().is_empty());
    assert_eq!(
        server.me(&alice.session).await.status(),
        401,
        "the epoch was bumped"
    );
    assert_eq!(
        server.me(&bob.session).await.status(),
        200,
        "only the owner is signed out"
    );
    assert_eq!(server.passkey_rows(&alice.id).await, 1);
    assert!(
        server
            .security_events_of(&alice.id)
            .await
            .contains(&"PasskeyDeleted".to_string())
    );
    assert_eq!(server.backup_code_rows(&alice.id).await, 10);
    let session = server
        .login_with_passkey(&mut macbook, "alice", USER_PASSWORD)
        .await;
    let status = server.status(&session).await;
    assert_eq!(status["passkeys"].as_array().unwrap().len(), 1);
    assert_eq!(status["passkeys"][0]["name"], json!("MacBook"));
    let token = server.mfa_token("alice", USER_PASSWORD).await;
    let (_, public_key) = server.start_login_challenge(&token).await;
    let allowed: Vec<&str> = public_key["allowCredentials"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(allowed.len(), 1);
    assert!(
        iphone.try_assert(&public_key).is_none(),
        "the iPhone's credential is not among the allowed ones any more"
    );
}

#[sqlx::test]
async fn removing_the_last_passkey_leaves_the_user_forced_into_setup_and_kills_the_codes(
    pool: PgPool,
) {
    let server = spawn_server(pool).await;
    server.new_user("alice").await;
    let alice = server
        .first_setup_with_passkey(&mut Device::new(), "alice", USER_PASSWORD, "MacBook")
        .await;
    let passkey_id = server.status(&alice.session).await["passkeys"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let old_codes = alice.backup_codes.clone();

    assert_eq!(
        server
            .delete_passkey(&alice.session, &passkey_id, USER_PASSWORD)
            .await
            .status(),
        204
    );
    assert_eq!(server.passkey_rows(&alice.id).await, 0);
    assert_eq!(
        server.backup_code_rows(&alice.id).await,
        0,
        "no factor left: no dormant codes either"
    );

    let login = server.login("alice", USER_PASSWORD).await;
    assert_eq!(
        (
            login["mfaSetupRequired"].clone(),
            login["mfaHasTotp"].clone(),
            login["mfaHasPasskey"].clone()
        ),
        (json!(true), json!(false), json!(false))
    );
    let token = login["mfaToken"].as_str().unwrap();
    assert_error(
        server
            .post(
                "/auth/mfa/verify",
                json!({ "mfaToken": token, "backupCode": old_codes[0] }),
            )
            .await,
        401,
        INVALID_CODE,
    )
    .await;
    assert_error(
        server.passkey_start(token).await,
        400,
        "no passkey is registered",
    )
    .await;

    let again = server
        .first_setup_with_passkey(&mut Device::new(), "alice", USER_PASSWORD, "Nouvelle clé")
        .await;
    assert_eq!(again.backup_codes.len(), 10);
    assert!(
        again
            .backup_codes
            .iter()
            .all(|code| !old_codes.contains(code)),
        "brand new codes"
    );
    assert_eq!(server.me(&again.session).await.status(), 200);
}

#[sqlx::test]
async fn removing_a_passkey_while_a_totp_remains_keeps_the_backup_codes(pool: PgPool) {
    let server = spawn_server(pool).await;
    server.new_user("alice").await;
    let alice = server.first_setup_with_totp("alice", USER_PASSWORD).await;
    let secret = alice.secret.clone().unwrap();
    let (passkey_id, _) = server
        .add_passkey(&alice.session, &mut Device::new(), USER_PASSWORD, "Clé")
        .await;
    let hashes = server.backup_code_hashes(&alice.id).await;

    assert_eq!(
        server
            .delete_passkey(&alice.session, &passkey_id, USER_PASSWORD)
            .await
            .status(),
        204
    );
    assert_eq!(
        server.backup_code_hashes(&alice.id).await,
        hashes,
        "the codes stay: the TOTP is still a factor"
    );
    let login = server.login("alice", USER_PASSWORD).await;
    assert_eq!(
        (
            login["mfaSetupRequired"].clone(),
            login["mfaHasTotp"].clone(),
            login["mfaHasPasskey"].clone()
        ),
        (json!(false), json!(true), json!(false))
    );
    let token = login["mfaToken"].as_str().unwrap();
    let res = server
        .post(
            "/auth/mfa/verify",
            json!({ "mfaToken": token, "code": next_step_code(&secret) }),
        )
        .await;
    assert_eq!(res.status(), 200);
    let session = res.json::<Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        server.status(&session).await,
        json!({ "totpEnabled": true, "backupCodesRemaining": 10, "passkeys": [] })
    );
}

#[sqlx::test]
async fn disabling_the_totp_while_a_passkey_remains_keeps_the_codes_and_the_passkey(pool: PgPool) {
    let server = spawn_server(pool).await;
    server.new_user("alice").await;
    let alice = server.first_setup_with_totp("alice", USER_PASSWORD).await;
    let secret = alice.secret.clone().unwrap();
    let mut device = Device::new();
    server
        .add_passkey(&alice.session, &mut device, USER_PASSWORD, "Clé")
        .await;
    let hashes = server.backup_code_hashes(&alice.id).await;

    assert_eq!(
        server
            .disable_totp(&alice.session, USER_PASSWORD)
            .await
            .status(),
        204
    );
    assert_eq!(
        server.me(&alice.session).await.status(),
        401,
        "signed out everywhere, as ever"
    );
    assert_eq!(server.totp_rows(&alice.id).await, 0);
    assert_eq!(server.passkey_rows(&alice.id).await, 1, "the passkey stays");
    assert_eq!(
        server.backup_code_hashes(&alice.id).await,
        hashes,
        "so do the backup codes"
    );

    let login = server.login("alice", USER_PASSWORD).await;
    assert_eq!(
        (
            login["mfaSetupRequired"].clone(),
            login["mfaHasTotp"].clone(),
            login["mfaHasPasskey"].clone()
        ),
        (json!(false), json!(false), json!(true))
    );
    let token = login["mfaToken"].as_str().unwrap();
    assert_error(
        server
            .post(
                "/auth/mfa/verify",
                json!({ "mfaToken": token, "code": next_step_code(&secret) }),
            )
            .await,
        401,
        INVALID_CODE,
    )
    .await;
    let session = server
        .login_with_passkey(&mut device, "alice", USER_PASSWORD)
        .await;
    assert_eq!(
        server.status(&session).await["backupCodesRemaining"],
        json!(10)
    );
}

#[sqlx::test]
async fn backup_codes_can_be_regenerated_by_a_passkey_only_user_with_the_password(pool: PgPool) {
    let server = spawn_server(pool).await;
    server.new_user("alice").await;
    let alice = server
        .first_setup_with_passkey(&mut Device::new(), "alice", USER_PASSWORD, "Clé")
        .await;
    let before = server.backup_code_hashes(&alice.id).await;

    let path = "/me/mfa/backup-codes/regenerate";
    assert_error(
        server
            .post_as(
                &alice.session,
                path,
                json!({ "currentPassword": WRONG_PASSWORD }),
            )
            .await,
        400,
        WRONG_PASSWORD_MESSAGE,
    )
    .await;
    assert_eq!(server.backup_code_hashes(&alice.id).await, before);
    let res = server
        .post_as(
            &alice.session,
            path,
            json!({ "currentPassword": USER_PASSWORD }),
        )
        .await;
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(keys(&body), key_set(&["backupCodes"]));
    assert_eq!(codes_of(&body).len(), 10);
    assert_ne!(
        server.backup_code_hashes(&alice.id).await,
        before,
        "the set was replaced"
    );
}

#[sqlx::test]
async fn a_failed_removal_still_leaves_the_sessions_dead_and_the_passkey_intact(pool: PgPool) {
    // The epoch bump comes before the removal: if the removal fails, the sessions are dead and the factor intact, which is
    // harmless. The reverse would leave a stolen session alive with the factor gone.
    let (server, sabotage) = spawn_sabotaged(pool).await;
    server.new_user("alice").await;
    let mut macbook = Device::new();
    let alice = server
        .first_setup_with_passkey(&mut macbook, "alice", USER_PASSWORD, "MacBook")
        .await;
    let (iphone_id, _) = server
        .add_passkey(&alice.session, &mut Device::new(), USER_PASSWORD, "iPhone")
        .await;

    sabotage
        .fail_delete
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert_error(
        server
            .delete_passkey(&alice.session, &iphone_id, USER_PASSWORD)
            .await,
        500,
        "internal error",
    )
    .await;
    assert_eq!(
        server.me(&alice.session).await.status(),
        401,
        "the epoch was bumped before the failing removal"
    );
    assert_eq!(
        server.passkey_rows(&alice.id).await,
        2,
        "the passkey is intact"
    );

    sabotage
        .fail_delete
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let session = server
        .login_with_passkey(&mut macbook, "alice", USER_PASSWORD)
        .await;
    assert_eq!(
        server
            .delete_passkey(&session, &iphone_id, USER_PASSWORD)
            .await
            .status(),
        204
    );
    assert_eq!(server.passkey_rows(&alice.id).await, 1);
}

#[sqlx::test]
async fn the_users_list_counts_a_passkey_as_mfa_and_an_admin_reset_removes_it(pool: PgPool) {
    let server = spawn_server(pool).await;
    for name in ["alice", "bob", "carol", "dave"] {
        server.new_user(name).await;
    }
    let admin = server
        .first_setup_with_passkey(&mut Device::new(), "admin", ADMIN_PASSWORD, "Clé admin")
        .await;
    let alice = server
        .first_setup_with_passkey(&mut Device::new(), "alice", USER_PASSWORD, "MacBook")
        .await;
    server.first_setup_with_totp("bob", USER_PASSWORD).await;
    let dave_token = server.mfa_token("dave", USER_PASSWORD).await;
    assert_eq!(
        server
            .post(
                "/auth/mfa/setup/totp/enroll",
                json!({ "mfaToken": dave_token })
            )
            .await
            .status(),
        200
    );

    let listing = server.admin_users(&admin.session).await;
    let rows = listing.as_array().unwrap();
    for row in rows {
        assert_eq!(
            keys(row),
            key_set(&[
                "id",
                "username",
                "email",
                "isAdmin",
                "createdAt",
                "state",
                "invitationExpiresAt",
                "mfaEnabled"
            ])
        );
    }
    let enabled = |listing: &Value, name: &str| {
        listing
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["username"] == json!(name))
            .unwrap()["mfaEnabled"]
            .clone()
    };
    assert_eq!(
        ["admin", "alice", "bob", "carol", "dave"].map(|name| enabled(&listing, name)),
        [
            json!(true),
            json!(true),
            json!(true),
            json!(false),
            json!(false)
        ],
        "a passkey-only user, a TOTP user: enabled; nothing or an unconfirmed TOTP: not"
    );

    let stale_token = server.mfa_token("alice", USER_PASSWORD).await;

    let res = server.admin_reset(&admin.session, &alice.id).await;
    assert_eq!(res.status(), 204);
    assert_eq!(
        server.passkey_rows(&alice.id).await,
        0,
        "the reset removes the passkeys too"
    );
    assert_eq!(server.backup_code_rows(&alice.id).await, 0);
    assert_eq!(
        enabled(&server.admin_users(&admin.session).await, "alice"),
        json!(false)
    );
    assert_eq!(
        server.me(&alice.session).await.status(),
        401,
        "Alice's sessions are dead"
    );
    assert_eq!(
        server.me(&admin.session).await.status(),
        200,
        "the admin's own is not"
    );
    assert_error(server.passkey_start(&stale_token).await, 401, INVALID_TOKEN).await;
    assert!(
        server
            .security_events_of(&alice.id)
            .await
            .contains(&"MfaResetByAdmin".to_string())
    );
    let bobs_row = enabled(&server.admin_users(&admin.session).await, "bob");
    assert_eq!(bobs_row, json!(true), "the reset touched Alice only");

    let login = server.login("alice", USER_PASSWORD).await;
    assert_eq!(
        (
            login["mfaSetupRequired"].clone(),
            login["mfaHasPasskey"].clone()
        ),
        (json!(true), json!(false))
    );
    wait_for_attempts(&server.mailer, 3).await;
    let subjects: Vec<String> = server
        .mailer
        .sent()
        .into_iter()
        .filter(|(to, _)| to == "alice@example.com")
        .map(|(_, subject)| subject)
        .collect();
    assert!(
        subjects.contains(&email_templates::mfa_reset("alice").subject),
        "{subjects:?}"
    );
}

#[sqlx::test]
async fn no_response_carries_a_secret_or_a_serialized_passkey(pool: PgPool) {
    let server = spawn_server(pool).await;
    server.new_user("alice").await;
    let mut device = Device::new();
    let admin = server
        .first_setup_with_passkey(&mut Device::new(), "admin", ADMIN_PASSWORD, "Clé admin")
        .await;

    let login = server
        .client
        .post(server.url("/auth/login"))
        .json(&json!({ "username": "alice", "password": USER_PASSWORD }))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let mfa_token = serde_json::from_str::<Value>(&login).unwrap()["mfaToken"]
        .as_str()
        .unwrap()
        .to_string();
    let (challenge_id, public_key) = server.start_setup_challenge(&mfa_token).await;
    let credential = device.register(&public_key);
    let finish = server
        .setup_finish(&mfa_token, &challenge_id, &credential, "MacBook")
        .await
        .text()
        .await
        .unwrap();
    let session = serde_json::from_str::<Value>(&finish).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let alice = server.user_id("alice").await;

    let mut bodies = vec![login.clone(), finish];
    bodies.push(
        server
            .get_as(&session, "/me/mfa")
            .await
            .text()
            .await
            .unwrap(),
    );
    server
        .add_passkey(&session, &mut Device::new(), USER_PASSWORD, "iPhone")
        .await;
    let start = server.register_start(&session, USER_PASSWORD).await;
    let start_text = start.text().await.unwrap();
    let start_json: Value = serde_json::from_str(&start_text).unwrap();
    let credential = Device::new().register(&start_json["publicKey"]);
    bodies.push(
        server
            .register_finish(
                &session,
                start_json["challengeId"].as_str().unwrap(),
                &credential,
                "Encore",
            )
            .await
            .text()
            .await
            .unwrap(),
    );
    bodies.push(
        server
            .get_as(&admin.session, "/admin/users")
            .await
            .text()
            .await
            .unwrap(),
    );
    let token = server.mfa_token("alice", USER_PASSWORD).await;
    let (challenge_id, public_key) = server.start_login_challenge(&token).await;
    let assertion = device.assert(&public_key);
    bodies.push(
        server
            .passkey_finish(&token, &challenge_id, &assertion)
            .await
            .text()
            .await
            .unwrap(),
    );
    bodies.push(
        server
            .passkey_finish(&token, &challenge_id, &assertion)
            .await
            .text()
            .await
            .unwrap(),
    ); // an error body

    let credential_ids = server.stored_credential_ids(&alice).await;
    assert_eq!(credential_ids.len(), 3);
    for body in &bodies {
        for id in &credential_ids {
            assert!(
                !body.contains(id.as_str()),
                "a credential id leaked into a response: {body}"
            );
        }
        for forbidden in [
            "\"counter\"",
            "\"cred\"",
            "attestation",
            "passkey_json",
            "\"backup_eligible\"",
            "\"backup_state\"",
            "\"passkey\":",
            "password",
        ] {
            assert!(
                !body.contains(forbidden),
                "{forbidden} in a response: {body}"
            );
        }
    }
}
