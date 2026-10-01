use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use serde_json::json;
use sqlx::PgPool;
use std::process::Command;

#[sqlx::test]
async fn nested_group_member_can_clone_and_push_while_creator_loses_access_after_role_removal(
    pool: PgPool,
) {
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

    let storage_dir = tempfile::tempdir().unwrap();
    let static_dir = tempfile::tempdir().unwrap();
    std::fs::write(static_dir.path().join("index.html"), "<html>spa</html>").unwrap();

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

    let admin_login: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "admin", "password": "adminpassword123" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let admin_jwt = admin_login["token"].as_str().unwrap();

    for username in ["creator", "member", "colead"] {
        client
            .post(format!("http://{addr}/api/admin/users"))
            .bearer_auth(admin_jwt)
            .json(&json!({ "username": username, "email": format!("{username}@example.com"), "password": "password12345" }))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
    }

    async fn login(client: &reqwest::Client, addr: std::net::SocketAddr, username: &str) -> String {
        let res: serde_json::Value = client
            .post(format!("http://{addr}/api/auth/login"))
            .json(&json!({ "username": username, "password": "password12345" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        res["token"].as_str().unwrap().to_string()
    }

    async fn api_token(client: &reqwest::Client, addr: std::net::SocketAddr, jwt: &str) -> String {
        let res: serde_json::Value = client
            .post(format!("http://{addr}/api/tokens"))
            .bearer_auth(jwt)
            .json(&json!({ "name": "ci" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        res["token"].as_str().unwrap().to_string()
    }

    let creator_jwt = login(&client, addr, "creator").await;
    let member_jwt = login(&client, addr, "member").await;

    let creator_token = api_token(&client, addr, &creator_jwt).await;
    let member_token = api_token(&client, addr, &member_jwt).await;

    let acme_res = client
        .post(format!("http://{addr}/api/groups"))
        .bearer_auth(&creator_jwt)
        .json(&json!({ "name": "acme", "description": "Acme Corp" }))
        .send()
        .await
        .unwrap();
    assert_eq!(acme_res.status(), 200);
    let acme_body: serde_json::Value = acme_res.json().await.unwrap();
    let acme_id = acme_body["id"].as_str().unwrap();

    // `CreateGroupUseCase` auto-adds the caller as a direct Maintainer of every group it creates, so `creator` holds two memberships that must both be removed later.

    let backend_res = client
        .post(format!("http://{addr}/api/groups/{acme_id}/subgroups"))
        .bearer_auth(&creator_jwt)
        .json(&json!({ "name": "backend", "description": "Backend team" }))
        .send()
        .await
        .unwrap();
    assert_eq!(backend_res.status(), 200);
    let backend_body: serde_json::Value = backend_res.json().await.unwrap();
    let backend_id = backend_body["id"].as_str().unwrap();

    let add_member_status = client
        .post(format!("http://{addr}/api/groups/{backend_id}/members"))
        .bearer_auth(&creator_jwt)
        .json(&json!({ "username": "member", "role": "contributor" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(add_member_status, 200);

    // `colead` is a second Maintainer only so that removing `creator`'s roles is permitted (the last Maintainer of a hierarchy cannot be removed).
    let add_colead_status = client
        .post(format!("http://{addr}/api/groups/{acme_id}/members"))
        .bearer_auth(&creator_jwt)
        .json(&json!({ "username": "colead", "role": "maintainer" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(add_colead_status, 200);

    let create_repo_res = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(&creator_jwt)
        .json(&json!({ "name": "terraform-modules", "visibility": "private", "groupPath": "acme/backend" }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_repo_res.status(), 200);

    let clone_parent = tempfile::tempdir().unwrap();
    let member_clone_url =
        format!("http://member:{member_token}@{addr}/acme/backend/terraform-modules.git");

    let member_clone_status = tokio::task::spawn_blocking({
        let url = member_clone_url.clone();
        let dir = clone_parent.path().to_path_buf();
        move || {
            Command::new("git")
                .args(["clone", &url, "member-repo"])
                .current_dir(&dir)
                .status()
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(
        member_clone_status.success(),
        "a group Contributor with only a subgroup-level grant must be able to clone a repository two levels deep"
    );

    let member_repo_path = clone_parent.path().join("member-repo");
    std::fs::write(member_repo_path.join("main.tf"), "# terraform").unwrap();

    let add_status = tokio::task::spawn_blocking({
        let repo_path = member_repo_path.clone();
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
        let repo_path = member_repo_path.clone();
        move || {
            Command::new("git")
                .args([
                    "-c",
                    "user.email=member@example.com",
                    "-c",
                    "user.name=member",
                    "commit",
                    "-q",
                    "-m",
                    "add terraform module",
                ])
                .current_dir(&repo_path)
                .status()
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(commit_status.success());

    let member_push_status = tokio::task::spawn_blocking({
        let repo_path = member_repo_path.clone();
        move || {
            Command::new("git")
                .args(["push", "origin", "HEAD:main"])
                .current_dir(&repo_path)
                .status()
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(
        member_push_status.success(),
        "a group Contributor with only a subgroup-level grant must be able to push to a repository two levels deep"
    );

    let resolve_res: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/resolve/acme/backend/terraform-modules"
        ))
        .bearer_auth(&member_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let repository_id = resolve_res["repositoryId"].as_str().unwrap().to_string();

    // `creator` keeps `owner_id` but has no group role: the clone must be rejected (no permanent owner bypass).

    let remove_creator_from_acme_status = client
        .delete(format!(
            "http://{addr}/api/groups/{acme_id}/members/creator"
        ))
        .bearer_auth(&creator_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        remove_creator_from_acme_status, 200,
        "removing creator's role on the root group must succeed"
    );

    let remove_creator_from_backend_status = client
        .delete(format!(
            "http://{addr}/api/groups/{backend_id}/members/creator"
        ))
        .bearer_auth(&creator_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        remove_creator_from_backend_status, 200,
        "removing creator's direct role on the subgroup must succeed"
    );

    let creator_by_id_status = client
        .get(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}"
        ))
        .bearer_auth(&creator_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        creator_by_id_status, 404,
        "the creator must lose API read access too once their only group roles are removed — owner_id must grant no implicit access on a group repository"
    );

    let creator_clone_url =
        format!("http://creator:{creator_token}@{addr}/acme/backend/terraform-modules.git");
    let creator_clone_output = tokio::task::spawn_blocking({
        let url = creator_clone_url.clone();
        let dir = clone_parent.path().to_path_buf();
        move || {
            Command::new("git")
                .args(["clone", &url, "creator-repo"])
                .current_dir(&dir)
                .output()
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(
        !creator_clone_output.status.success(),
        "the repository creator must be rejected after their only group roles are removed — owner_id must grant no implicit access on a group repository, got stdout={} stderr={}",
        String::from_utf8_lossy(&creator_clone_output.stdout),
        String::from_utf8_lossy(&creator_clone_output.stderr)
    );
    let creator_clone_stderr = String::from_utf8_lossy(&creator_clone_output.stderr).to_lowercase();
    assert!(
        creator_clone_stderr.contains("401")
            || creator_clone_stderr.contains("auth")
            || creator_clone_stderr.contains("denied")
            || creator_clone_stderr.contains("fatal"),
        "expected an authentication-failure indication in git's stderr, got: {creator_clone_stderr}"
    );

    // The git smart-HTTP endpoint itself must answer the standard git 401 challenge, not only the git CLI.
    let creator_info_refs_res = client
        .get(format!(
            "http://{addr}/acme/backend/terraform-modules.git/info/refs?service=git-upload-pack"
        ))
        .basic_auth("creator", Some(&creator_token))
        .send()
        .await
        .unwrap();
    assert_eq!(
        creator_info_refs_res.status(),
        401,
        "the creator's token must be rejected by the git smart-HTTP endpoint directly, not just via the git CLI wrapper"
    );
    assert!(
        creator_info_refs_res
            .headers()
            .contains_key(axum::http::header::WWW_AUTHENTICATE),
        "a 401 to a git client must carry the WWW-Authenticate challenge header"
    );

    // A sub-page URL without `.git` must fall through to the SPA, not be routed into `git_smart_http`.

    let spa_res = client
        .get(format!("http://{addr}/acme/backend/terraform-modules"))
        .send()
        .await
        .unwrap();
    assert_ne!(
        spa_res.status(),
        401,
        "a non-.git repository sub-page URL must never be treated as a git request (no WWW-Authenticate challenge)"
    );
    assert!(
        !spa_res
            .headers()
            .contains_key(axum::http::header::WWW_AUTHENTICATE),
        "the SPA fallback must never emit a git WWW-Authenticate challenge"
    );
    let spa_body = spa_res.text().await.unwrap();
    assert_eq!(
        spa_body, "<html>spa</html>",
        "a .git-less repository sub-page URL must be served the SPA's index.html, not dispatched to the git handler"
    );
}
