// Anonymous Git reads of a public repository (and its wiki) follow the "Pages publiques" switch: with it off they are
// refused exactly like a private repository. Signed-in users aren't affected.

mod common;

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use async_trait::async_trait;
use common::{ADMIN_PASSWORD, Options, Server, USER_PASSWORD, spawn_with};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::public_pages::{
    PublicPagesSettings, PublicPagesSettingsPort, PublicPagesSettingsUpdate,
};
use reqwest::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

async fn spawn(pool: PgPool) -> Server {
    spawn_with(pool, Options::default(), |state| {
        state.mfa_enforced = false; // not about MFA, so log in with a plain session
    })
    .await
}

async fn session(server: &Server, username: &str, password: &str) -> String {
    let body: Value = server
        .post(
            "/auth/login",
            json!({ "username": username, "password": password }),
        )
        .await
        .json()
        .await
        .unwrap();
    body["token"].as_str().expect("a session token").to_string()
}

async fn api_token(server: &Server, jwt: &str) -> String {
    let body: Value = server
        .post_as(jwt, "/tokens", json!({ "name": "git" }))
        .await
        .json()
        .await
        .unwrap();
    body["token"].as_str().unwrap().to_string()
}

async fn set_public_pages(server: &Server, jwt: &str, enabled: bool) {
    let response = server
        .client
        .put(server.url("/admin/settings"))
        .bearer_auth(jwt)
        .json(&json!({ "publicPagesEnabled": enabled }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn create_repository(server: &Server, jwt: &str, name: &str, visibility: &str) {
    let response = server
        .post_as(
            jwt,
            "/repositories",
            json!({ "name": name, "visibility": visibility }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args([
            "-c",
            "user.email=dev@example.com",
            "-c",
            "user.name=Dev",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// Pushes one commit to `remote`. In spawn_blocking, because a blocking git subprocess would starve the server task.
async fn push_commit(remote: String) {
    tokio::task::spawn_blocking(move || {
        let work = tempfile::tempdir().unwrap();
        let dir = work.path();
        std::fs::write(dir.join("README.md"), "# Hello\n").unwrap();
        git(dir, &["init", "-q"]);
        git(dir, &["add", "."]);
        git(dir, &["commit", "-q", "-m", "Initial commit"]);
        git(dir, &["push", "-q", &remote, "main"]);
    })
    .await
    .unwrap();
}

/// Clones with the credentials in `url` and nothing else, never prompting; true if it worked.
async fn clone_succeeds(url: String) -> bool {
    tokio::task::spawn_blocking(move || {
        let work = tempfile::tempdir().unwrap();
        // No credential helper either, so a developer's keychain can't answer the server's challenge.
        let output = Command::new("git")
            .args(["-c", "credential.helper=", "clone", "-q", &url, "checkout"])
            .env("GIT_TERMINAL_PROMPT", "0")
            .current_dir(work.path())
            .output()
            .unwrap();
        output.status.success()
    })
    .await
    .unwrap()
}

/// What an anonymous git client sees on the discovery request: status, challenge header and body.
async fn anonymous_discovery(server: &Server, path: &str) -> (StatusCode, Option<String>, Vec<u8>) {
    let response = server
        .client
        .get(format!(
            "http://{}/{path}/info/refs?service=git-upload-pack",
            server.addr
        ))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let challenge = response
        .headers()
        .get("www-authenticate")
        .map(|v| v.to_str().unwrap().to_string());
    (status, challenge, response.bytes().await.unwrap().to_vec())
}

/// Two repositories and their wikis, one public and one private, each with a first commit pushed.
async fn seed(server: &Server, admin: &str) -> String {
    create_repository(server, admin, "open", "public").await;
    create_repository(server, admin, "closed", "private").await;
    let token = api_token(server, admin).await;
    for name in ["open", "closed"] {
        push_commit(format!(
            "http://admin:{token}@{}/admin/{name}.git",
            server.addr
        ))
        .await;
        push_commit(format!(
            "http://admin:{token}@{}/admin/{name}.wiki.git",
            server.addr
        ))
        .await;
    }
    token
}

#[sqlx::test]
async fn anonymous_clone_of_a_public_repository_works_while_public_pages_are_on(pool: PgPool) {
    let server = spawn(pool).await;
    let admin = session(&server, "admin", ADMIN_PASSWORD).await;
    seed(&server, &admin).await;

    assert!(clone_succeeds(format!("http://{}/admin/open.git", server.addr)).await);
    assert!(clone_succeeds(format!("http://{}/admin/open.wiki.git", server.addr)).await);
    assert!(!clone_succeeds(format!("http://{}/admin/closed.git", server.addr)).await);
}

#[sqlx::test]
async fn with_public_pages_off_anonymous_git_reads_are_refused_like_a_private_repository(
    pool: PgPool,
) {
    let server = spawn(pool).await;
    let admin = session(&server, "admin", ADMIN_PASSWORD).await;
    seed(&server, &admin).await;
    set_public_pages(&server, &admin, false).await;

    let private_repo = anonymous_discovery(&server, "admin/closed.git").await;
    assert_eq!(private_repo.0, StatusCode::UNAUTHORIZED);
    assert_eq!(private_repo.1.as_deref(), Some("Basic realm=\"FerrisGit\""));
    let private_wiki = anonymous_discovery(&server, "admin/closed.wiki.git").await;
    let unknown = anonymous_discovery(&server, "admin/nothing.git").await;

    // Same status, same challenge, same body: nothing tells the public repository from a private or unknown one.
    assert_eq!(
        anonymous_discovery(&server, "admin/open.git").await,
        private_repo
    );
    assert_eq!(
        anonymous_discovery(&server, "admin/open.wiki.git").await,
        private_wiki
    );
    assert_eq!(private_wiki, private_repo);
    assert_eq!(unknown, private_repo);

    assert!(!clone_succeeds(format!("http://{}/admin/open.git", server.addr)).await);
    assert!(!clone_succeeds(format!("http://{}/admin/open.wiki.git", server.addr)).await);
    // Credentials that match nobody are anonymous in all but name.
    assert!(
        !clone_succeeds(format!(
            "http://admin:fg_not-a-real-token@{}/admin/open.git",
            server.addr
        ))
        .await
    );

    // Switching it back on restores the anonymous clone.
    set_public_pages(&server, &admin, true).await;
    assert!(clone_succeeds(format!("http://{}/admin/open.git", server.addr)).await);
    assert!(clone_succeeds(format!("http://{}/admin/open.wiki.git", server.addr)).await);
}

#[sqlx::test]
async fn with_public_pages_off_signed_in_users_still_read_public_repositories(pool: PgPool) {
    let server = spawn(pool).await;
    let admin = session(&server, "admin", ADMIN_PASSWORD).await;
    let owner_token = seed(&server, &admin).await;
    server.new_user("stranger").await;
    let stranger_jwt = session(&server, "stranger", USER_PASSWORD).await;
    let stranger_token = api_token(&server, &stranger_jwt).await;
    set_public_pages(&server, &admin, false).await;

    // The owner, and a user with no role on the repository: signed in, so a Reader of a public repository.
    assert!(
        clone_succeeds(format!(
            "http://admin:{owner_token}@{}/admin/open.git",
            server.addr
        ))
        .await
    );
    assert!(
        clone_succeeds(format!(
            "http://stranger:{stranger_token}@{}/admin/open.git",
            server.addr
        ))
        .await
    );
    assert!(
        clone_succeeds(format!(
            "http://stranger:{stranger_token}@{}/admin/open.wiki.git",
            server.addr
        ))
        .await
    );
    // Being signed in does not open a private repository, nor allow writing to the public one.
    assert!(
        !clone_succeeds(format!(
            "http://stranger:{stranger_token}@{}/admin/closed.git",
            server.addr
        ))
        .await
    );
    assert_eq!(
        push_discovery_status(&server, &stranger_token).await,
        StatusCode::UNAUTHORIZED
    );
}

async fn push_discovery_status(server: &Server, token: &str) -> StatusCode {
    server
        .client
        .get(format!(
            "http://{}/admin/open.git/info/refs?service=git-receive-pack",
            server.addr
        ))
        .basic_auth("stranger", Some(token))
        .send()
        .await
        .unwrap()
        .status()
}

struct UnreadableSettings;

#[async_trait]
impl PublicPagesSettingsPort for UnreadableSettings {
    async fn get(&self) -> Result<PublicPagesSettings, DomainError> {
        Err(DomainError::Infrastructure(
            "settings unreadable".to_string(),
        ))
    }

    async fn update(
        &self,
        _update: PublicPagesSettingsUpdate,
    ) -> Result<PublicPagesSettings, DomainError> {
        Err(DomainError::Infrastructure(
            "settings unreadable".to_string(),
        ))
    }
}

#[sqlx::test]
async fn an_unreadable_public_pages_setting_refuses_anonymous_reads_but_not_signed_in_ones(
    pool: PgPool,
) {
    let server = spawn(pool).await;
    let admin = session(&server, "admin", ADMIN_PASSWORD).await;
    let token = seed(&server, &admin).await;

    // The same instance, now unable to read the setting.
    let mut state = server.state.clone();
    state.public_pages_settings = Arc::new(UnreadableSettings);
    let app = ferrisgit_api::build_router(state, &tempfile::tempdir().unwrap().keep());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });

    assert!(!clone_succeeds(format!("http://{addr}/admin/open.git")).await);
    assert!(clone_succeeds(format!("http://admin:{token}@{addr}/admin/open.git")).await);
}
