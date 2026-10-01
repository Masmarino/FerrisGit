use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use serde_json::json;
use sqlx::PgPool;
use std::process::Command;

#[sqlx::test]
async fn role_gating_matches_reader_contributor_maintainer_across_git_settings_and_merge_requests(
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
    let run_git = |args: Vec<String>, cwd: std::path::PathBuf| {
        tokio::task::spawn_blocking(move || {
            Command::new("git").args(&args).current_dir(&cwd).status()
        })
    };
    let run_git_output = |args: Vec<String>, cwd: std::path::PathBuf| {
        tokio::task::spawn_blocking(move || {
            Command::new("git").args(&args).current_dir(&cwd).output()
        })
    };

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
    assert_eq!(
        repo_res["path"],
        json!(["admin", "hello"]),
        "a personal repository's resolvable path is [owner_username, name]"
    );
    let repo_id = repo_res["id"].as_str().unwrap().to_string();

    let owner_token_res: serde_json::Value = client
        .post(format!("http://{addr}/api/tokens"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": "owner-ci" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let owner_plain_token = owner_token_res["token"].as_str().unwrap();

    let owner_clone_parent = tempfile::tempdir().unwrap();
    let owner_clone_url = format!("http://admin:{owner_plain_token}@{addr}/admin/hello.git");
    let owner_repo_path = owner_clone_parent.path().join("repo");
    assert!(
        run_git(
            vec!["clone".to_string(), owner_clone_url, "repo".to_string()],
            owner_clone_parent.path().to_path_buf()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    std::fs::write(owner_repo_path.join("README.md"), "line one\n").unwrap();
    assert!(
        run_git(
            vec!["add".to_string(), ".".to_string()],
            owner_repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    assert!(
        run_git(
            vec![
                "-c".to_string(),
                "user.email=o@o.com".to_string(),
                "-c".to_string(),
                "user.name=owner".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "root".to_string()
            ],
            owner_repo_path.clone()
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
            owner_repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    let root_sha_output = run_git_output(
        vec!["rev-parse".to_string(), "HEAD".to_string()],
        owner_repo_path.clone(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(root_sha_output.status.success());
    let root_sha = String::from_utf8(root_sha_output.stdout)
        .unwrap()
        .trim()
        .to_string();

    for username in ["reader", "contributor", "maintainer"] {
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

    let reader_jwt = login(&client, addr, "reader").await;
    let contributor_jwt = login(&client, addr, "contributor").await;
    let maintainer_jwt = login(&client, addr, "maintainer").await;

    for (username, role) in [
        ("reader", "reader"),
        ("contributor", "contributor"),
        ("maintainer", "maintainer"),
    ] {
        let status = client
            .post(format!(
                "http://{addr}/api/repositories/{repo_id}/collaborators"
            ))
            .bearer_auth(owner_jwt)
            .json(&json!({ "username": username, "role": role }))
            .send()
            .await
            .unwrap()
            .status();
        assert_eq!(
            status, 204,
            "expected adding {username} as {role} to return 204"
        );
    }

    async fn main_tip_sha(
        client: &reqwest::Client,
        addr: std::net::SocketAddr,
        jwt: &str,
        repo_id: &str,
    ) -> String {
        let branches: serde_json::Value = client
            .get(format!("http://{addr}/api/repositories/{repo_id}/branches"))
            .bearer_auth(jwt)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        branches
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["name"] == "main")
            .unwrap()["tipSha"]
            .as_str()
            .unwrap()
            .to_string()
    }

    let get_repo_status = client
        .get(format!("http://{addr}/api/repositories/admin/hello"))
        .bearer_auth(&reader_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(get_repo_status, 200);

    let reader_token_res: serde_json::Value = client
        .post(format!("http://{addr}/api/tokens"))
        .bearer_auth(&reader_jwt)
        .json(&json!({ "name": "reader-ci" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let reader_plain_token = reader_token_res["token"].as_str().unwrap();

    let sha_before_reader_push = main_tip_sha(&client, addr, &reader_jwt, &repo_id).await;
    assert_eq!(
        sha_before_reader_push, root_sha,
        "sanity check: main's tip should still be the owner's root commit before the reader's push attempt"
    );

    let reader_clone_parent = tempfile::tempdir().unwrap();
    let reader_clone_url = format!("http://reader:{reader_plain_token}@{addr}/admin/hello.git");
    let reader_repo_path = reader_clone_parent.path().join("repo");
    assert!(
        run_git(
            vec!["clone".to_string(), reader_clone_url, "repo".to_string()],
            reader_clone_parent.path().to_path_buf()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    std::fs::write(
        reader_repo_path.join("README.md"),
        "line one\nreader was here\n",
    )
    .unwrap();
    assert!(
        run_git(
            vec!["add".to_string(), ".".to_string()],
            reader_repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    assert!(
        run_git(
            vec![
                "-c".to_string(),
                "user.email=r@r.com".to_string(),
                "-c".to_string(),
                "user.name=reader".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "reader tries to push".to_string()
            ],
            reader_repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    let reader_push_status = run_git(
        vec![
            "push".to_string(),
            "-q".to_string(),
            "origin".to_string(),
            "HEAD:main".to_string(),
        ],
        reader_repo_path.clone(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        !reader_push_status.success(),
        "a reader's git push must be rejected"
    );

    let sha_after_reader_push = main_tip_sha(&client, addr, &reader_jwt, &repo_id).await;
    assert_eq!(
        sha_after_reader_push, root_sha,
        "a rejected reader push must not move main's tip sha"
    );

    let mr_list_before_reader_create: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_id}/merge-requests"
        ))
        .bearer_auth(&reader_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(mr_list_before_reader_create.as_array().unwrap().len(), 0);

    let reader_create_mr_status = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/merge-requests"))
        .bearer_auth(&reader_jwt)
        .json(&json!({ "sourceBranch": "main", "targetBranch": "main", "title": "reader mr", "description": "" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(reader_create_mr_status, 404);

    let mr_list_after_reader_create: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_id}/merge-requests"
        ))
        .bearer_auth(&reader_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        mr_list_after_reader_create.as_array().unwrap().len(),
        0,
        "a rejected merge-request creation must not create anything"
    );

    let reader_settings_status = client
        .get(format!("http://{addr}/api/repositories/{repo_id}/settings"))
        .bearer_auth(&reader_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(reader_settings_status, 404);

    let contributor_token_res: serde_json::Value = client
        .post(format!("http://{addr}/api/tokens"))
        .bearer_auth(&contributor_jwt)
        .json(&json!({ "name": "contributor-ci" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let contributor_plain_token = contributor_token_res["token"].as_str().unwrap();

    let contributor_clone_parent = tempfile::tempdir().unwrap();
    let contributor_clone_url =
        format!("http://contributor:{contributor_plain_token}@{addr}/admin/hello.git");
    let contributor_repo_path = contributor_clone_parent.path().join("repo");
    assert!(
        run_git(
            vec![
                "clone".to_string(),
                contributor_clone_url,
                "repo".to_string()
            ],
            contributor_clone_parent.path().to_path_buf()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    std::fs::write(
        contributor_repo_path.join("README.md"),
        "line one\ncontributor was here\n",
    )
    .unwrap();
    assert!(
        run_git(
            vec!["add".to_string(), ".".to_string()],
            contributor_repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    assert!(
        run_git(
            vec![
                "-c".to_string(),
                "user.email=c@c.com".to_string(),
                "-c".to_string(),
                "user.name=contributor".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "contributor push".to_string()
            ],
            contributor_repo_path.clone()
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
            contributor_repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    let contributor_push_sha_output = run_git_output(
        vec!["rev-parse".to_string(), "HEAD".to_string()],
        contributor_repo_path.clone(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(contributor_push_sha_output.status.success());
    let contributor_push_sha = String::from_utf8(contributor_push_sha_output.stdout)
        .unwrap()
        .trim()
        .to_string();

    let sha_after_contributor_push = main_tip_sha(&client, addr, &contributor_jwt, &repo_id).await;
    assert_eq!(
        sha_after_contributor_push, contributor_push_sha,
        "a successful contributor push must move main's tip sha to the new commit"
    );

    assert!(
        run_git(
            vec![
                "checkout".to_string(),
                "-q".to_string(),
                "-b".to_string(),
                "contributor-feature".to_string()
            ],
            contributor_repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    std::fs::write(contributor_repo_path.join("FEATURE.md"), "feature work\n").unwrap();
    assert!(
        run_git(
            vec!["add".to_string(), ".".to_string()],
            contributor_repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    assert!(
        run_git(
            vec![
                "-c".to_string(),
                "user.email=c@c.com".to_string(),
                "-c".to_string(),
                "user.name=contributor".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "feature work".to_string()
            ],
            contributor_repo_path.clone()
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
                "HEAD:contributor-feature".to_string()
            ],
            contributor_repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    let contributor_mr_res = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/merge-requests"))
        .bearer_auth(&contributor_jwt)
        .json(&json!({ "sourceBranch": "contributor-feature", "targetBranch": "main", "title": "Add feature", "description": "" }))
        .send()
        .await
        .unwrap();
    assert_eq!(contributor_mr_res.status(), 200);
    let contributor_mr_body: serde_json::Value = contributor_mr_res.json().await.unwrap();
    assert_eq!(contributor_mr_body["sourceBranch"], "contributor-feature");
    assert_eq!(contributor_mr_body["status"], "open");

    let contributor_get_settings_status = client
        .get(format!("http://{addr}/api/repositories/{repo_id}/settings"))
        .bearer_auth(&contributor_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(contributor_get_settings_status, 404);

    let settings_before_contributor_put: serde_json::Value = client
        .get(format!("http://{addr}/api/repositories/{repo_id}/settings"))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        settings_before_contributor_put["pipelineFilePath"],
        ".ferrisgit-ci.yml"
    );

    let contributor_put_settings_status = client
        .put(format!("http://{addr}/api/repositories/{repo_id}/settings"))
        .bearer_auth(&contributor_jwt)
        .json(&json!({ "pipelineFilePath": "should-not-apply.yml" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(contributor_put_settings_status, 404);

    let settings_after_contributor_put: serde_json::Value = client
        .get(format!("http://{addr}/api/repositories/{repo_id}/settings"))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        settings_after_contributor_put["pipelineFilePath"], ".ferrisgit-ci.yml",
        "a rejected settings PUT must not change anything"
    );

    let collaborators_before_contributor_add: serde_json::Value = client
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
    assert_eq!(
        collaborators_before_contributor_add
            .as_array()
            .unwrap()
            .len(),
        3
    );

    let contributor_add_collaborator_status = client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/collaborators"
        ))
        .bearer_auth(&contributor_jwt)
        .json(&json!({ "username": "admin", "role": "reader" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(contributor_add_collaborator_status, 404);

    let collaborators_after_contributor_add: serde_json::Value = client
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
    assert_eq!(
        collaborators_after_contributor_add
            .as_array()
            .unwrap()
            .len(),
        3,
        "a rejected add-collaborator must not add anything"
    );

    let maintainer_get_settings_status = client
        .get(format!("http://{addr}/api/repositories/{repo_id}/settings"))
        .bearer_auth(&maintainer_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(maintainer_get_settings_status, 200);

    let maintainer_put_settings_res = client
        .put(format!("http://{addr}/api/repositories/{repo_id}/settings"))
        .bearer_auth(&maintainer_jwt)
        .json(&json!({ "pipelineFilePath": "custom-pipeline.yml" }))
        .send()
        .await
        .unwrap();
    assert_eq!(maintainer_put_settings_res.status(), 200);
    let maintainer_put_settings_body: serde_json::Value =
        maintainer_put_settings_res.json().await.unwrap();
    assert_eq!(
        maintainer_put_settings_body["pipelineFilePath"],
        "custom-pipeline.yml"
    );

    client
        .post(format!("http://{addr}/api/admin/users"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "username": "extra", "email": "extra@example.com", "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let maintainer_add_collaborator_status = client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/collaborators"
        ))
        .bearer_auth(&maintainer_jwt)
        .json(&json!({ "username": "extra", "role": "reader" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(maintainer_add_collaborator_status, 204);

    let maintainer_set_role_status = client
        .patch(format!(
            "http://{addr}/api/repositories/{repo_id}/collaborators/reader"
        ))
        .bearer_auth(&maintainer_jwt)
        .json(&json!({ "role": "contributor" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(maintainer_set_role_status, 204);

    let final_collaborators: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_id}/collaborators"
        ))
        .bearer_auth(&maintainer_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let final_collaborators = final_collaborators.as_array().unwrap();
    // The owner is never itself a collaborator row, so this is the original three plus `extra`.
    assert_eq!(
        final_collaborators.len(),
        4,
        "expected reader, contributor, maintainer, and extra to all be listed, got: {final_collaborators:#?}"
    );
    let readers_row = final_collaborators
        .iter()
        .find(|c| c["username"] == "reader")
        .unwrap();
    assert_eq!(
        readers_row["role"], "contributor",
        "reader's role must now report as contributor after the maintainer's role change"
    );
    let extras_row = final_collaborators
        .iter()
        .find(|c| c["username"] == "extra")
        .unwrap();
    assert_eq!(
        extras_row["role"], "reader",
        "extra must appear with the role they were added at"
    );
}
