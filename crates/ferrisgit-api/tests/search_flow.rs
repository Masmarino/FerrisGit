use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use serde_json::json;
use sqlx::PgPool;
use std::path::Path;
use std::process::Command;

async fn git(args: &[&str], cwd: &Path) -> String {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let cwd = cwd.to_path_buf();
    let output = tokio::task::spawn_blocking(move || {
        Command::new("git").args(&args).current_dir(&cwd).output()
    })
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

async fn commit_file(repo_path: &Path, file: &str, content: &str, message: &str) {
    std::fs::write(repo_path.join(file), content).unwrap();
    git(&["add", "."], repo_path).await;
    git(
        &[
            "-c",
            "user.email=t@t.com",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            message,
        ],
        repo_path,
    )
    .await;
}

async fn push_main_and_feature(
    addr: std::net::SocketAddr,
    plain_token: &str,
    repo_name: &str,
) -> tempfile::TempDir {
    let clone_parent = tempfile::tempdir().unwrap();
    let clone_url = format!("http://admin:{plain_token}@{addr}/admin/{repo_name}.git");
    let repo_path = clone_parent.path().join("repo");
    git(&["clone", "-q", &clone_url, "repo"], clone_parent.path()).await;
    commit_file(&repo_path, "README.md", "line one\n", "root").await;
    git(&["push", "-q", "origin", "HEAD:main"], &repo_path).await;
    git(&["checkout", "-q", "-b", "feature"], &repo_path).await;
    commit_file(
        &repo_path,
        "README.md",
        "line one\nline two\n",
        "feature work",
    )
    .await;
    git(&["push", "-q", "origin", "HEAD:feature"], &repo_path).await;
    clone_parent
}

fn assert_created_at(actual: &serde_json::Value, expected: &serde_json::Value, what: &str) {
    let actual = actual
        .as_str()
        .unwrap_or_else(|| panic!("{what}: createdAt must be a string, got {actual}"));
    let expected = expected.as_str().unwrap();
    let parsed = chrono::DateTime::parse_from_rfc3339(actual)
        .unwrap_or_else(|e| panic!("{what}: createdAt {actual:?} is not RFC 3339: {e}"));
    assert_eq!(
        parsed,
        chrono::DateTime::parse_from_rfc3339(expected).unwrap(),
        "{what}: createdAt must equal the item's own creation time"
    );
}

#[sqlx::test]
async fn search_results_are_scoped_to_repository_access(pool: PgPool) {
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

    for (username, password) in [
        ("alice-collaborator", "password12345"),
        ("group-member", "password12345"),
        ("stranger-outsider", "password12345"),
    ] {
        client
            .post(format!("http://{addr}/api/admin/users"))
            .bearer_auth(owner_jwt)
            .json(&json!({ "username": username, "email": format!("{username}@example.com"), "password": password }))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
    }

    let repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": "hush-hush", "description": "top secret widget project", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let repo_id = repo_res["id"].as_str().unwrap();

    client
        .post(format!("http://{addr}/api/repositories/{repo_id}/issues"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "title": "gizmo-flavored bug in the widget project", "description": "", "kind": "bug" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/collaborators"
        ))
        .bearer_auth(owner_jwt)
        .json(&json!({ "username": "alice-collaborator", "role": "reader" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let group_res: serde_json::Value = client
        .post(format!("http://{addr}/api/groups"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": "widgets-group", "description": "" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let group_id = group_res["id"].as_str().unwrap();

    client
        .post(format!("http://{addr}/api/groups/{group_id}/members"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "username": "group-member", "role": "reader" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    // No route moves an existing repo, so create a second group-owned repository with the same searchable title.
    let group_repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": "hush-hush-group", "description": "top secret widget project", "visibility": "private", "groupPath": "widgets-group" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let group_repo_id = group_repo_res["id"].as_str().unwrap();
    client
        .post(format!("http://{addr}/api/repositories/{group_repo_id}/issues"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "title": "gizmo-flavored bug in the group project", "description": "", "kind": "bug" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let public_repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": "open-widget", "description": "top secret widget project", "visibility": "public" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let public_repo_id = public_repo_res["id"].as_str().unwrap();
    let public_issue: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories/{public_repo_id}/issues"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "title": "gizmo-flavored bug in the open project", "description": "", "kind": "bug" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();

    let token: serde_json::Value = client
        .post(format!("http://{addr}/api/tokens"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": "ci" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let _clone_dir =
        push_main_and_feature(addr, token["token"].as_str().unwrap(), "open-widget").await;
    let public_mr: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories/{public_repo_id}/merge-requests"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "sourceBranch": "feature", "targetBranch": "main", "title": "gizmo-flavored line two", "description": "" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();

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

    async fn search(
        client: &reqwest::Client,
        addr: std::net::SocketAddr,
        jwt: &str,
        q: &str,
    ) -> serde_json::Value {
        client
            .get(format!("http://{addr}/api/search"))
            .bearer_auth(jwt)
            .query(&[("q", q)])
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    let alice_jwt = login(&client, addr, "alice-collaborator").await;
    let alice_results = search(&client, addr, &alice_jwt, "gizmo-flavored").await;
    assert_eq!(
        alice_results["issues"].as_array().unwrap().len(),
        2,
        "collaborator should see their own repo's issue plus the public repo's issue"
    );

    let group_member_jwt = login(&client, addr, "group-member").await;
    let group_member_results = search(&client, addr, &group_member_jwt, "gizmo-flavored").await;
    let group_member_titles: Vec<&str> = group_member_results["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].as_str().unwrap())
        .collect();
    assert_eq!(
        group_member_titles.len(),
        2,
        "group member should see the group's issue plus the public repo's issue"
    );
    assert!(group_member_titles.contains(&"gizmo-flavored bug in the group project"));
    assert!(group_member_titles.contains(&"gizmo-flavored bug in the open project"));

    let stranger_jwt = login(&client, addr, "stranger-outsider").await;
    let stranger_results = search(&client, addr, &stranger_jwt, "gizmo-flavored").await;
    assert_eq!(
        stranger_results["issues"].as_array().unwrap().len(),
        1,
        "a stranger should see only the public repo's issue, not either private repo's issue"
    );
    assert_eq!(
        stranger_results["issues"][0]["title"], "gizmo-flavored bug in the open project",
        "the ONE visible issue must be the public repo's, not either private one leaking through"
    );

    let stranger_issue = &stranger_results["issues"][0];
    assert_eq!(stranger_issue["id"], public_issue["id"]);
    assert_eq!(stranger_issue["number"], 1);
    assert_eq!(stranger_issue["kind"], "bug");
    assert_eq!(stranger_issue["status"], public_issue["status"]);
    assert_eq!(
        stranger_issue["repository"]["path"],
        json!(["admin", "open-widget"])
    );
    assert_created_at(
        &stranger_issue["createdAt"],
        &public_issue["createdAt"],
        "issue result",
    );

    let stranger_mrs = stranger_results["mergeRequests"].as_array().unwrap();
    assert_eq!(stranger_mrs.len(), 1);
    assert_eq!(stranger_mrs[0]["id"], public_mr["id"]);
    assert_eq!(stranger_mrs[0]["title"], "gizmo-flavored line two");
    assert_eq!(stranger_mrs[0]["sourceBranch"], "feature");
    assert_eq!(stranger_mrs[0]["targetBranch"], "main");
    assert_eq!(stranger_mrs[0]["status"], public_mr["status"]);
    assert_eq!(
        stranger_mrs[0]["repository"]["path"],
        json!(["admin", "open-widget"])
    );
    assert_created_at(
        &stranger_mrs[0]["createdAt"],
        &public_mr["createdAt"],
        "merge request result",
    );

    let stranger_user_search = search(&client, addr, &stranger_jwt, "alice-collaborator").await;
    assert_eq!(stranger_user_search["users"].as_array().unwrap().len(), 1);
    assert_eq!(
        stranger_user_search["users"][0]["username"],
        "alice-collaborator"
    );
}
