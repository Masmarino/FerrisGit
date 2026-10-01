use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use serde_json::json;
use sqlx::PgPool;
use std::process::Command;

#[sqlx::test]
async fn a_collaborator_can_use_a_repo_they_do_not_own_and_a_stranger_cannot(pool: PgPool) {
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
    assert_eq!(repo_res["owner"], "admin");
    assert_eq!(repo_res["role"], "owner");
    let repo_id = repo_res["id"].as_str().unwrap().to_string();

    for username in ["alice", "stranger"] {
        client
            .post(format!("http://{addr}/api/admin/users"))
            .bearer_auth(owner_jwt)
            .json(&json!({ "username": username, "email": format!("{username}@example.com"), "password": "password12345" }))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
    }

    let alice_login: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "alice", "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let alice_jwt = alice_login["token"].as_str().unwrap();

    let stranger_login: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "stranger", "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let stranger_jwt = stranger_login["token"].as_str().unwrap();

    let before_status = client
        .get(format!("http://{addr}/api/repositories/admin/hello"))
        .bearer_auth(alice_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(before_status, 404);

    client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/collaborators"
        ))
        .bearer_auth(owner_jwt)
        .json(&json!({ "username": "alice", "role": "maintainer" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let collaborators_list: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_id}/collaborators"
        ))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let usernames: Vec<&str> = collaborators_list
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["username"].as_str().unwrap())
        .collect();
    assert_eq!(usernames, vec!["alice"]);

    let alice_repos: serde_json::Value = client
        .get(format!("http://{addr}/api/repositories"))
        .bearer_auth(alice_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let alice_repos = alice_repos.as_array().unwrap();
    assert_eq!(alice_repos.len(), 1);
    assert_eq!(alice_repos[0]["name"], "hello");
    assert_eq!(alice_repos[0]["owner"], "admin");
    assert_eq!(alice_repos[0]["role"], "maintainer");

    client
        .put(format!("http://{addr}/api/repositories/{repo_id}/settings"))
        .bearer_auth(alice_jwt)
        .json(&json!({ "ciEnabled": true }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/ci-variables"
        ))
        .bearer_auth(alice_jwt)
        .json(&json!({ "key": "SECRET", "value": "s3cr3t", "masked": true }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let alice_token_res: serde_json::Value = client
        .post(format!("http://{addr}/api/tokens"))
        .bearer_auth(alice_jwt)
        .json(&json!({ "name": "alice-ci" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let alice_plain_token = alice_token_res["token"].as_str().unwrap();

    let clone_parent = tempfile::tempdir().unwrap();
    let clone_url = format!("http://alice:{alice_plain_token}@{addr}/admin/hello.git");
    let repo_path = clone_parent.path().join("repo");
    let run_git = |args: Vec<String>, cwd: std::path::PathBuf| {
        tokio::task::spawn_blocking(move || {
            Command::new("git").args(&args).current_dir(&cwd).status()
        })
    };
    assert!(
        run_git(
            vec!["clone".to_string(), clone_url, "repo".to_string()],
            clone_parent.path().to_path_buf()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    std::fs::write(repo_path.join("README.md"), "hello from alice\n").unwrap();
    assert!(
        run_git(vec!["add".to_string(), ".".to_string()], repo_path.clone())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    assert!(
        run_git(
            vec![
                "-c".to_string(),
                "user.email=a@a.com".to_string(),
                "-c".to_string(),
                "user.name=alice".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "alice's commit".to_string()
            ],
            repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    assert!(
        run_git(
            vec![
                "push".to_string(),
                "-q".to_string(),
                "origin".to_string(),
                "HEAD:main".to_string()
            ],
            repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    assert!(
        run_git(
            vec![
                "checkout".to_string(),
                "-q".to_string(),
                "-b".to_string(),
                "feature".to_string()
            ],
            repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    std::fs::write(
        repo_path.join("README.md"),
        "hello from alice\nfeature line\n",
    )
    .unwrap();
    assert!(
        run_git(vec!["add".to_string(), ".".to_string()], repo_path.clone())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    assert!(
        run_git(
            vec![
                "-c".to_string(),
                "user.email=a@a.com".to_string(),
                "-c".to_string(),
                "user.name=alice".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "feature work".to_string()
            ],
            repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    assert!(
        run_git(
            vec![
                "push".to_string(),
                "-q".to_string(),
                "origin".to_string(),
                "HEAD:feature".to_string()
            ],
            repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    let mr_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/merge-requests"))
        .bearer_auth(alice_jwt)
        .json(&json!({ "sourceBranch": "feature", "targetBranch": "main", "title": "Add feature", "description": "" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mr_id = mr_res["id"].as_str().unwrap();
    let merge_res = client
        .post(format!("http://{addr}/api/merge-requests/{mr_id}/merge"))
        .bearer_auth(alice_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(merge_res.status(), 200);

    assert_eq!(
        client
            .get(format!("http://{addr}/api/repositories/admin/hello"))
            .bearer_auth(stranger_jwt)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        client
            .post(format!("http://{addr}/api/repositories/{repo_id}/merge-requests"))
            .bearer_auth(stranger_jwt)
            .json(&json!({ "sourceBranch": "feature", "targetBranch": "main", "title": "x", "description": "" }))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );

    client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/collaborators"
        ))
        .bearer_auth(alice_jwt)
        .json(&json!({ "username": "stranger", "role": "reader" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    client
        .delete(format!(
            "http://{addr}/api/repositories/{repo_id}/collaborators/alice"
        ))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    assert_eq!(
        client
            .get(format!("http://{addr}/api/repositories/admin/hello"))
            .bearer_auth(alice_jwt)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
}

/// The public Reader bypass must not expose who the collaborators are (a product decision).
#[sqlx::test]
async fn a_public_repos_collaborator_list_is_not_exposed_to_a_non_collaborator(pool: PgPool) {
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

    let repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": "hello", "visibility": "public" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let repo_id = repo_res["id"].as_str().unwrap().to_string();

    client
        .post(format!("http://{addr}/api/admin/users"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "username": "stranger", "email": "stranger@example.com", "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let stranger_login: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "stranger", "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let stranger_jwt = stranger_login["token"].as_str().unwrap();

    assert_eq!(
        client
            .get(format!("http://{addr}/api/repositories/admin/hello"))
            .bearer_auth(stranger_jwt)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    assert_eq!(
        client
            .get(format!(
                "http://{addr}/api/repositories/{repo_id}/collaborators"
            ))
            .bearer_auth(stranger_jwt)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );

    let owner_view = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_id}/collaborators"
        ))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(owner_view.status(), 200);
}
