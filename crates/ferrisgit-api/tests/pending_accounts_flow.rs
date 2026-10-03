// Accounts nobody activated: a reminder with a fresh link whenever the previous one has expired, and deletion after seven
// days. The sweep takes its clock from the test, so days pass in an instant, while the API and the database are real.

mod common;

use common::{ADMIN_PASSWORD, activation_link, token_of, totp_code};

use chrono::{DateTime, Duration, Utc};
use ferrisgit_application::use_cases::sweep_pending_accounts::SweepReport;
use serde_json::{Value, json};
use sqlx::PgPool;

const PASSWORD: &str = "correct-horse-battery";
const CONFIRMATION: &str = "Confirmez votre inscription à FerrisGit";
const REMINDER: &str = "Votre compte FerrisGit n'est pas encore activé";

async fn admin_session(server: &common::Server) -> String {
    let body = server.login("admin", ADMIN_PASSWORD).await;
    let mfa_token = body["mfaToken"].as_str().unwrap().to_string();
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
    let res = server
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

async fn configure_smtp(server: &common::Server, admin: &str) {
    let smtp = server
        .client
        .put(server.url("/admin/settings/smtp"))
        .bearer_auth(admin)
        .json(&json!({
            "host": "smtp.example.com", "port": 587, "security": "starttls",
            "fromAddress": "noreply@example.com", "fromName": "FerrisGit"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(smtp.status(), 200);
}

/// Registration on and mail configured, so that people can sign up. Returns the admin's session.
async fn open_registration(server: &common::Server) -> String {
    let admin = admin_session(server).await;
    configure_smtp(server, &admin).await;
    let switch = server
        .client
        .put(server.url("/admin/settings"))
        .bearer_auth(&admin)
        .json(&json!({ "registrationEnabled": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(switch.status(), 200);
    admin
}

async fn register(server: &common::Server, username: &str) -> reqwest::Response {
    server
        .post(
            "/auth/register",
            json!({ "username": username, "email": format!("{username}@example.com") }),
        )
        .await
}

async fn activate(server: &common::Server, token: &str) -> reqwest::Response {
    server
        .post(
            "/auth/activate",
            json!({ "token": token, "password": PASSWORD }),
        )
        .await
}

async fn login_status(server: &common::Server, username: &str, password: &str) -> u16 {
    server
        .post(
            "/auth/login",
            json!({ "username": username, "password": password }),
        )
        .await
        .status()
        .as_u16()
}

/// The token of the most recent mail with this subject.
fn latest_token(server: &common::Server, subject: &str) -> String {
    let mails = server.mailer.attempted_with_subject(subject);
    token_of(&activation_link(&mails.last().expect("no such mail").html))
}

async fn sweep(server: &common::Server, now: DateTime<Utc>) -> SweepReport {
    server
        .state
        .sweep_pending_accounts()
        .execute(now)
        .await
        .unwrap()
}

async fn stored_link_hash(server: &common::Server, username: &str) -> String {
    sqlx::query_scalar(
        "SELECT i.token_hash FROM user_invitations i JOIN users u ON u.id = i.user_id WHERE u.username = $1",
    )
    .bind(username)
    .fetch_one(&server.pool)
    .await
    .unwrap()
}

async fn users_named(server: &common::Server, username: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&server.pool)
        .await
        .unwrap()
}

#[sqlx::test]
async fn a_reminder_with_a_new_working_link_follows_the_expiry_of_the_first_link(pool: PgPool) {
    let server = common::spawn_server(pool).await;
    open_registration(&server).await;
    let start = Utc::now();
    assert_eq!(register(&server, "alice").await.status(), 204);
    let first = latest_token(&server, CONFIRMATION);

    let early = sweep(&server, start + Duration::hours(1)).await;
    assert_eq!(
        early,
        SweepReport::default(),
        "the first link is still valid"
    );
    assert!(server.mailer.attempted_with_subject(REMINDER).is_empty());

    let due = sweep(&server, start + Duration::hours(25)).await;
    assert_eq!(due.reminded, 1);
    let mails = server.mailer.delivered();
    let reminder = mails.last().unwrap();
    assert_eq!(reminder.to, "alice@example.com");
    assert_eq!(reminder.subject, REMINDER);
    assert!(reminder.text.contains("alice"), "{}", reminder.text);
    let deletion = (start + Duration::days(7)).format("%d/%m/%Y").to_string();
    assert!(
        reminder.text.contains(&deletion),
        "the deletion date {deletion} is in the mail: {}",
        reminder.text
    );

    let second = latest_token(&server, REMINDER);
    assert_ne!(first, second);
    assert_eq!(
        activate(&server, &first).await.status(),
        400,
        "the first link stops working"
    );
    assert_eq!(activate(&server, &second).await.status(), 204);
    assert_eq!(
        server.login("alice", PASSWORD).await["mfaSetupRequired"],
        json!(true)
    );
}

#[sqlx::test]
async fn a_reminder_goes_out_once_a_day_and_the_account_is_deleted_after_seven_days(pool: PgPool) {
    let server = common::spawn_server(pool).await;
    open_registration(&server).await;
    let start = Utc::now();
    register(&server, "alice").await;

    let mut reminders = 0;
    for day in 1..=6 {
        let now = start + Duration::days(day) + Duration::minutes(5);
        let report = sweep(&server, now).await;
        assert_eq!(report.reminded, 1, "day {day}");
        assert_eq!(report.deleted, 0, "day {day}");
        reminders += report.reminded;
        // The sweep runs hourly: the next ones, the same day, have nothing to do.
        let again = sweep(&server, now + Duration::hours(3)).await;
        assert_eq!(
            again,
            SweepReport::default(),
            "day {day}, three hours later"
        );
    }
    assert_eq!(reminders, 6);
    assert_eq!(server.mailer.attempted_with_subject(REMINDER).len(), 6);
    let last_link = latest_token(&server, REMINDER);

    let end = sweep(&server, start + Duration::days(7) + Duration::minutes(1)).await;

    assert_eq!(end.deleted, 1);
    assert_eq!(end.reminded, 0, "no reminder on the day it goes");
    assert_eq!(users_named(&server, "alice").await, 0);
    assert_eq!(login_status(&server, "alice", PASSWORD).await, 401);
    assert_eq!(activate(&server, &last_link).await.status(), 400);
    assert_eq!(
        register(&server, "alice").await.status(),
        204,
        "the name and the address are free again"
    );
    assert_eq!(server.mailer.attempted_with_subject(CONFIRMATION).len(), 2);
}

#[sqlx::test]
async fn an_activated_account_is_never_touched(pool: PgPool) {
    let server = common::spawn_server(pool).await;
    open_registration(&server).await;
    let start = Utc::now();
    register(&server, "alice").await;
    assert_eq!(
        activate(&server, &latest_token(&server, CONFIRMATION))
            .await
            .status(),
        204
    );

    let report = sweep(&server, start + Duration::days(30)).await;

    assert_eq!(report, SweepReport::default());
    assert_eq!(users_named(&server, "alice").await, 1);
    assert!(server.mailer.attempted_with_subject(REMINDER).is_empty());
    assert_eq!(login_status(&server, "alice", PASSWORD).await, 200);
}

#[sqlx::test]
async fn an_account_invited_by_an_admin_is_cleaned_up_the_same_way(pool: PgPool) {
    let server = common::spawn_server(pool).await;
    let admin = admin_session(&server).await;
    configure_smtp(&server, &admin).await;
    let start = Utc::now();
    let invited = server
        .post_as(
            &admin,
            "/admin/users/invite",
            json!({ "username": "bob", "email": "bob@example.com", "isAdmin": false }),
        )
        .await;
    assert_eq!(invited.status(), 200);

    let reminded = sweep(&server, start + Duration::hours(25)).await;
    assert_eq!(reminded.reminded, 1);
    assert_eq!(server.mailer.delivered().last().unwrap().subject, REMINDER);
    // The admin's list still shows the account as waiting, with the renewed link.
    let listed = server.admin_users(&admin).await;
    let bob = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["username"] == "bob")
        .unwrap();
    assert_eq!(bob["state"], "invited");

    let gone = sweep(&server, start + Duration::days(8)).await;
    assert_eq!(gone.deleted, 1);
    assert_eq!(users_named(&server, "bob").await, 0);
}

#[sqlx::test]
async fn a_reminder_that_cannot_be_sent_leaves_the_link_alone_and_is_retried(pool: PgPool) {
    let server = common::spawn_server(pool).await;
    open_registration(&server).await;
    let start = Utc::now();
    register(&server, "alice").await;
    let before = stored_link_hash(&server, "alice").await;
    server.mailer.fail_with("550 relay refused");

    let report = sweep(&server, start + Duration::hours(25)).await;

    assert_eq!(report.failed, 1);
    assert_eq!(report.reminded, 0);
    assert_eq!(
        stored_link_hash(&server, "alice").await,
        before,
        "the stored link was not replaced by one nobody received"
    );

    server.mailer.recover();
    let retry = sweep(&server, start + Duration::hours(26)).await;
    assert_eq!(retry.reminded, 1);
    assert_ne!(stored_link_hash(&server, "alice").await, before);
    assert_eq!(server.mailer.delivered().last().unwrap().subject, REMINDER);
}

#[sqlx::test]
async fn without_mail_configured_there_are_no_reminders_but_the_account_still_goes(pool: PgPool) {
    let server = common::spawn_server(pool).await;
    let admin = admin_session(&server).await;
    let start = Utc::now();
    let invited = server
        .post_as(
            &admin,
            "/admin/users/invite",
            json!({ "username": "bob", "email": "bob@example.com", "isAdmin": false }),
        )
        .await;
    assert_eq!(invited.status(), 200);
    let attempts = server.mailer.attempts();

    let quiet = sweep(&server, start + Duration::hours(25)).await;
    assert_eq!(
        quiet,
        SweepReport::default(),
        "no mail, no reminder, no failure"
    );
    assert_eq!(server.mailer.attempts(), attempts);

    let gone = sweep(&server, start + Duration::days(8)).await;
    assert_eq!(gone.deleted, 1);
    assert_eq!(users_named(&server, "bob").await, 0);
}
