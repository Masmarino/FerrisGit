use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use serde_json::json;
use sqlx::PgPool;
use std::process::Command;
use std::time::Duration;

/// Kills the child process on drop, including on a panicking unwind, so a failed assertion never orphans the runner.
struct KillOnDrop(std::process::Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn poll_until<F, Fut>(mut check: F, timeout: Duration, description: &str) -> serde_json::Value
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Option<serde_json::Value>>,
{
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(value) = check().await {
            return value;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for: {description}"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[sqlx::test]
async fn notifications_are_created_for_merge_request_activity_and_pipeline_failure(pool: PgPool) {
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
    assert_eq!(repo_res["name"], "hello");
    assert_eq!(repo_res["owner"], "admin");
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

    let clone_parent = tempfile::tempdir().unwrap();
    let clone_url = format!("http://admin:{owner_plain_token}@{addr}/admin/hello.git");
    let repo_path = clone_parent.path().join("repo");
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
                "user.email=o@o.com".to_string(),
                "-c".to_string(),
                "user.name=owner".to_string(),
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

    client
        .post(format!("http://{addr}/api/admin/users"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "username": "contributor", "email": "contributor@example.com", "password": "password12345" }))
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

    let add_collaborator_status = client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/collaborators"
        ))
        .bearer_auth(owner_jwt)
        .json(&json!({ "username": "contributor", "role": "contributor" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(add_collaborator_status, 204);

    let contributor_notifications_after_add: serde_json::Value = client
        .get(format!("http://{addr}/api/notifications"))
        .bearer_auth(contributor_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let contributor_notifications_after_add =
        contributor_notifications_after_add.as_array().unwrap();
    assert_eq!(
        contributor_notifications_after_add.len(),
        1,
        "expected exactly one notification for the contributor after being added as a collaborator, got: {contributor_notifications_after_add:#?}"
    );
    let collaborator_added_notification = &contributor_notifications_after_add[0];
    assert_eq!(
        collaborator_added_notification["kind"],
        "collaborator_added"
    );
    assert_eq!(collaborator_added_notification["role"], "contributor");
    assert_eq!(collaborator_added_notification["repositoryOwner"], "admin");
    assert_eq!(collaborator_added_notification["repositoryName"], "hello");
    assert_eq!(collaborator_added_notification["actorUsername"], "admin");
    assert_eq!(collaborator_added_notification["read"], false);

    let owner_notifications_after_add: serde_json::Value = client
        .get(format!("http://{addr}/api/notifications"))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let owner_notifications_after_add = owner_notifications_after_add.as_array().unwrap();
    assert!(
        owner_notifications_after_add.is_empty(),
        "the owner performed the collaborator-add themselves and must not be notified of their own action, got: {owner_notifications_after_add:#?}"
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
                "user.email=o@o.com".to_string(),
                "-c".to_string(),
                "user.name=owner".to_string(),
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
        .bearer_auth(owner_jwt)
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

    let review_res = client
        .post(format!("http://{addr}/api/merge-requests/{mr_id}/reviews"))
        .bearer_auth(contributor_jwt)
        .json(&json!({ "decision": "approved" }))
        .send()
        .await
        .unwrap();
    assert_eq!(review_res.status(), 200);
    let review_body: serde_json::Value = review_res.json().await.unwrap();
    assert_eq!(review_body["decision"], "approved");
    assert_eq!(review_body["username"], "contributor");

    let notifications_after_approval: serde_json::Value = client
        .get(format!("http://{addr}/api/notifications"))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let notifications_after_approval = notifications_after_approval.as_array().unwrap();
    assert_eq!(
        notifications_after_approval.len(),
        1,
        "expected exactly one notification for the owner after the approval, got: {notifications_after_approval:#?}"
    );
    let approval_notification = &notifications_after_approval[0];
    assert_eq!(approval_notification["kind"], "merge_request_approved");
    assert_eq!(approval_notification["mergeRequestId"], mr_id.as_str());
    assert_eq!(approval_notification["repositoryOwner"], "admin");
    assert_eq!(approval_notification["repositoryName"], "hello");
    assert_eq!(approval_notification["actorUsername"], "contributor");
    assert_eq!(approval_notification["read"], false);
    let notification_id = approval_notification["id"].as_str().unwrap().to_string();

    let unread_count_after_approval: serde_json::Value = client
        .get(format!("http://{addr}/api/notifications/unread-count"))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(unread_count_after_approval["count"], 1);

    let mark_read_status = client
        .post(format!(
            "http://{addr}/api/notifications/{notification_id}/read"
        ))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(mark_read_status, 204);

    let unread_count_after_read: serde_json::Value = client
        .get(format!("http://{addr}/api/notifications/unread-count"))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        unread_count_after_read["count"], 0,
        "marking the notification read must drop the unread count to zero"
    );

    let notifications_after_read: serde_json::Value = client
        .get(format!("http://{addr}/api/notifications"))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let notifications_after_read = notifications_after_read.as_array().unwrap();
    let reread_notification = notifications_after_read
        .iter()
        .find(|n| n["id"] == notification_id)
        .unwrap();
    assert_eq!(
        reread_notification["read"], true,
        "the marked-read notification must now report read == true"
    );

    let runner_res: serde_json::Value = client
        .post(format!("http://{addr}/api/admin/runners"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": "test-runner", "tags": [] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let runner_token = runner_res["token"].as_str().unwrap().to_string();

    std::fs::write(
        repo_path.join(".ferrisgit-ci.yml"),
        "stages: [build]\njobs:\n  will-fail:\n    stage: build\n    image: alpine:3.20\n    script:\n      - exit 1\n",
    )
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
                "user.email=o@o.com".to_string(),
                "-c".to_string(),
                "user.name=owner".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "add failing pipeline".to_string()
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

    let failing_commit_output = tokio::task::spawn_blocking({
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
    assert!(failing_commit_output.status.success());
    let failing_commit_sha = String::from_utf8(failing_commit_output.stdout)
        .unwrap()
        .trim()
        .to_string();

    let pipelines = poll_until(
        || {
            let client = client.clone();
            let jwt = owner_jwt.to_string();
            let failing_commit_sha = failing_commit_sha.clone();
            let repo_id = repo_id.clone();
            async move {
                let res: serde_json::Value = client
                    .get(format!(
                        "http://{addr}/api/repositories/{repo_id}/pipelines"
                    ))
                    .bearer_auth(&jwt)
                    .send()
                    .await
                    .unwrap()
                    .json()
                    .await
                    .unwrap();
                if res
                    .as_array()
                    .is_some_and(|a| a.iter().any(|p| p["commitSha"] == failing_commit_sha))
                {
                    Some(res)
                } else {
                    None
                }
            }
        },
        Duration::from_secs(10),
        "a pipeline to be created for the failing commit after the push",
    )
    .await;
    let pipeline_id = pipelines
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["commitSha"] == failing_commit_sha)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let runner_workdir = tempfile::tempdir().unwrap();
    let runner_bin = escargot::CargoBuild::new()
        .manifest_path(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../ferrisgit-runner/Cargo.toml"
        ))
        .bin("ferrisgit-runner")
        .run()
        .expect("failed to build the ferrisgit-runner binary via escargot")
        .path()
        .to_path_buf();
    let runner_process = Command::new(runner_bin)
        .env("FERRISGIT_SERVER_URL", format!("http://{addr}"))
        .env("FERRISGIT_RUNNER_TOKEN", &runner_token)
        .env("FERRISGIT_RUNNER_TAGS", "")
        .env("FERRISGIT_POLL_INTERVAL_SECS", "1")
        .env("FERRISGIT_WORKDIR_ROOT", runner_workdir.path())
        .spawn()
        .expect("failed to spawn ferrisgit-runner — is the escargot-built binary path correct?");
    let mut runner_process = KillOnDrop(runner_process);

    let final_pipeline = poll_until(
        || {
            let client = client.clone();
            let jwt = owner_jwt.to_string();
            let pipeline_id = pipeline_id.clone();
            async move {
                let res: serde_json::Value =
                    client.get(format!("http://{addr}/api/pipelines/{pipeline_id}")).bearer_auth(&jwt).send().await.unwrap().json().await.unwrap();
                let status = res["status"].as_str().unwrap_or("");
                if matches!(status, "success" | "failed" | "canceled") { Some(res) } else { None }
            }
        },
        Duration::from_secs(120),
        "the pipeline to reach a terminal status (this requires a running Docker daemon to run the job)",
    )
    .await;

    let _ = runner_process.0.kill();
    let _ = runner_process.0.wait();

    assert_eq!(
        final_pipeline["status"], "failed",
        "pipeline did not reach the expected failed status: {final_pipeline:#}"
    );
    let jobs = final_pipeline["jobs"].as_array().unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0]["status"], "failed");

    // Polled: the pipeline row turns `failed` before the notification is written, so a concurrent read can see `failed` without the notification yet.
    let notifications_after_pipeline_failure = poll_until(
        || {
            let client = client.clone();
            let jwt = owner_jwt.to_string();
            async move {
                let res: serde_json::Value = client
                    .get(format!("http://{addr}/api/notifications"))
                    .bearer_auth(&jwt)
                    .send()
                    .await
                    .unwrap()
                    .json()
                    .await
                    .unwrap();
                res.as_array()
                    .is_some_and(|list| list.iter().any(|n| n["kind"] == "pipeline_failed"))
                    .then_some(res)
            }
        },
        Duration::from_secs(10),
        "the pipeline_failed notification to be created",
    )
    .await;
    let notifications_after_pipeline_failure =
        notifications_after_pipeline_failure.as_array().unwrap();
    let pipeline_failed_notification = notifications_after_pipeline_failure
        .iter()
        .find(|n| n["kind"] == "pipeline_failed")
        .unwrap_or_else(|| panic!("expected a pipeline_failed notification, got: {notifications_after_pipeline_failure:#?}"));
    assert_eq!(
        pipeline_failed_notification["commitSha"],
        failing_commit_sha.as_str()
    );
    assert_eq!(pipeline_failed_notification["repositoryOwner"], "admin");
    assert_eq!(pipeline_failed_notification["repositoryName"], "hello");
    assert_eq!(
        pipeline_failed_notification["pipelineId"],
        pipeline_id.as_str()
    );
    assert_eq!(pipeline_failed_notification["read"], false);

    assert_eq!(
        notifications_after_pipeline_failure.len(),
        2,
        "expected exactly the approval and pipeline-failure notifications, got: {notifications_after_pipeline_failure:#?}"
    );
    let still_present_approval_notification = notifications_after_pipeline_failure
        .iter()
        .find(|n| n["id"] == notification_id)
        .unwrap();
    assert_eq!(
        still_present_approval_notification["kind"],
        "merge_request_approved"
    );
    assert_eq!(
        still_present_approval_notification["read"], true,
        "the earlier mark-read must be unaffected by the later pipeline-failure notification"
    );
}
