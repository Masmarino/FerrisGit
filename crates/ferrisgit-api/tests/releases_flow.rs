use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use serde_json::json;
use sqlx::PgPool;
use std::process::Command;

#[sqlx::test]
async fn releases_are_maintainer_gated_drafts_stay_hidden_and_assets_round_trip(pool: PgPool) {
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

    let storage_dir = tempfile::tempdir().unwrap();
    let static_dir = tempfile::tempdir().unwrap();
    std::fs::write(static_dir.path().join("index.html"), "<html></html>").unwrap();

    let config = Config {
        database_url: String::new(),
        jwt_secret: "test-secret-that-is-at-least-32-characters-long".to_string(),
        storage_root: storage_dir.path().to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:0".to_string(),
        static_dir: static_dir.path().to_string_lossy().to_string(),
        bootstrap_admin_username: Some("admin".to_string()),
        bootstrap_admin_password: Some("adminpassword123".to_string()),
        settings_encryption_key: [b'k'; 32],
        public_url: "http://localhost:4200".to_string(),
        trusted_proxy_cidrs: vec![],
    };

    let mut state = AppState::new(pool, config.clone()).await;
    state.mfa_enforced = false; // these tests are not about MFA: they log in with a plain session
    BootstrapAdminUseCase::new(state.users.clone(), state.hasher.clone())
        .execute(
            config.bootstrap_admin_username.clone(),
            config.bootstrap_admin_password.clone(),
        )
        .await
        .unwrap();

    let app = build_router(state, static_dir.path());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();

    let owner_login: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "admin", "password": "adminpassword123" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let owner_jwt = owner_login["token"].as_str().unwrap();

    let token_res: serde_json::Value = client
        .post(format!("http://{addr}/api/tokens"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": "ci" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let plain_token = token_res["token"].as_str().unwrap();

    let repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": "hello", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let repo_id = repo_res["id"].as_str().unwrap();

    client.post(format!("http://{addr}/api/admin/users")).bearer_auth(owner_jwt).json(&json!({ "username": "contributor-carl", "email": "carl@example.com", "password": "password12345" })).send().await.unwrap().error_for_status().unwrap();
    client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/collaborators"
        ))
        .bearer_auth(owner_jwt)
        .json(&json!({ "username": "contributor-carl", "role": "contributor" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let carl_login: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "contributor-carl", "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let carl_jwt = carl_login["token"].as_str().unwrap();

    // git subprocess calls run in `spawn_blocking`: a blocking `Command` starves the `axum::serve` task on the same runtime and deadlocks the clone.
    let clone_parent = tempfile::tempdir().unwrap();
    let clone_url = format!("http://admin:{plain_token}@{addr}/admin/hello.git");
    let repo_path = clone_parent.path().join("repo");
    let clone_status = tokio::task::spawn_blocking({
        let clone_url = clone_url.clone();
        let clone_dir = clone_parent.path().to_path_buf();
        move || {
            Command::new("git")
                .args(["clone", &clone_url, "repo"])
                .current_dir(&clone_dir)
                .status()
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(clone_status.success());

    std::fs::write(repo_path.join("README.md"), "hello\n").unwrap();

    let add_status = tokio::task::spawn_blocking({
        let repo_path = repo_path.clone();
        move || {
            Command::new("git")
                .args(["add", "."])
                .current_dir(&repo_path)
                .status()
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(add_status.success());

    let commit_status = tokio::task::spawn_blocking({
        let repo_path = repo_path.clone();
        move || {
            Command::new("git")
                .args([
                    "-c",
                    "user.email=t@t.com",
                    "-c",
                    "user.name=t",
                    "commit",
                    "-q",
                    "-m",
                    "root",
                ])
                .current_dir(&repo_path)
                .status()
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(commit_status.success());

    let push_status = tokio::task::spawn_blocking({
        let repo_path = repo_path.clone();
        move || {
            Command::new("git")
                .args(["push", "-q", "origin", "HEAD:main"])
                .current_dir(&repo_path)
                .status()
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(push_status.success());

    let sha_output = tokio::task::spawn_blocking({
        let repo_path = repo_path.clone();
        move || {
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&repo_path)
                .output()
        }
    })
    .await
    .unwrap()
    .unwrap();
    let commit_sha = String::from_utf8_lossy(&sha_output.stdout)
        .trim()
        .to_string();

    let forbidden = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/releases"))
        .bearer_auth(carl_jwt)
        .json(&json!({ "tagName": "v1.0.0", "targetCommitSha": commit_sha, "title": "First release", "draft": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(forbidden.status(), reqwest::StatusCode::NOT_FOUND);

    // A tag name containing '/' must never reach `CreateReleaseUseCase`/git: it would break routing for routes that treat `{tag_name}` as one segment.
    let invalid_tag = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/releases"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "tagName": "release/1.0", "targetCommitSha": commit_sha, "title": "Bad tag", "draft": false }))
        .send()
        .await
        .unwrap();
    assert!(
        invalid_tag.status().is_client_error(),
        "a tag name containing '/' must be rejected, got {}",
        invalid_tag.status()
    );

    let draft: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/releases"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "tagName": "v1.0.0", "targetCommitSha": commit_sha, "title": "First release", "notes": "notes", "draft": true }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(draft["draft"].as_bool().unwrap());
    assert!(draft["publishedAt"].is_null());

    // Checked over the wire via `ls-remote`, not by guessing the bare repo's on-disk layout.
    let verify_dir = tempfile::tempdir().unwrap();
    let verify_clone_status = tokio::task::spawn_blocking({
        let clone_url = clone_url.clone();
        let verify_dir = verify_dir.path().to_path_buf();
        move || {
            Command::new("git")
                .args(["clone", "-q", &clone_url, "verify"])
                .current_dir(&verify_dir)
                .status()
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(verify_clone_status.success());
    let verify_path = verify_dir.path().join("verify");
    let remote_tag = tokio::task::spawn_blocking({
        let verify_path = verify_path.clone();
        move || {
            Command::new("git")
                .args(["ls-remote", "--tags", "origin"])
                .current_dir(&verify_path)
                .output()
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(
        String::from_utf8_lossy(&remote_tag.stdout).contains("refs/tags/v1.0.0"),
        "the release's tag must be a real, pushed-visible git tag"
    );

    let carl_sees: serde_json::Value = client
        .get(format!("http://{addr}/api/repositories/{repo_id}/releases"))
        .bearer_auth(carl_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(carl_sees.as_array().unwrap().is_empty());
    let carl_detail = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_id}/releases/v1.0.0"
        ))
        .bearer_auth(carl_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(carl_detail.status(), reqwest::StatusCode::NOT_FOUND);

    let owner_detail: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_id}/releases/v1.0.0"
        ))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        owner_detail["targetCommitSha"].as_str().unwrap(),
        commit_sha
    );

    client
        .patch(format!(
            "http://{addr}/api/repositories/{repo_id}/releases/v1.0.0"
        ))
        .bearer_auth(owner_jwt)
        .json(&json!({ "draft": false }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let carl_sees_now: serde_json::Value = client
        .get(format!("http://{addr}/api/repositories/{repo_id}/releases"))
        .bearer_auth(carl_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(carl_sees_now.as_array().unwrap().len(), 1);

    let asset_bytes = b"binary content, not really a tarball".to_vec();
    let part = reqwest::multipart::Part::bytes(asset_bytes.clone())
        .file_name("archive.tar.gz")
        .mime_str("application/gzip")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);
    let uploaded: serde_json::Value = client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/releases/v1.0.0/assets"
        ))
        .bearer_auth(owner_jwt)
        .multipart(form)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let asset_id = uploaded["id"].as_str().unwrap();
    assert_eq!(uploaded["filename"], "archive.tar.gz");

    let downloaded = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_id}/releases/v1.0.0/assets/{asset_id}"
        ))
        .bearer_auth(carl_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(downloaded.status(), reqwest::StatusCode::OK);
    let downloaded_bytes = downloaded.bytes().await.unwrap();
    assert_eq!(downloaded_bytes.as_ref(), asset_bytes.as_slice());

    let forbidden_delete_asset = client
        .delete(format!(
            "http://{addr}/api/repositories/{repo_id}/releases/v1.0.0/assets/{asset_id}"
        ))
        .bearer_auth(carl_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        forbidden_delete_asset.status(),
        reqwest::StatusCode::NOT_FOUND
    );
    let forbidden_delete_release = client
        .delete(format!(
            "http://{addr}/api/repositories/{repo_id}/releases/v1.0.0"
        ))
        .bearer_auth(carl_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        forbidden_delete_release.status(),
        reqwest::StatusCode::NOT_FOUND
    );

    // 404, not 403: never reveal a release's existence to someone who cannot manage it.
    let forbidden_update = client
        .patch(format!(
            "http://{addr}/api/repositories/{repo_id}/releases/v1.0.0"
        ))
        .bearer_auth(carl_jwt)
        .json(&json!({ "title": "Hijacked title" }))
        .send()
        .await
        .unwrap();
    assert_eq!(forbidden_update.status(), reqwest::StatusCode::NOT_FOUND);
}

/// The target commit is resolved live from git, so a deleted tag must degrade to `targetCommitSha: null`, not a 500.
#[sqlx::test]
async fn a_release_stays_viewable_with_a_null_target_commit_after_its_git_tag_is_deleted(
    pool: PgPool,
) {
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

    let storage_dir = tempfile::tempdir().unwrap();
    let static_dir = tempfile::tempdir().unwrap();
    std::fs::write(static_dir.path().join("index.html"), "<html></html>").unwrap();

    let config = Config {
        database_url: String::new(),
        jwt_secret: "test-secret-that-is-at-least-32-characters-long".to_string(),
        storage_root: storage_dir.path().to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:0".to_string(),
        static_dir: static_dir.path().to_string_lossy().to_string(),
        bootstrap_admin_username: Some("admin".to_string()),
        bootstrap_admin_password: Some("adminpassword123".to_string()),
        settings_encryption_key: [b'k'; 32],
        public_url: "http://localhost:4200".to_string(),
        trusted_proxy_cidrs: vec![],
    };

    let mut state = AppState::new(pool, config.clone()).await;
    state.mfa_enforced = false; // these tests are not about MFA: they log in with a plain session
    BootstrapAdminUseCase::new(state.users.clone(), state.hasher.clone())
        .execute(
            config.bootstrap_admin_username.clone(),
            config.bootstrap_admin_password.clone(),
        )
        .await
        .unwrap();

    let app = build_router(state, static_dir.path());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();

    let owner_login: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "admin", "password": "adminpassword123" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let owner_jwt = owner_login["token"].as_str().unwrap();

    let token_res: serde_json::Value = client
        .post(format!("http://{addr}/api/tokens"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": "ci" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let plain_token = token_res["token"].as_str().unwrap();

    let repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": "hello", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let repo_id = repo_res["id"].as_str().unwrap();

    let clone_parent = tempfile::tempdir().unwrap();
    let clone_url = format!("http://admin:{plain_token}@{addr}/admin/hello.git");
    let repo_path = clone_parent.path().join("repo");
    let clone_status = tokio::task::spawn_blocking({
        let clone_url = clone_url.clone();
        let clone_dir = clone_parent.path().to_path_buf();
        move || {
            Command::new("git")
                .args(["clone", &clone_url, "repo"])
                .current_dir(&clone_dir)
                .status()
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(clone_status.success());

    std::fs::write(repo_path.join("README.md"), "hello\n").unwrap();
    let add_status = tokio::task::spawn_blocking({
        let repo_path = repo_path.clone();
        move || {
            Command::new("git")
                .args(["add", "."])
                .current_dir(&repo_path)
                .status()
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(add_status.success());
    let commit_status = tokio::task::spawn_blocking({
        let repo_path = repo_path.clone();
        move || {
            Command::new("git")
                .args([
                    "-c",
                    "user.email=t@t.com",
                    "-c",
                    "user.name=t",
                    "commit",
                    "-q",
                    "-m",
                    "root",
                ])
                .current_dir(&repo_path)
                .status()
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(commit_status.success());
    let push_status = tokio::task::spawn_blocking({
        let repo_path = repo_path.clone();
        move || {
            Command::new("git")
                .args(["push", "-q", "origin", "HEAD:main"])
                .current_dir(&repo_path)
                .status()
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(push_status.success());
    let sha_output = tokio::task::spawn_blocking({
        let repo_path = repo_path.clone();
        move || {
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&repo_path)
                .output()
        }
    })
    .await
    .unwrap()
    .unwrap();
    let commit_sha = String::from_utf8_lossy(&sha_output.stdout)
        .trim()
        .to_string();

    let release: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/releases"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "tagName": "v1.0.0", "targetCommitSha": commit_sha, "title": "First release", "draft": false }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(release["tagName"], "v1.0.0");

    let delete_tag_status = tokio::task::spawn_blocking({
        let repo_path = repo_path.clone();
        move || {
            Command::new("git")
                .args(["push", "-q", "origin", "--delete", "v1.0.0"])
                .current_dir(&repo_path)
                .status()
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(delete_tag_status.success());

    let detail = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_id}/releases/v1.0.0"
        ))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        detail.status(),
        reqwest::StatusCode::OK,
        "the release must stay viewable, not 500, after its tag is deleted"
    );
    let detail: serde_json::Value = detail.json().await.unwrap();
    assert!(detail["targetCommitSha"].is_null());
    assert_eq!(detail["tagName"], "v1.0.0");
}

/// Regression: deleting a release never touched its git tag, leaving it stuck. Tag deletion is Maintainer+ only and refused while a release references it.
#[sqlx::test]
async fn deleting_a_tag_is_maintainer_gated_and_refused_while_a_release_still_uses_it(
    pool: PgPool,
) {
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

    let storage_dir = tempfile::tempdir().unwrap();
    let static_dir = tempfile::tempdir().unwrap();
    std::fs::write(static_dir.path().join("index.html"), "<html></html>").unwrap();

    let config = Config {
        database_url: String::new(),
        jwt_secret: "test-secret-that-is-at-least-32-characters-long".to_string(),
        storage_root: storage_dir.path().to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:0".to_string(),
        static_dir: static_dir.path().to_string_lossy().to_string(),
        bootstrap_admin_username: Some("admin".to_string()),
        bootstrap_admin_password: Some("adminpassword123".to_string()),
        settings_encryption_key: [b'k'; 32],
        public_url: "http://localhost:4200".to_string(),
        trusted_proxy_cidrs: vec![],
    };

    let mut state = AppState::new(pool, config.clone()).await;
    state.mfa_enforced = false; // these tests are not about MFA: they log in with a plain session
    BootstrapAdminUseCase::new(state.users.clone(), state.hasher.clone())
        .execute(
            config.bootstrap_admin_username.clone(),
            config.bootstrap_admin_password.clone(),
        )
        .await
        .unwrap();

    let app = build_router(state, static_dir.path());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();

    let owner_login: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "admin", "password": "adminpassword123" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let owner_jwt = owner_login["token"].as_str().unwrap();

    let token_res: serde_json::Value = client
        .post(format!("http://{addr}/api/tokens"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": "ci" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let plain_token = token_res["token"].as_str().unwrap();

    let repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": "hello", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let repo_id = repo_res["id"].as_str().unwrap();

    client.post(format!("http://{addr}/api/admin/users")).bearer_auth(owner_jwt).json(&json!({ "username": "contributor-carl", "email": "carl@example.com", "password": "password12345" })).send().await.unwrap().error_for_status().unwrap();
    client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/collaborators"
        ))
        .bearer_auth(owner_jwt)
        .json(&json!({ "username": "contributor-carl", "role": "contributor" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let carl_login: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "contributor-carl", "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let carl_jwt = carl_login["token"].as_str().unwrap();

    let clone_parent = tempfile::tempdir().unwrap();
    let clone_url = format!("http://admin:{plain_token}@{addr}/admin/hello.git");
    let repo_path = clone_parent.path().join("repo");
    let clone_status = tokio::task::spawn_blocking({
        let clone_url = clone_url.clone();
        let clone_dir = clone_parent.path().to_path_buf();
        move || {
            Command::new("git")
                .args(["clone", &clone_url, "repo"])
                .current_dir(&clone_dir)
                .status()
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(clone_status.success());

    std::fs::write(repo_path.join("README.md"), "hello\n").unwrap();
    let add_status = tokio::task::spawn_blocking({
        let repo_path = repo_path.clone();
        move || {
            Command::new("git")
                .args(["add", "."])
                .current_dir(&repo_path)
                .status()
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(add_status.success());
    let commit_status = tokio::task::spawn_blocking({
        let repo_path = repo_path.clone();
        move || {
            Command::new("git")
                .args([
                    "-c",
                    "user.email=t@t.com",
                    "-c",
                    "user.name=t",
                    "commit",
                    "-q",
                    "-m",
                    "root",
                ])
                .current_dir(&repo_path)
                .status()
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(commit_status.success());
    let push_status = tokio::task::spawn_blocking({
        let repo_path = repo_path.clone();
        move || {
            Command::new("git")
                .args(["push", "-q", "origin", "HEAD:main"])
                .current_dir(&repo_path)
                .status()
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(push_status.success());
    let sha_output = tokio::task::spawn_blocking({
        let repo_path = repo_path.clone();
        move || {
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&repo_path)
                .output()
        }
    })
    .await
    .unwrap()
    .unwrap();
    let commit_sha = String::from_utf8_lossy(&sha_output.stdout)
        .trim()
        .to_string();

    client
        .post(format!("http://{addr}/api/repositories/{repo_id}/releases"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "tagName": "v1.0.0", "targetCommitSha": commit_sha, "title": "Wrong-commit draft", "draft": true }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let forbidden = client
        .delete(format!(
            "http://{addr}/api/repositories/{repo_id}/tags/v1.0.0"
        ))
        .bearer_auth(carl_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        forbidden.status(),
        reqwest::StatusCode::NOT_FOUND,
        "insufficient role maps to 404 in this codebase, not 403"
    );

    let conflict = client
        .delete(format!(
            "http://{addr}/api/repositories/{repo_id}/tags/v1.0.0"
        ))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(conflict.status(), reqwest::StatusCode::CONFLICT);

    client
        .delete(format!(
            "http://{addr}/api/repositories/{repo_id}/releases/v1.0.0"
        ))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let tags_after_release_delete: Vec<serde_json::Value> = client
        .get(format!("http://{addr}/api/repositories/{repo_id}/tags"))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        tags_after_release_delete.len(),
        1,
        "the tag itself must still exist — deleting a release never deletes its tag"
    );

    let deleted = client
        .delete(format!(
            "http://{addr}/api/repositories/{repo_id}/tags/v1.0.0"
        ))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), reqwest::StatusCode::NO_CONTENT);

    let tags_after_tag_delete: Vec<serde_json::Value> = client
        .get(format!("http://{addr}/api/repositories/{repo_id}/tags"))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        tags_after_tag_delete.is_empty(),
        "the tag must actually be gone"
    );

    let already_gone = client
        .delete(format!(
            "http://{addr}/api/repositories/{repo_id}/tags/v1.0.0"
        ))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(already_gone.status(), reqwest::StatusCode::NOT_FOUND);
}
