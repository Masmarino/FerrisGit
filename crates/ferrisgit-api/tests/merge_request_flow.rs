use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use serde_json::json;
use sqlx::PgPool;
use std::process::Command;

#[sqlx::test]
async fn creating_diffing_commenting_on_and_merging_a_real_branch_through_the_api(pool: PgPool) {
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

    let login_res: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "admin", "password": "adminpassword123" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let jwt = login_res["token"].as_str().unwrap();

    let token_res: serde_json::Value = client
        .post(format!("http://{addr}/api/tokens"))
        .bearer_auth(jwt)
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
        .bearer_auth(jwt)
        .json(&json!({ "name": "hello", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(repo_res["name"], "hello");
    let repo_id = repo_res["id"].as_str().unwrap().to_string();

    client
        .put(format!("http://{addr}/api/repositories/{repo_id}/settings"))
        .bearer_auth(jwt)
        .json(&json!({ "ciEnabled": true }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let clone_parent = tempfile::tempdir().unwrap();
    let clone_url = format!("http://admin:{plain_token}@{addr}/admin/hello.git");
    let repo_path = clone_parent.path().join("repo");
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

    std::fs::write(repo_path.join(".ferrisgit-ci.yml"), "stages: [build]\njobs:\n  compile:\n    stage: build\n    image: alpine\n    script: [\"echo hi\"]\n").unwrap();
    std::fs::write(repo_path.join("README.md"), "line one\n").unwrap();
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
                "user.email=t@t.com".to_string(),
                "-c".to_string(),
                "user.name=t".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "root".to_string()
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

    let root_sha_output = run_git_output(
        vec!["rev-parse".to_string(), "HEAD".to_string()],
        repo_path.clone(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(root_sha_output.status.success());
    let root_sha = String::from_utf8(root_sha_output.stdout)
        .unwrap()
        .trim()
        .to_string();

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
    std::fs::write(repo_path.join("README.md"), "line one\nline two\n").unwrap();
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
                "user.email=t@t.com".to_string(),
                "-c".to_string(),
                "user.name=t".to_string(),
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

    assert!(
        run_git(
            vec!["checkout".to_string(), "-q".to_string(), root_sha.clone()],
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
                "docs".to_string()
            ],
            repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    std::fs::write(repo_path.join("CONTRIBUTING.md"), "please contribute\n").unwrap();
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
                "user.email=t@t.com".to_string(),
                "-c".to_string(),
                "user.name=t".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "docs work".to_string()
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
                "HEAD:docs".to_string()
            ],
            repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    let branches: serde_json::Value = client
        .get(format!("http://{addr}/api/repositories/{repo_id}/branches"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let branch_names: Vec<&str> = branches
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["name"].as_str().unwrap())
        .collect();
    assert!(branch_names.contains(&"main"));
    assert!(branch_names.contains(&"feature"));
    assert!(branch_names.contains(&"docs"));
    let main_branch = branches
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["name"] == "main")
        .unwrap();
    assert_eq!(main_branch["isDefault"], true);
    let feature_branch = branches
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["name"] == "feature")
        .unwrap();
    assert_eq!(feature_branch["isDefault"], false);

    let mr_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/merge-requests"))
        .bearer_auth(jwt)
        .json(&json!({ "sourceBranch": "feature", "targetBranch": "main", "title": "Add line two", "description": "" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mr_id = mr_res["id"].as_str().unwrap().to_string();
    assert_eq!(mr_res["status"], "open");
    assert_eq!(mr_res["sourceBranch"], "feature");
    assert_eq!(mr_res["targetBranch"], "main");

    let mr2_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/merge-requests"))
        .bearer_auth(jwt)
        .json(&json!({ "sourceBranch": "docs", "targetBranch": "main", "title": "Add contributing guide", "description": "" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mr2_id = mr2_res["id"].as_str().unwrap().to_string();
    assert_eq!(mr2_res["status"], "open");

    let mr_list: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_id}/merge-requests"
        ))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let listed_ids: Vec<&str> = mr_list
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert!(listed_ids.contains(&mr_id.as_str()));
    assert!(listed_ids.contains(&mr2_id.as_str()));
    let listed_feature_mr = mr_list
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == mr_id)
        .unwrap();
    assert_eq!(listed_feature_mr["sourceBranch"], "feature");
    let listed_docs_mr = mr_list
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == mr2_id)
        .unwrap();
    assert_eq!(listed_docs_mr["sourceBranch"], "docs");

    let mr_detail: serde_json::Value = client
        .get(format!("http://{addr}/api/merge-requests/{mr_id}"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(mr_detail["id"], mr_id.as_str());
    assert_eq!(mr_detail["sourceBranch"], "feature");
    assert_eq!(mr_detail["targetBranch"], "main");
    assert_eq!(mr_detail["title"], "Add line two");
    assert_eq!(mr_detail["status"], "open");
    assert!(mr_detail["mergeCommitSha"].is_null());
    assert!(mr_detail["closedAt"].is_null());

    let diff: serde_json::Value = client
        .get(format!("http://{addr}/api/merge-requests/{mr_id}/diff"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let diff_array = diff.as_array().unwrap();
    assert_eq!(
        diff_array.len(),
        1,
        "expected only README.md to differ between feature and main, got: {diff:#}"
    );
    let readme_diff = diff_array
        .iter()
        .find(|d| d["path"] == "README.md")
        .unwrap();
    assert_eq!(readme_diff["change"], "modified");

    let root_comment: serde_json::Value = client
        .post(format!("http://{addr}/api/merge-requests/{mr_id}/comments"))
        .bearer_auth(jwt)
        .json(&json!({ "body": "why do we need this?", "filePath": "README.md", "lineNumber": 2, "side": "new" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(root_comment["filePath"], "README.md");
    assert_eq!(root_comment["lineNumber"], 2);
    assert_eq!(root_comment["side"], "new");
    assert_eq!(root_comment["outdated"], false);
    let root_comment_id = root_comment["id"].as_str().unwrap();

    let reply_res = client
        .post(format!("http://{addr}/api/merge-requests/{mr_id}/comments"))
        .bearer_auth(jwt)
        .json(
            &json!({ "body": "because it explains the second step", "replyToId": root_comment_id }),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(reply_res.status(), 200);
    let reply_body: serde_json::Value = reply_res.json().await.unwrap();
    assert_eq!(reply_body["replyToId"], root_comment_id);
    assert_eq!(
        reply_body["filePath"], "README.md",
        "a reply must inherit its root's anchor"
    );

    let stale_anchor_status = client
        .post(format!("http://{addr}/api/merge-requests/{mr_id}/comments"))
        .bearer_auth(jwt)
        .json(
            &json!({ "body": "bogus", "filePath": "README.md", "lineNumber": 999, "side": "new" }),
        )
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        stale_anchor_status, 400,
        "a line that doesn't exist in the current diff must be rejected"
    );

    assert!(
        run_git(
            vec![
                "checkout".to_string(),
                "-q".to_string(),
                "feature".to_string()
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
                "pull".to_string(),
                "-q".to_string(),
                "origin".to_string(),
                "feature".to_string()
            ],
            repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    std::fs::write(repo_path.join("README.md"), "line one\n").unwrap();
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
                "user.email=t@t.com".to_string(),
                "-c".to_string(),
                "user.name=t".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "revert line two".to_string()
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

    let comments_after_push: serde_json::Value = client
        .get(format!("http://{addr}/api/merge-requests/{mr_id}/comments"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let comments_after_push = comments_after_push.as_array().unwrap();
    let root_after_push = comments_after_push
        .iter()
        .find(|c| c["id"] == root_comment_id)
        .unwrap();
    assert_eq!(
        root_after_push["outdated"], true,
        "the commented line no longer exists in the diff after the revert, so it must now report outdated"
    );

    let resolve_reply_status = client
        .post(format!(
            "http://{addr}/api/merge-requests/{mr_id}/comments/{}/resolve",
            reply_body["id"].as_str().unwrap()
        ))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        resolve_reply_status, 400,
        "only a thread's root comment can be resolved, not a reply"
    );

    let resolve_status = client
        .post(format!(
            "http://{addr}/api/merge-requests/{mr_id}/comments/{root_comment_id}/resolve"
        ))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(resolve_status, 204);

    let comments_after_resolve: serde_json::Value = client
        .get(format!("http://{addr}/api/merge-requests/{mr_id}/comments"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let root_after_resolve = comments_after_resolve
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == root_comment_id)
        .unwrap();
    assert_eq!(root_after_resolve["resolved"], true);
    assert_eq!(
        root_after_resolve["outdated"], true,
        "resolved and outdated are independent — resolving must not clear the outdated flag"
    );

    let unresolve_status = client
        .post(format!(
            "http://{addr}/api/merge-requests/{mr_id}/comments/{root_comment_id}/unresolve"
        ))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(unresolve_status, 204);

    let comments_after_unresolve: serde_json::Value = client
        .get(format!("http://{addr}/api/merge-requests/{mr_id}/comments"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let root_after_unresolve = comments_after_unresolve
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == root_comment_id)
        .unwrap();
    assert_eq!(root_after_unresolve["resolved"], false);

    assert!(
        run_git(
            vec![
                "checkout".to_string(),
                "-q".to_string(),
                "feature".to_string()
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
                "pull".to_string(),
                "-q".to_string(),
                "origin".to_string(),
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
        "line one\nline two\nline three\n",
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
                "user.email=t@t.com".to_string(),
                "-c".to_string(),
                "user.name=t".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "reintroduce two lines for the suggestion scenario".to_string()
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

    let suggestion_res: serde_json::Value = client
        .post(format!("http://{addr}/api/merge-requests/{mr_id}/comments"))
        .bearer_auth(jwt)
        .json(&json!({ "body": "swap this block", "filePath": "README.md", "lineNumber": 2, "endLine": 3, "side": "new", "suggestedContent": "line TWO\nline THREE\n" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(suggestion_res["endLine"], 3);
    assert_eq!(suggestion_res["suggestedContent"], "line TWO\nline THREE\n");
    assert_eq!(suggestion_res["appliedAt"], serde_json::Value::Null);
    let suggestion_comment_id = suggestion_res["id"].as_str().unwrap();

    let apply_res = client
        .post(format!("http://{addr}/api/merge-requests/{mr_id}/comments/{suggestion_comment_id}/apply-suggestion"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(apply_res.status(), 200);
    let applied_comment: serde_json::Value = apply_res.json().await.unwrap();
    assert_ne!(applied_comment["appliedAt"], serde_json::Value::Null);
    let applied_commit_sha = applied_comment["appliedCommitSha"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!applied_commit_sha.is_empty());

    let reapply_status = client
        .post(format!("http://{addr}/api/merge-requests/{mr_id}/comments/{suggestion_comment_id}/apply-suggestion"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        reapply_status, 400,
        "an already-applied suggestion must not be re-appliable"
    );

    let verify_feature_parent = tempfile::tempdir().unwrap();
    let verify_feature_url = format!("http://admin:{plain_token}@{addr}/admin/hello.git");
    assert!(
        run_git(
            vec![
                "clone".to_string(),
                "-q".to_string(),
                "-b".to_string(),
                "feature".to_string(),
                verify_feature_url,
                "verify-feature".to_string()
            ],
            verify_feature_parent.path().to_path_buf()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    let feature_readme = std::fs::read_to_string(
        verify_feature_parent
            .path()
            .join("verify-feature")
            .join("README.md"),
    )
    .unwrap();
    assert_eq!(
        feature_readme, "line one\nline TWO\nline THREE\n",
        "the applied suggestion's commit must genuinely be feature's new tip"
    );

    let add_comment_res: serde_json::Value = client
        .post(format!("http://{addr}/api/merge-requests/{mr_id}/comments"))
        .bearer_auth(jwt)
        .json(&json!({ "body": "looks good" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(add_comment_res["body"], "looks good");
    let comments: serde_json::Value = client
        .get(format!("http://{addr}/api/merge-requests/{mr_id}/comments"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let comments = comments.as_array().unwrap();
    assert_eq!(
        comments.len(),
        4,
        "expected the earlier root inline comment, its reply, the applied suggestion comment, and this general comment, got: {comments:#?}"
    );
    let looks_good_comment = comments
        .iter()
        .find(|c| c["id"] == add_comment_res["id"])
        .unwrap();
    assert_eq!(looks_good_comment["body"], "looks good");

    let merge_res: serde_json::Value = client
        .post(format!("http://{addr}/api/merge-requests/{mr_id}/merge"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(merge_res["outcome"], "merged");
    assert_eq!(merge_res["status"], "merged");
    let merge_commit_sha = merge_res["mergeCommitSha"].as_str().unwrap().to_string();

    let verify_clone_parent = tempfile::tempdir().unwrap();
    let verify_clone_url = format!("http://admin:{plain_token}@{addr}/admin/hello.git");
    assert!(
        run_git(
            vec![
                "clone".to_string(),
                "-q".to_string(),
                "-b".to_string(),
                "main".to_string(),
                verify_clone_url,
                "verify".to_string()
            ],
            verify_clone_parent.path().to_path_buf()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    let readme_content =
        std::fs::read_to_string(verify_clone_parent.path().join("verify").join("README.md"))
            .unwrap();
    assert_eq!(
        readme_content, "line one\nline TWO\nline THREE\n",
        "feature's tip carries the applied suggestion's replacement content by the time it's merged into main"
    );

    let merged_detail: serde_json::Value = client
        .get(format!("http://{addr}/api/merge-requests/{mr_id}"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(merged_detail["status"], "merged");
    assert_eq!(merged_detail["mergeCommitSha"], merge_commit_sha.as_str());

    // The merge commit is written by git plumbing, not a smart-HTTP push, yet must still trigger a pipeline.
    let pipelines: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_id}/pipelines"
        ))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        pipelines
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["commitSha"] == merge_commit_sha),
        "merging must trigger a pipeline against the new merge commit, exactly like a real push does"
    );

    // A comment is outdated when its line keeps its number but the content changes. Once the MR is not open, comments always report outdated: false.

    let docs_comment: serde_json::Value = client
        .post(format!("http://{addr}/api/merge-requests/{mr2_id}/comments"))
        .bearer_auth(jwt)
        .json(&json!({ "body": "typo here?", "filePath": "CONTRIBUTING.md", "lineNumber": 1, "side": "new" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(docs_comment["outdated"], false);
    let docs_comment_id = docs_comment["id"].as_str().unwrap().to_string();

    assert!(
        run_git(
            vec!["checkout".to_string(), "-q".to_string(), "docs".to_string()],
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
                "pull".to_string(),
                "-q".to_string(),
                "origin".to_string(),
                "docs".to_string()
            ],
            repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    std::fs::write(
        repo_path.join("CONTRIBUTING.md"),
        "please contribute nicely\n",
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
                "user.email=t@t.com".to_string(),
                "-c".to_string(),
                "user.name=t".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "reword contributing guide".to_string()
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
                "HEAD:docs".to_string()
            ],
            repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    let comments_while_open: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/merge-requests/{mr2_id}/comments"
        ))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let docs_comment_while_open = comments_while_open
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == docs_comment_id)
        .unwrap();
    assert_eq!(
        docs_comment_while_open["outdated"], true,
        "the anchored line's content changed in place (same line number, different text) — this must be detected as outdated too, not just a removed line"
    );

    let close_status = client
        .post(format!("http://{addr}/api/merge-requests/{mr2_id}/close"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(close_status, reqwest::StatusCode::NO_CONTENT);

    let closed_detail: serde_json::Value = client
        .get(format!("http://{addr}/api/merge-requests/{mr2_id}"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(closed_detail["status"], "closed");
    assert!(
        closed_detail["closedAt"].is_string(),
        "expected closedAt to be set after closing, got: {closed_detail:#}"
    );
    assert!(
        closed_detail["mergeCommitSha"].is_null(),
        "a closed (not merged) request must never get a merge commit sha"
    );

    let comments_after_close: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/merge-requests/{mr2_id}/comments"
        ))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let docs_comment_after_close = comments_after_close
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == docs_comment_id)
        .unwrap();
    assert_eq!(
        docs_comment_after_close["outdated"], false,
        "once a merge request is no longer open there is no live diff to compare against, so every comment must report outdated: false, even though this one demonstrably would be if the MR were still open"
    );

    assert!(
        run_git(
            vec!["checkout".to_string(), "-q".to_string(), "main".to_string()],
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
                "pull".to_string(),
                "-q".to_string(),
                "origin".to_string(),
                "main".to_string()
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
                "extra".to_string()
            ],
            repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    std::fs::write(repo_path.join("EXTRA.md"), "first version\n").unwrap();
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
                "user.email=t@t.com".to_string(),
                "-c".to_string(),
                "user.name=t".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "extra work".to_string()
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
                "HEAD:extra".to_string()
            ],
            repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    let mr3_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/merge-requests"))
        .bearer_auth(jwt)
        .json(&json!({ "sourceBranch": "extra", "targetBranch": "main", "title": "Add extra file", "description": "" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mr3_id = mr3_res["id"].as_str().unwrap().to_string();

    client
        .put(format!("http://{addr}/api/repositories/{repo_id}/settings"))
        .bearer_auth(jwt)
        .json(&json!({ "requiredApprovals": 1 }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let blocked_res = client
        .post(format!("http://{addr}/api/merge-requests/{mr3_id}/merge"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(blocked_res.status(), 400);
    let blocked_body: serde_json::Value = blocked_res.json().await.unwrap();
    assert!(
        blocked_body["error"].as_str().unwrap().contains("0/1"),
        "expected a 0/1 approvals message, got: {blocked_body:#}"
    );

    // An MR author cannot approve their own MR, so the gate needs an independent reviewer.
    client
        .post(format!("http://{addr}/api/admin/users"))
        .bearer_auth(jwt)
        .json(&json!({ "username": "reviewer", "email": "reviewer@example.com", "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/collaborators"
        ))
        .bearer_auth(jwt)
        .json(&json!({ "username": "reviewer", "role": "contributor" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let reviewer_login: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "reviewer", "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let reviewer_jwt = reviewer_login["token"].as_str().unwrap();

    client
        .post(format!("http://{addr}/api/merge-requests/{mr3_id}/reviews"))
        .bearer_auth(reviewer_jwt)
        .json(&json!({ "decision": "approved" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let reviews_after_approval: serde_json::Value = client
        .get(format!("http://{addr}/api/merge-requests/{mr3_id}/reviews"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(reviews_after_approval["liveApprovalCount"], 1);
    assert_eq!(reviews_after_approval["blocked"], false);

    std::fs::write(repo_path.join("EXTRA.md"), "second version\n").unwrap();
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
                "user.email=t@t.com".to_string(),
                "-c".to_string(),
                "user.name=t".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "extra work v2".to_string()
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
                "HEAD:extra".to_string()
            ],
            repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    let reviews_after_new_commit: serde_json::Value = client
        .get(format!("http://{addr}/api/merge-requests/{mr3_id}/reviews"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        reviews_after_new_commit["reviews"][0]["stale"], true,
        "the earlier approval must now be stale against the new tip commit"
    );
    assert_eq!(reviews_after_new_commit["blocked"], true);

    let blocked_again_status = client
        .post(format!("http://{addr}/api/merge-requests/{mr3_id}/merge"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        blocked_again_status, 400,
        "a stale approval must not unblock a merge against a new commit"
    );

    client
        .post(format!("http://{addr}/api/merge-requests/{mr3_id}/reviews"))
        .bearer_auth(reviewer_jwt)
        .json(&json!({ "decision": "approved" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let final_merge_res: serde_json::Value = client
        .post(format!("http://{addr}/api/merge-requests/{mr3_id}/merge"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(final_merge_res["outcome"], "merged");

    // With the approvals gate off, a live "changes requested" review must not block: the summary's `blocked` flag and the merge veto must agree.
    client
        .put(format!("http://{addr}/api/repositories/{repo_id}/settings"))
        .bearer_auth(jwt)
        .json(&json!({ "requiredApprovals": 0 }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    assert!(
        run_git(
            vec!["checkout".to_string(), "-q".to_string(), "main".to_string()],
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
                "pull".to_string(),
                "-q".to_string(),
                "origin".to_string(),
                "main".to_string()
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
                "extra2".to_string()
            ],
            repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    std::fs::write(repo_path.join("EXTRA2.md"), "first version\n").unwrap();
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
                "user.email=t@t.com".to_string(),
                "-c".to_string(),
                "user.name=t".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "extra2 work".to_string()
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
                "HEAD:extra2".to_string()
            ],
            repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    let mr4_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/merge-requests"))
        .bearer_auth(jwt)
        .json(&json!({ "sourceBranch": "extra2", "targetBranch": "main", "title": "Add extra2 file", "description": "" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mr4_id = mr4_res["id"].as_str().unwrap().to_string();

    client
        .post(format!("http://{addr}/api/merge-requests/{mr4_id}/reviews"))
        .bearer_auth(jwt)
        .json(&json!({ "decision": "changes_requested" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let mr4_reviews: serde_json::Value = client
        .get(format!("http://{addr}/api/merge-requests/{mr4_id}/reviews"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(mr4_reviews["reviews"][0]["decision"], "changes_requested");
    assert_eq!(mr4_reviews["reviews"][0]["stale"], false);
    assert_eq!(
        mr4_reviews["blocked"], false,
        "a live change-request must not block when the approvals gate is disabled (required_approvals == 0), got: {mr4_reviews:#}"
    );

    let mr4_merge_status = client
        .post(format!("http://{addr}/api/merge-requests/{mr4_id}/merge"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_ne!(
        mr4_merge_status, 400,
        "the merge use case's own gate is also disabled at required_approvals == 0, so this must not fail with the changes-requested 400"
    );
}

#[sqlx::test]
async fn updating_a_merge_request_assigns_a_milestone_sets_labels_and_the_list_can_be_filtered(
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

    let login_res: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "admin", "password": "adminpassword123" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let jwt = login_res["token"].as_str().unwrap();

    let token_res: serde_json::Value = client
        .post(format!("http://{addr}/api/tokens"))
        .bearer_auth(jwt)
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
        .bearer_auth(jwt)
        .json(&json!({ "name": "hello", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let repo_id = repo_res["id"].as_str().unwrap().to_string();

    // A milestone scoped to another repository must never be assignable here.
    let other_repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(jwt)
        .json(&json!({ "name": "other", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let other_repo_id = other_repo_res["id"].as_str().unwrap().to_string();

    let clone_parent = tempfile::tempdir().unwrap();
    let clone_url = format!("http://admin:{plain_token}@{addr}/admin/hello.git");
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

    std::fs::write(repo_path.join("README.md"), "line one\n").unwrap();
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
                "user.email=t@t.com".to_string(),
                "-c".to_string(),
                "user.name=t".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "root".to_string()
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

    for branch in ["feature", "unrelated"] {
        assert!(
            run_git(
                vec![
                    "checkout".to_string(),
                    "-q".to_string(),
                    "-b".to_string(),
                    branch.to_string(),
                    "main".to_string()
                ],
                repo_path.clone()
            )
            .await
            .unwrap()
            .unwrap()
            .success()
        );
        std::fs::write(repo_path.join(format!("{branch}.md")), "content\n").unwrap();
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
                    "user.email=t@t.com".to_string(),
                    "-c".to_string(),
                    "user.name=t".to_string(),
                    "commit".to_string(),
                    "-q".to_string(),
                    "-m".to_string(),
                    branch.to_string()
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
                    format!("HEAD:{branch}")
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
                vec!["checkout".to_string(), "-q".to_string(), "main".to_string()],
                repo_path.clone()
            )
            .await
            .unwrap()
            .unwrap()
            .success()
        );
    }

    let mr_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/merge-requests"))
        .bearer_auth(jwt)
        .json(&json!({ "sourceBranch": "feature", "targetBranch": "main", "title": "Original title", "description": "Original description" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mr_id = mr_res["id"].as_str().unwrap().to_string();
    assert_eq!(mr_res["milestoneId"], serde_json::Value::Null);
    assert_eq!(mr_res["labels"].as_array().unwrap().len(), 0);

    let mr2_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/merge-requests"))
        .bearer_auth(jwt)
        .json(&json!({ "sourceBranch": "unrelated", "targetBranch": "main", "title": "Unrelated MR", "description": "" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mr2_id = mr2_res["id"].as_str().unwrap().to_string();

    let update_res: serde_json::Value = client
        .patch(format!("http://{addr}/api/merge-requests/{mr_id}"))
        .bearer_auth(jwt)
        .json(
            &json!({ "title": "New title", "description": "New description", "milestoneId": null }),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(update_res["title"], "New title");
    assert_eq!(update_res["description"], "New description");
    assert_eq!(update_res["milestoneId"], serde_json::Value::Null);

    // Full-replace PATCH: an omitted `milestoneId` must be rejected, not silently clear the milestone (serde maps a missing Option to None, hence Option<Option<Uuid>>).
    let omitted_milestone_res = client
        .patch(format!("http://{addr}/api/merge-requests/{mr_id}"))
        .bearer_auth(jwt)
        .json(&json!({ "title": "New title", "description": "New description" }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        omitted_milestone_res.status(),
        400,
        "an omitted milestoneId must be rejected, never treated as 'clear the milestone'"
    );
    let omitted_milestone_body: serde_json::Value = omitted_milestone_res.json().await.unwrap();
    assert!(
        omitted_milestone_body["error"]
            .as_str()
            .unwrap()
            .contains("milestoneId is required"),
        "the rejection must say what is missing, got: {omitted_milestone_body:#}"
    );

    let milestone_res: serde_json::Value = client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/milestones"
        ))
        .bearer_auth(jwt)
        .json(&json!({ "title": "v1", "description": "", "dueDate": null }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let milestone_id = milestone_res["id"].as_str().unwrap().to_string();

    let other_milestone_res: serde_json::Value = client
        .post(format!(
            "http://{addr}/api/repositories/{other_repo_id}/milestones"
        ))
        .bearer_auth(jwt)
        .json(&json!({ "title": "out-of-scope", "description": "", "dueDate": null }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let other_milestone_id = other_milestone_res["id"].as_str().unwrap().to_string();

    let out_of_scope_status = client
        .patch(format!("http://{addr}/api/merge-requests/{mr_id}"))
        .bearer_auth(jwt)
        .json(&json!({ "title": "New title", "description": "New description", "milestoneId": other_milestone_id }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        out_of_scope_status, 400,
        "a milestone scoped to a different repository must be rejected"
    );

    let with_milestone_res: serde_json::Value = client
        .patch(format!("http://{addr}/api/merge-requests/{mr_id}"))
        .bearer_auth(jwt)
        .json(&json!({ "title": "New title", "description": "New description", "milestoneId": milestone_id }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(with_milestone_res["milestoneId"], milestone_id.as_str());

    // Omitting `milestoneId` must be refused and leave the milestone in place. An explicit null clears it.
    let omitted_after_set_status = client
        .patch(format!("http://{addr}/api/merge-requests/{mr_id}"))
        .bearer_auth(jwt)
        .json(&json!({ "title": "New title", "description": "New description" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(omitted_after_set_status, 400);
    let still_has_milestone: serde_json::Value = client
        .get(format!("http://{addr}/api/merge-requests/{mr_id}"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        still_has_milestone["milestoneId"],
        milestone_id.as_str(),
        "a rejected update must not have cleared the milestone"
    );

    let label_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/labels"))
        .bearer_auth(jwt)
        .json(&json!({ "name": "Bug", "color": "#dc2626" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let label_id = label_res["id"].as_str().unwrap().to_string();

    let set_labels_res: serde_json::Value = client
        .put(format!("http://{addr}/api/merge-requests/{mr_id}/labels"))
        .bearer_auth(jwt)
        .json(&json!({ "labelIds": [label_id] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let set_labels = set_labels_res.as_array().unwrap();
    assert_eq!(set_labels.len(), 1);
    assert_eq!(set_labels[0]["id"], label_id.as_str());

    let detail_after_res: serde_json::Value = client
        .get(format!("http://{addr}/api/merge-requests/{mr_id}"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(detail_after_res["milestoneId"], milestone_id.as_str());
    assert_eq!(detail_after_res["labels"].as_array().unwrap().len(), 1);
    assert_eq!(detail_after_res["labels"][0]["id"], label_id.as_str());

    let filtered_by_label: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_id}/merge-requests?labelIds={label_id}"
        ))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let filtered_by_label_ids: Vec<&str> = filtered_by_label
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(filtered_by_label_ids, vec![mr_id.as_str()]);

    let filtered_by_milestone: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_id}/merge-requests?milestoneId={milestone_id}"
        ))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let filtered_by_milestone_ids: Vec<&str> = filtered_by_milestone
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(filtered_by_milestone_ids, vec![mr_id.as_str()]);

    let unfiltered: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_id}/merge-requests"
        ))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let unfiltered_ids: Vec<&str> = unfiltered
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert!(unfiltered_ids.contains(&mr_id.as_str()));
    assert!(
        unfiltered_ids.contains(&mr2_id.as_str()),
        "the unrelated MR must still appear when not filtering"
    );
}
