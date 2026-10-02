mod common;

use common::http::{delete, get_json, login, post, post_anon, post_json, post_ok, put};
use common::poll_until;

use serde_json::json;
use sqlx::PgPool;
use std::process::Command;
use std::time::Duration;

/// Kills the child on drop, panics included, so a failed assertion doesn't orphan the runner.
struct KillOnDrop(std::process::Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[sqlx::test]
async fn a_pushed_pipeline_file_runs_to_success_via_a_real_runner(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;

    let client = reqwest::Client::new();

    let jwt = login(&client, addr, "admin", "adminpassword123").await;

    let token_res: serde_json::Value =
        post_json(&client, addr, &jwt, "/tokens", &json!({ "name": "ci" })).await;
    let plain_token = token_res["token"].as_str().unwrap();

    let repo_res: serde_json::Value = post_json(
        &client,
        addr,
        &jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    assert_eq!(repo_res["name"], "hello");
    let repo_id = repo_res["id"].as_str().unwrap().to_string();

    let runner_res: serde_json::Value = post_json(
        &client,
        addr,
        &jwt,
        "/admin/runners",
        &json!({ "name": "test-runner", "tags": [] }),
    )
    .await;
    let runner_token = runner_res["token"].as_str().unwrap().to_string();

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

    // A masked variable printed by the job: the runner must redact it from the streamed logs.
    post_ok(
        &client,
        addr,
        &jwt,
        &format!("/repositories/{repo_id}/ci-variables"),
        &json!({ "key": "SECRET_VALUE", "value": "super-secret-db-password", "masked": true }),
    )
    .await;

    std::fs::write(
        repo_path.join(".ferrisgit-ci.yml"),
        "stages: [build]\njobs:\n  hello:\n    stage: build\n    image: alpine:3.20\n    script:\n      - echo hello-from-ci\n      - echo $SECRET_VALUE\n",
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
                "user.email=ci@example.com".to_string(),
                "-c".to_string(),
                "user.name=ci".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "add pipeline".to_string()
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

    let pipelines = poll_until(
        || {
            let client = client.clone();
            let jwt = jwt.to_string();
            let repo_id = repo_id.clone();
            async move {
                let res: serde_json::Value = get_json(
                    &client,
                    addr,
                    &jwt,
                    &format!("/repositories/{repo_id}/pipelines"),
                )
                .await;
                if res.as_array().is_some_and(|a| !a.is_empty()) {
                    Some(res)
                } else {
                    None
                }
            }
        },
        Duration::from_secs(10),
        "a pipeline to be created after the push",
    )
    .await;
    let pipeline_id = pipelines[0]["id"].as_str().unwrap().to_string();

    let runner_workdir = tempfile::tempdir().unwrap();
    // CARGO_BIN_EXE_ferrisgit-runner isn't set for a dev-dependency's binaries, so escargot builds the real runner and returns its path.
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
            let jwt = jwt.to_string();
            let pipeline_id = pipeline_id.clone();
            async move {
                let res: serde_json::Value =
                    get_json(&client, addr, &jwt, &format!("/pipelines/{pipeline_id}")).await;
                let status = res["status"].as_str().unwrap_or("");
                if matches!(status, "success" | "failed" | "canceled") { Some(res) } else { None }
            }
        },
        Duration::from_secs(120),
        "the pipeline to reach a terminal status (this requires a running Docker daemon to pull alpine:3.20 and run the job)",
    )
    .await;

    let _ = runner_process.0.kill();
    let _ = runner_process.0.wait();

    assert_eq!(
        final_pipeline["status"], "success",
        "pipeline did not succeed: {final_pipeline:#}"
    );
    let jobs = final_pipeline["jobs"].as_array().unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0]["status"], "success");
    assert!(
        jobs[0]["logs"].as_str().unwrap().contains("hello-from-ci"),
        "expected job logs to contain the script's echo output, got: {}",
        jobs[0]["logs"]
    );
    assert!(
        !jobs[0]["logs"]
            .as_str()
            .unwrap()
            .contains("super-secret-db-password"),
        "a masked CI variable's value must be redacted from the job logs, got: {}",
        jobs[0]["logs"]
    );
    assert!(
        jobs[0]["logs"].as_str().unwrap().contains("***"),
        "the masked value's line must still show the redaction marker, got: {}",
        jobs[0]["logs"]
    );

    let unconfigured_attempt = post_anon(
        &client,
        addr,
        "/runner/register",
        &json!({ "registration_token": "whatever", "name": "self-registered", "tags": [] }),
    )
    .await;
    assert_eq!(
        unconfigured_attempt.status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "self-registration must be disabled until an admin sets runner_registration_token"
    );

    put(
        &client,
        addr,
        &jwt,
        "/admin/settings",
        &json!({ "runnerRegistrationToken": "shared-secret-123" }),
    )
    .await
    .error_for_status()
    .unwrap();

    let wrong_token_attempt = post_anon(
        &client,
        addr,
        "/runner/register",
        &json!({ "registration_token": "not-the-secret", "name": "self-registered", "tags": [] }),
    )
    .await;
    assert_eq!(
        wrong_token_attempt.status(),
        reqwest::StatusCode::UNAUTHORIZED
    );

    let correct_token_res: serde_json::Value = post_anon(&client, addr, "/runner/register", &json!({ "registration_token": "shared-secret-123", "name": "self-registered", "tags": [] })).await
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        correct_token_res["token"]
            .as_str()
            .unwrap()
            .starts_with("fgr_")
    );

    // A runner token is instance-wide, yet a second valid runner can't write to a job it never claimed.
    let other_runner_token = correct_token_res["token"].as_str().unwrap();
    let job_id = jobs[0]["id"].as_str().unwrap();

    let foreign_log_attempt = post(
        &client,
        addr,
        other_runner_token,
        &format!("/runner/jobs/{job_id}/logs"),
        &json!({ "chunk": "injected-by-a-foreign-runner\n" }),
    )
    .await;
    assert_eq!(
        foreign_log_attempt.status(),
        reqwest::StatusCode::NOT_FOUND,
        "a runner must not append logs to a job claimed by a different runner"
    );

    let foreign_result_attempt = post(
        &client,
        addr,
        other_runner_token,
        &format!("/runner/jobs/{job_id}/result"),
        &json!({ "status": "failed" }),
    )
    .await;
    assert_eq!(
        foreign_result_attempt.status(),
        reqwest::StatusCode::NOT_FOUND,
        "a runner must not report a result for a job claimed by a different runner"
    );

    let after: serde_json::Value =
        get_json(&client, addr, &jwt, &format!("/pipelines/{pipeline_id}")).await;
    assert_eq!(
        after["status"], "success",
        "the rejected foreign report must not have changed the pipeline"
    );
    let after_job = &after["jobs"].as_array().unwrap()[0];
    assert!(
        after["finishedAt"].is_string(),
        "a finished pipeline reports when it finished"
    );
    assert!(
        after_job["startedAt"].is_string() && after_job["finishedAt"].is_string(),
        "a job that ran reports both timestamps"
    );
    assert_eq!(after_job["status"], "success");
    assert!(
        !after_job["logs"]
            .as_str()
            .unwrap()
            .contains("injected-by-a-foreign-runner"),
        "the rejected foreign log write must not have reached the job's logs"
    );

    let other_runner_id = correct_token_res["id"].as_str().unwrap();
    let unauthenticated_revoke = client
        .delete(format!("http://{addr}/api/admin/runners/{other_runner_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        unauthenticated_revoke.status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "revoking a runner must be admin-only"
    );

    let revoke = delete(
        &client,
        addr,
        &jwt,
        &format!("/admin/runners/{other_runner_id}"),
    )
    .await;
    assert_eq!(revoke.status(), reqwest::StatusCode::NO_CONTENT);

    let after_revoke_claim = post(
        &client,
        addr,
        other_runner_token,
        "/runner/jobs/claim",
        &json!({ "tags": [] }),
    )
    .await;
    assert_eq!(
        after_revoke_claim.status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "a revoked runner's token must stop authenticating"
    );

    let remaining: serde_json::Value = get_json(&client, addr, &jwt, "/admin/runners").await;
    let remaining_ids: Vec<&str> = remaining
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert!(
        !remaining_ids.contains(&other_runner_id),
        "the revoked runner must be gone from the admin listing"
    );
}
