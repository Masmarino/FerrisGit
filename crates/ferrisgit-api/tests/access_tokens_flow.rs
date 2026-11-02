//! API tokens over real HTTP: what a scope opens, what it never opens, and the sign-in from a terminal.

mod common;

use common::http::{get, login, post, post_anon, post_json};
use common::{ADMIN_PASSWORD, Device, USER_PASSWORD, assert_error, next_step_code};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;

const ADMIN: &str = "adminpassword123";

async fn new_token(
    client: &reqwest::Client,
    addr: SocketAddr,
    session: &str,
    scopes: &[&str],
) -> String {
    let res = post(
        client,
        addr,
        session,
        "/access-tokens",
        &json!({ "name": "ci", "scopes": scopes, "expiresInDays": 30 }),
    )
    .await;
    assert_eq!(res.status(), 200, "the token must be created");
    let body: Value = res.json().await.unwrap();
    body["token"].as_str().unwrap().to_string()
}

async fn new_repository(client: &reqwest::Client, addr: SocketAddr, session: &str) -> String {
    let repo: Value = post_json(
        client,
        addr,
        session,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    repo["id"].as_str().unwrap().to_string()
}

#[sqlx::test]
async fn a_token_opens_its_scope_and_nothing_next_to_it(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let session = login(&client, addr, "admin", ADMIN).await;
    let repo = new_repository(&client, addr, &session).await;
    let token = new_token(&client, addr, &session, &["issues:read"]).await;

    let issues = format!("/repositories/{repo}/issues");
    assert_eq!(get(&client, addr, &token, &issues).await.status(), 200);

    let write = post(
        &client,
        addr,
        &token,
        &issues,
        &json!({ "title": "t", "description": "d", "kind": "bug" }),
    )
    .await;
    assert_error(write, 403, "this token lacks the scope issues:write").await;

    let beside = get(
        &client,
        addr,
        &token,
        &format!("/repositories/{repo}/merge-requests"),
    )
    .await;
    assert_error(
        beside,
        403,
        "this token lacks the scope merge-requests:read",
    )
    .await;
}

#[sqlx::test]
async fn write_includes_read(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let session = login(&client, addr, "admin", ADMIN).await;
    let repo = new_repository(&client, addr, &session).await;
    let token = new_token(&client, addr, &session, &["issues:write"]).await;
    let issues = format!("/repositories/{repo}/issues");

    let created = post(
        &client,
        addr,
        &token,
        &issues,
        &json!({ "title": "t", "description": "d", "kind": "bug" }),
    )
    .await;

    assert_eq!(created.status(), 200);
    assert_eq!(get(&client, addr, &token, &issues).await.status(), 200);
}

#[sqlx::test]
async fn a_scope_does_not_reach_past_the_owners_rights(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let admin = login(&client, addr, "admin", ADMIN).await;
    let repo = new_repository(&client, addr, &admin).await;
    common::http::create_user(&client, addr, &admin, "bob").await;
    let bob = login(&client, addr, "bob", USER_PASSWORD).await;
    let token = new_token(&client, addr, &bob, &["repositories:write", "issues:write"]).await;

    let res = get(
        &client,
        addr,
        &token,
        &format!("/repositories/{repo}/issues"),
    )
    .await;

    assert_eq!(
        res.status(),
        404,
        "bob has no role on the admin's private repository"
    );
}

#[sqlx::test]
async fn a_token_can_not_manage_credentials_or_the_account(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let session = login(&client, addr, "admin", ADMIN).await;
    let every_scope = [
        "repositories:write",
        "issues:write",
        "merge-requests:write",
        "pipelines:write",
        "releases:write",
        "admin:write",
    ];
    let token = new_token(&client, addr, &session, &every_scope).await;

    for (path, body) in [
        (
            "/access-tokens",
            json!({ "name": "x", "scopes": ["issues:read"], "expiresInDays": 1 }),
        ),
        ("/tokens", json!({ "name": "x" })),
        ("/auth/logout-all", json!({})),
        (
            "/auth/me/password",
            json!({ "currentPassword": ADMIN, "newPassword": "another-password" }),
        ),
        ("/me/mfa/totp/enroll", json!({ "currentPassword": ADMIN })),
        (
            "/auth/device/BCDF-GHJK/approve",
            json!({ "code": "000000" }),
        ),
    ] {
        let res = post(&client, addr, &token, path, &body).await;
        assert_eq!(res.status(), 403, "POST {path}");
    }
    for path in [
        "/access-tokens",
        "/tokens",
        "/me/mfa",
        "/auth/device/BCDF-GHJK",
    ] {
        assert_eq!(
            get(&client, addr, &token, path).await.status(),
            403,
            "GET {path}"
        );
    }
    assert_eq!(get(&client, addr, &token, "/auth/me").await.status(), 200);
}

#[sqlx::test]
async fn the_admin_routes_take_the_admin_scope_and_an_admin(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let admin = login(&client, addr, "admin", ADMIN).await;

    let without = new_token(&client, addr, &admin, &["repositories:write"]).await;
    assert_error(
        get(&client, addr, &without, "/admin/users").await,
        403,
        "this token lacks the scope admin:read",
    )
    .await;

    let reader = new_token(&client, addr, &admin, &["admin:read"]).await;
    assert_eq!(
        get(&client, addr, &reader, "/admin/users").await.status(),
        200
    );
    let refused = post(
        &client,
        addr,
        &reader,
        "/admin/users/invite",
        &json!({ "email": "x@example.com" }),
    )
    .await;
    assert_error(refused, 403, "this token lacks the scope admin:write").await;

    common::http::create_user(&client, addr, &admin, "bob").await;
    let bob = login(&client, addr, "bob", USER_PASSWORD).await;
    let asked = post(
        &client,
        addr,
        &bob,
        "/access-tokens",
        &json!({ "name": "x", "scopes": ["admin:read"], "expiresInDays": 1 }),
    )
    .await;
    assert_error(
        asked,
        400,
        "only an administrator can create a token with the admin scope",
    )
    .await;
}

#[sqlx::test]
async fn a_demoted_administrators_token_stops_at_the_role(pool: PgPool) {
    let app = common::spawn_app(pool.clone()).await;
    let addr = app.addr;
    let client = reqwest::Client::new();
    let admin = login(&client, addr, "admin", ADMIN).await;
    let token = new_token(&client, addr, &admin, &["admin:read"]).await;

    sqlx::query("UPDATE users SET is_admin = false WHERE username = 'admin'")
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(
        get(&client, addr, &token, "/admin/users").await.status(),
        403
    );
}

#[sqlx::test]
async fn an_expired_or_revoked_token_is_a_401(pool: PgPool) {
    let addr = common::spawn_app(pool.clone()).await.addr;
    let client = reqwest::Client::new();
    let session = login(&client, addr, "admin", ADMIN).await;

    let expired = new_token(&client, addr, &session, &["repositories:read"]).await;
    sqlx::query("UPDATE access_tokens SET expires_at = now() - interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    assert_error(
        get(&client, addr, &expired, "/repositories").await,
        401,
        "invalid or expired token",
    )
    .await;

    let listed: Value = get(&client, addr, &session, "/access-tokens")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(listed[0]["expired"], true);
    assert!(
        listed[0].get("token").is_none() && listed[0].get("tokenHash").is_none(),
        "a listing never shows the token: {listed}"
    );

    let revoked = new_token(&client, addr, &session, &["repositories:read"]).await;
    assert_eq!(
        get(&client, addr, &revoked, "/repositories").await.status(),
        200
    );
    let listed: Value = get(&client, addr, &session, "/access-tokens")
        .await
        .json()
        .await
        .unwrap();
    let id = listed[0]["id"].as_str().unwrap();
    let res = common::http::delete(&client, addr, &session, &format!("/access-tokens/{id}")).await;
    assert_eq!(res.status(), 204);
    assert_error(
        get(&client, addr, &revoked, "/repositories").await,
        401,
        "invalid or expired token",
    )
    .await;
}

#[sqlx::test]
async fn revoking_another_accounts_token_is_a_404(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let admin = login(&client, addr, "admin", ADMIN).await;
    let token = new_token(&client, addr, &admin, &["repositories:read"]).await;
    common::http::create_user(&client, addr, &admin, "bob").await;
    let bob = login(&client, addr, "bob", USER_PASSWORD).await;
    let listed: Value = get(&client, addr, &admin, "/access-tokens")
        .await
        .json()
        .await
        .unwrap();
    let id = listed[0]["id"].as_str().unwrap();

    let res = common::http::delete(&client, addr, &bob, &format!("/access-tokens/{id}")).await;

    assert_eq!(res.status(), 404);
    assert_eq!(
        get(&client, addr, &token, "/repositories").await.status(),
        200
    );
}

#[sqlx::test]
async fn git_takes_a_token_within_its_scope(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let session = login(&client, addr, "admin", ADMIN).await;
    new_repository(&client, addr, &session).await;

    let git_status = |service: &'static str, token: String| {
        let client = client.clone();
        async move {
            client
                .get(format!(
                    "http://{addr}/admin/hello.git/info/refs?service={service}"
                ))
                .basic_auth("fgit", Some(token))
                .send()
                .await
                .unwrap()
                .status()
        }
    };

    let reader = new_token(&client, addr, &session, &["repositories:read"]).await;
    assert_eq!(git_status("git-upload-pack", reader.clone()).await, 200);
    assert_eq!(git_status("git-receive-pack", reader).await, 401);

    let writer = new_token(&client, addr, &session, &["repositories:write"]).await;
    assert_eq!(git_status("git-receive-pack", writer).await, 200);

    let elsewhere = new_token(&client, addr, &session, &["issues:write"]).await;
    assert_eq!(git_status("git-upload-pack", elsewhere).await, 401);
}

mod device_login {
    use super::*;

    async fn start(client: &reqwest::Client, addr: SocketAddr, scopes: &[&str]) -> Value {
        let res = post_anon(
            client,
            addr,
            "/auth/device/start",
            &json!({ "clientName": "fgit on laptop", "scopes": scopes, "expiresInDays": 90 }),
        )
        .await;
        assert_eq!(res.status(), 200);
        res.json().await.unwrap()
    }

    async fn poll(
        client: &reqwest::Client,
        addr: SocketAddr,
        device_code: &str,
    ) -> reqwest::Response {
        post_anon(
            client,
            addr,
            "/auth/device/token",
            &json!({ "deviceCode": device_code }),
        )
        .await
    }

    #[sqlx::test]
    async fn a_terminal_gets_its_token_once_the_user_confirms_with_a_second_factor(pool: PgPool) {
        let server = common::spawn_server(pool).await;
        let (addr, client) = (server.addr, server.client.clone());
        let admin = server.first_setup_with_totp("admin", ADMIN_PASSWORD).await;
        let secret = admin.secret.clone().unwrap();

        let started = start(&client, addr, &["repositories:read"]).await;
        let user_code = started["userCode"].as_str().unwrap();
        let device_code = started["deviceCode"].as_str().unwrap();
        assert!(
            started["verificationUriComplete"]
                .as_str()
                .unwrap()
                .ends_with(&format!("/account/device?code={user_code}"))
        );

        assert_error(
            poll(&client, addr, device_code).await,
            400,
            "authorization_pending",
        )
        .await;

        let shown: Value = get(
            &client,
            addr,
            &admin.session,
            &format!("/auth/device/{}", user_code.to_lowercase()),
        )
        .await
        .json()
        .await
        .unwrap();
        assert_eq!(shown["clientName"], "fgit on laptop");
        assert_eq!(shown["scopes"], json!(["repositories:read"]));
        assert_eq!(shown["hasTotp"], true);
        assert!(shown.get("deviceCode").is_none(), "{shown}");

        // A session isn't enough: the second factor is asked again.
        let approve = format!("/auth/device/{user_code}/approve");
        assert_error(
            post(&client, addr, &admin.session, &approve, &json!({})).await,
            400,
            "invalid code",
        )
        .await;
        assert_error(
            post(
                &client,
                addr,
                &admin.session,
                &approve,
                &json!({ "code": common::wrong_code(&secret) }),
            )
            .await,
            400,
            "invalid code",
        )
        .await;
        assert_error(poll(&client, addr, device_code).await, 400, "slow_down").await;

        let approved = post(
            &client,
            addr,
            &admin.session,
            &approve,
            &json!({ "code": next_step_code(&secret) }),
        )
        .await;
        assert_eq!(approved.status(), 204);

        let collected = poll(&client, addr, device_code).await;
        assert_eq!(collected.status(), 200);
        let body: Value = collected.json().await.unwrap();
        let token = body["token"].as_str().unwrap();
        assert!(token.starts_with("fga_"));
        assert_eq!(body["scopes"], json!(["repositories:read"]));
        assert_eq!(body["name"], "fgit on laptop");

        let me: Value = get(&client, addr, token, "/auth/me")
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(me["username"], "admin");
        assert_eq!(
            get(&client, addr, token, "/repositories").await.status(),
            200
        );

        assert_error(poll(&client, addr, device_code).await, 400, "expired_token").await;

        let events = server.security_events_of(&admin.id).await;
        assert!(
            events.iter().any(|e| e == "DeviceLoginApproved"),
            "{events:?}"
        );
        assert!(
            events.iter().any(|e| e == "AccessTokenCreated"),
            "{events:?}"
        );
    }

    #[sqlx::test]
    async fn a_refused_request_tells_the_terminal_and_can_not_be_approved(pool: PgPool) {
        let server = common::spawn_server(pool).await;
        let (addr, client) = (server.addr, server.client.clone());
        let admin = server.first_setup_with_totp("admin", ADMIN_PASSWORD).await;
        let secret = admin.secret.clone().unwrap();
        let started = start(&client, addr, &["repositories:read"]).await;
        let user_code = started["userCode"].as_str().unwrap();

        let denied = post(
            &client,
            addr,
            &admin.session,
            &format!("/auth/device/{user_code}/deny"),
            &json!({}),
        )
        .await;
        assert_eq!(denied.status(), 204);

        assert_error(
            poll(&client, addr, started["deviceCode"].as_str().unwrap()).await,
            400,
            "access_denied",
        )
        .await;
        let late = post(
            &client,
            addr,
            &admin.session,
            &format!("/auth/device/{user_code}/approve"),
            &json!({ "code": next_step_code(&secret) }),
        )
        .await;
        assert_eq!(late.status(), 404);
    }

    #[sqlx::test]
    async fn nobody_approves_without_a_session(pool: PgPool) {
        let server = common::spawn_server(pool).await;
        let (addr, client) = (server.addr, server.client.clone());
        let started = start(&client, addr, &["repositories:read"]).await;
        let user_code = started["userCode"].as_str().unwrap();

        let res = post_anon(
            &client,
            addr,
            &format!("/auth/device/{user_code}/approve"),
            &json!({ "code": "000000" }),
        )
        .await;

        assert_eq!(res.status(), 401);
    }

    #[sqlx::test]
    async fn an_account_without_the_admin_right_can_not_approve_the_admin_scope(pool: PgPool) {
        let server = common::spawn_server(pool).await;
        let (addr, client) = (server.addr, server.client.clone());
        server.new_user("bob").await;
        let bob = server.first_setup_with_totp("bob", USER_PASSWORD).await;
        let started = start(&client, addr, &["admin:write"]).await;

        let res = post(
            &client,
            addr,
            &bob.session,
            &format!(
                "/auth/device/{}/approve",
                started["userCode"].as_str().unwrap()
            ),
            &json!({ "code": next_step_code(bob.secret.as_ref().unwrap()) }),
        )
        .await;

        assert_error(
            res,
            400,
            "only an administrator can create a token with the admin scope",
        )
        .await;
        assert_error(
            poll(&client, addr, started["deviceCode"].as_str().unwrap()).await,
            400,
            "authorization_pending",
        )
        .await;
    }

    #[sqlx::test]
    async fn a_passkey_confirms_a_request_too(pool: PgPool) {
        let server = common::spawn_server(pool).await;
        let (addr, client) = (server.addr, server.client.clone());
        let mut device = Device::new();
        let admin = server
            .first_setup_with_passkey(&mut device, "admin", ADMIN_PASSWORD, "laptop")
            .await;
        let started = start(&client, addr, &["repositories:read"]).await;
        let user_code = started["userCode"].as_str().unwrap();

        let challenge: Value = post(
            &client,
            addr,
            &admin.session,
            &format!("/auth/device/{user_code}/passkey"),
            &json!({}),
        )
        .await
        .json()
        .await
        .unwrap();
        let credential = device.assert(&challenge["publicKey"]);

        let approved = post(
            &client,
            addr,
            &admin.session,
            &format!("/auth/device/{user_code}/approve"),
            &json!({ "challengeId": challenge["challengeId"], "credential": credential }),
        )
        .await;

        assert_eq!(approved.status(), 204);
    }

    #[sqlx::test]
    async fn starting_needs_known_scopes_and_a_bounded_lifetime(pool: PgPool) {
        let server = common::spawn_server(pool).await;
        let (addr, client) = (server.addr, server.client.clone());

        for body in [
            json!({ "clientName": "fgit", "scopes": ["wiki:read"], "expiresInDays": 90 }),
            json!({ "clientName": "fgit", "scopes": [], "expiresInDays": 90 }),
            json!({ "clientName": "fgit", "scopes": ["issues:read"], "expiresInDays": 366 }),
            json!({ "clientName": "\u{1b}[2J", "scopes": ["issues:read"], "expiresInDays": 90 }),
        ] {
            let res = post_anon(&client, addr, "/auth/device/start", &body).await;
            assert_eq!(res.status(), 400, "{body}");
        }
    }
}
