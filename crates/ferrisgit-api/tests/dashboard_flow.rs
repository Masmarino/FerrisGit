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
async fn dashboard_lists_an_issue_assigned_to_the_caller(pool: PgPool) {
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

    let contributor: serde_json::Value = client
        .post(format!("http://{addr}/api/admin/users"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "username": "contributor", "email": "contributor@example.com", "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let contributor_id = contributor["id"].as_str().unwrap();

    let repo: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": "hello", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let repo_id = repo["id"].as_str().unwrap();

    client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/collaborators"
        ))
        .bearer_auth(owner_jwt)
        .json(&json!({ "username": "contributor", "role": "contributor" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let created_issue: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/issues"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "title": "Fix the thing", "description": "", "kind": "bug" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();

    client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/issues/1/assign"
        ))
        .bearer_auth(owner_jwt)
        .json(&json!({ "assigneeId": contributor_id }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let contributor_login: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "contributor", "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let contributor_jwt = contributor_login["token"].as_str().unwrap();

    let dashboard: serde_json::Value = client
        .get(format!("http://{addr}/api/dashboard"))
        .bearer_auth(contributor_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let assigned = dashboard["assignedIssues"].as_array().unwrap();
    assert_eq!(assigned.len(), 1);
    assert_eq!(assigned[0]["title"], "Fix the thing");
    assert_eq!(assigned[0]["number"], 1);
    assert_eq!(assigned[0]["repository"]["path"], json!(["admin", "hello"]));
    assert_eq!(assigned[0]["status"], created_issue["status"]);
    assert_eq!(assigned[0]["kind"], "bug");
    assert_eq!(assigned[0]["id"], created_issue["id"]);
    assert_created_at(
        &assigned[0]["createdAt"],
        &created_issue["createdAt"],
        "assigned issue",
    );

    let owner_dashboard: serde_json::Value = client
        .get(format!("http://{addr}/api/dashboard"))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let authored = owner_dashboard["authoredIssues"].as_array().unwrap();
    assert_eq!(authored.len(), 1);
    assert_eq!(authored[0]["title"], "Fix the thing");
    assert_created_at(
        &authored[0]["createdAt"],
        &created_issue["createdAt"],
        "authored issue",
    );

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
    let _clone_dir = push_main_and_feature(addr, token["token"].as_str().unwrap(), "hello").await;
    let created_mr: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/merge-requests"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "sourceBranch": "feature", "targetBranch": "main", "title": "Add line two", "description": "" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();

    let owner_dashboard: serde_json::Value = client
        .get(format!("http://{addr}/api/dashboard"))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let authored_mrs = owner_dashboard["authoredMergeRequests"].as_array().unwrap();
    assert_eq!(authored_mrs.len(), 1);
    assert_eq!(authored_mrs[0]["title"], "Add line two");
    assert_eq!(authored_mrs[0]["sourceBranch"], "feature");
    assert_eq!(authored_mrs[0]["targetBranch"], "main");
    assert_eq!(authored_mrs[0]["status"], created_mr["status"]);
    assert_created_at(
        &authored_mrs[0]["createdAt"],
        &created_mr["createdAt"],
        "authored merge request",
    );

    let contributor_dashboard: serde_json::Value = client
        .get(format!("http://{addr}/api/dashboard"))
        .bearer_auth(contributor_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let to_review = contributor_dashboard["mergeRequestsToReview"]
        .as_array()
        .unwrap();
    assert_eq!(to_review.len(), 1);
    assert_eq!(to_review[0]["title"], "Add line two");
    assert_eq!(
        to_review[0]["repository"]["path"],
        json!(["admin", "hello"])
    );
    assert_created_at(
        &to_review[0]["createdAt"],
        &created_mr["createdAt"],
        "merge request to review",
    );
}
