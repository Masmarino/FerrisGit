// Pipelines pushed through git, driven over HTTP: an invalid pipeline file is visible to the pusher, a job only starts
// once the stages before it succeeded, and a failure ends the pipeline instead of leaving it waiting. The "runner" is
// the test itself calling the runner endpoints, so no Docker is needed.

mod common;

use common::http::{post_anon, post_json};

use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;

const TWO_STAGES: &str = "stages: [build, test]
jobs:
  compile:
    stage: build
    image: alpine:3.20
    script: [\"true\"]
  unit:
    stage: test
    image: alpine:3.20
    script: [\"true\"]
";

const THREE_STAGES_TWO_IN_THE_FIRST: &str = "stages: [build, test, deploy]
jobs:
  compile:
    stage: build
    image: alpine:3.20
    script: [\"true\"]
  lint:
    stage: build
    image: alpine:3.20
    script: [\"true\"]
  unit:
    stage: test
    image: alpine:3.20
    script: [\"true\"]
  ship:
    stage: deploy
    image: alpine:3.20
    script: [\"true\"]
";

struct Harness {
    addr: SocketAddr,
    client: reqwest::Client,
    jwt: String,
    plain_token: String,
    runner_token: String,
    repository_id: String,
    work_dir: tempfile::TempDir,
    pushes: u32,
}

impl Harness {
    async fn start(pool: PgPool) -> Self {
        let addr = common::spawn_app(pool).await.addr;

        let client = reqwest::Client::new();
        let login: Value = post_anon(
            &client,
            addr,
            "/auth/login",
            &json!({ "username": "admin", "password": "adminpassword123" }),
        )
        .await
        .json()
        .await
        .unwrap();
        let jwt = login["token"].as_str().unwrap().to_string();
        let token: Value =
            post_json(&client, addr, &jwt, "/tokens", &json!({ "name": "ci" })).await;
        let plain_token = token["token"].as_str().unwrap().to_string();
        let repo: Value = post_json(
            &client,
            addr,
            &jwt,
            "/repositories",
            &json!({ "name": "hello", "visibility": "private" }),
        )
        .await;
        let repository_id = repo["id"].as_str().unwrap().to_string();
        let runner: Value = post_json(
            &client,
            addr,
            &jwt,
            "/admin/runners",
            &json!({ "name": "test-runner", "tags": [] }),
        )
        .await;
        let runner_token = runner["token"].as_str().unwrap().to_string();

        let harness = Self {
            addr,
            client,
            jwt,
            plain_token,
            runner_token,
            repository_id,
            work_dir: tempfile::tempdir().unwrap(),
            pushes: 0,
        };
        let clone_url = format!(
            "http://admin:{}@{addr}/admin/hello.git",
            harness.plain_token
        );
        git(
            &["clone", &clone_url, "repo"],
            harness.work_dir.path().to_path_buf(),
        )
        .await;
        harness
    }

    fn repo_path(&self) -> PathBuf {
        self.work_dir.path().join("repo")
    }

    /// Commits `files` (path, content) and pushes them to `main`: one push, so one pipeline when there is a file.
    async fn push(&mut self, files: &[(&str, &str)]) {
        self.pushes += 1;
        for (path, content) in files {
            std::fs::write(self.repo_path().join(path), content).unwrap();
        }
        git(&["add", "."], self.repo_path()).await;
        git(
            &[
                "-c",
                "user.email=ci@example.com",
                "-c",
                "user.name=ci",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                &format!("push {}", self.pushes),
            ],
            self.repo_path(),
        )
        .await;
        git(&["push", "origin", "HEAD:main"], self.repo_path()).await;
    }

    async fn pipelines(&self) -> Vec<Value> {
        self.client
            .get(format!(
                "http://{}/api/repositories/{}/pipelines",
                self.addr, self.repository_id
            ))
            .bearer_auth(&self.jwt)
            .send()
            .await
            .unwrap()
            .json::<Vec<Value>>()
            .await
            .unwrap()
    }

    async fn detail(&self, pipeline_id: &str) -> Value {
        self.client
            .get(format!("http://{}/api/pipelines/{pipeline_id}", self.addr))
            .bearer_auth(&self.jwt)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    /// The job a runner would get now, if any.
    async fn claim(&self) -> Option<Value> {
        let res = self
            .client
            .post(format!("http://{}/api/runner/jobs/claim", self.addr))
            .bearer_auth(&self.runner_token)
            .send()
            .await
            .unwrap();
        if res.status() == 204 {
            return None;
        }
        assert_eq!(res.status(), 200);
        Some(res.json().await.unwrap())
    }

    async fn report(&self, job_id: &str, status: &str) {
        let res = self
            .client
            .post(format!(
                "http://{}/api/runner/jobs/{job_id}/result",
                self.addr
            ))
            .bearer_auth(&self.runner_token)
            .json(&json!({ "status": status }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 204);
    }
}

async fn git(args: &[&str], cwd: PathBuf) {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let status = tokio::task::spawn_blocking(move || run_git(&args, &cwd))
        .await
        .unwrap();
    assert!(status.success(), "git command failed");
}

fn run_git(args: &[String], cwd: &Path) -> std::process::ExitStatus {
    Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .unwrap()
}

fn job_statuses(detail: &Value) -> Vec<(String, String)> {
    detail["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|j| {
            (
                j["name"].as_str().unwrap().to_string(),
                j["status"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn pair(name: &str, status: &str) -> (String, String) {
    (name.to_string(), status.to_string())
}

#[sqlx::test]
async fn an_invalid_pipeline_file_shows_up_as_a_failed_pipeline_with_the_parser_message(
    pool: PgPool,
) {
    let mut h = Harness::start(pool).await;

    // No pipeline file: nothing is created.
    h.push(&[("README.md", "# hello\n")]).await;
    assert!(h.pipelines().await.is_empty());

    h.push(&[(
        ".ferrisgit-ci.yml",
        "stages: [build]
jobs:
  a:
    stage: build
    image: alpine:3.20
    script: [\"true\"]
    needs: [b]
  b:
    stage: build
    image: alpine:3.20
    script: [\"true\"]
    needs: [a]
",
    )])
    .await;

    let pipelines = h.pipelines().await;
    assert_eq!(
        pipelines.len(),
        1,
        "the invalid file still yields a pipeline"
    );
    let listed = &pipelines[0];
    assert_eq!(listed["status"], "failed");
    assert!(
        listed["error"].as_str().unwrap().contains("a -> b -> a"),
        "the message names the cycle: {listed}"
    );
    assert!(listed["finishedAt"].is_string());
    assert_eq!(listed["triggeredBy"]["username"], "admin");

    let detail = h.detail(listed["id"].as_str().unwrap()).await;
    assert_eq!(detail["status"], "failed");
    assert_eq!(detail["error"], listed["error"]);
    assert!(detail["jobs"].as_array().unwrap().is_empty());
    assert!(h.claim().await.is_none(), "there is nothing to run");

    // Fixing the file: a regular pipeline, without error, on the next push.
    h.push(&[(".ferrisgit-ci.yml", TWO_STAGES)]).await;
    let pipelines = h.pipelines().await;
    assert_eq!(pipelines.len(), 2);
    assert!(pipelines[0]["error"].is_null());
    assert_eq!(pipelines[0]["status"], "pending");
}

#[sqlx::test]
async fn each_kind_of_invalid_file_is_reported_not_just_logged(pool: PgPool) {
    let mut h = Harness::start(pool).await;
    let cases = [
        ("not: [valid, yaml", "invalid YAML"),
        (
            "stages: [build]\njobs:\n  a:\n    stage: deploy\n    image: alpine\n    script: [\"true\"]\n",
            "stage 'deploy'",
        ),
        (
            "stages: [build]\njobs:\n  a:\n    stage: build\n    image: alpine\n    script: [\"true\"]\n    needs: [ghost]\n",
            "ghost",
        ),
        (
            "stages: [build]\njobs:\n  a:\n    stage: build\n    image: alpine\n    script: [\"true\"]\n    cache: [\"Bad Key\"]\n",
            "Bad Key",
        ),
    ];
    for (index, (yaml, expected)) in cases.iter().enumerate() {
        h.push(&[(".ferrisgit-ci.yml", yaml)]).await;

        let pipelines = h.pipelines().await;
        assert_eq!(pipelines.len(), index + 1);
        assert_eq!(pipelines[0]["status"], "failed");
        assert!(
            pipelines[0]["error"].as_str().unwrap().contains(expected),
            "expected {expected:?} in {}",
            pipelines[0]["error"]
        );
    }
}

#[sqlx::test]
async fn a_second_stage_job_starts_only_after_the_first_stage_succeeded(pool: PgPool) {
    let mut h = Harness::start(pool).await;
    h.push(&[(".ferrisgit-ci.yml", TWO_STAGES)]).await;
    let pipeline_id = h.pipelines().await[0]["id"].as_str().unwrap().to_string();

    let detail = h.detail(&pipeline_id).await;
    assert_eq!(detail["status"], "pending");
    assert_eq!(
        job_statuses(&detail),
        vec![pair("compile", "pending"), pair("unit", "pending")]
    );

    let first = h.claim().await.expect("the first stage is claimable");
    assert_eq!(first["name"], "compile");
    assert_eq!(
        h.detail(&pipeline_id).await["status"],
        "running",
        "the pipeline runs as soon as a job starts"
    );
    assert!(
        h.claim().await.is_none(),
        "`unit` is in the next stage: it waits, although it has no needs"
    );

    h.report(first["id"].as_str().unwrap(), "success").await;
    let second = h.claim().await.expect("the second stage is now released");
    assert_eq!(second["name"], "unit");
    assert_eq!(h.detail(&pipeline_id).await["status"], "running");

    h.report(second["id"].as_str().unwrap(), "success").await;
    let detail = h.detail(&pipeline_id).await;
    assert_eq!(detail["status"], "success");
    assert!(detail["finishedAt"].is_string());
    assert_eq!(
        job_statuses(&detail),
        vec![pair("compile", "success"), pair("unit", "success")]
    );
    assert!(h.claim().await.is_none());
}

#[sqlx::test]
async fn jobs_of_the_same_stage_without_dependencies_start_in_parallel(pool: PgPool) {
    let mut h = Harness::start(pool).await;
    h.push(&[(".ferrisgit-ci.yml", THREE_STAGES_TWO_IN_THE_FIRST)])
        .await;

    let a = h.claim().await.expect("first job of the first stage");
    let b = h.claim().await.expect("second job of the first stage");
    let mut names = vec![
        a["name"].as_str().unwrap().to_string(),
        b["name"].as_str().unwrap().to_string(),
    ];
    names.sort();
    assert_eq!(names, vec!["compile", "lint"]);
    assert!(h.claim().await.is_none(), "the test stage still waits");

    h.report(a["id"].as_str().unwrap(), "success").await;
    assert!(
        h.claim().await.is_none(),
        "the barrier needs every job of the stage, and the other one is still running"
    );
    h.report(b["id"].as_str().unwrap(), "success").await;
    assert_eq!(h.claim().await.unwrap()["name"], "unit");
}

#[sqlx::test]
async fn a_failure_in_the_first_stage_fails_the_pipeline_and_skips_every_later_job(pool: PgPool) {
    let mut h = Harness::start(pool).await;
    h.push(&[(".ferrisgit-ci.yml", THREE_STAGES_TWO_IN_THE_FIRST)])
        .await;
    let pipeline_id = h.pipelines().await[0]["id"].as_str().unwrap().to_string();

    let a = h.claim().await.unwrap();
    let b = h.claim().await.unwrap();
    let (failing, other) = if a["name"] == "compile" {
        (a, b)
    } else {
        (b, a)
    };

    h.report(failing["id"].as_str().unwrap(), "failed").await;
    let detail = h.detail(&pipeline_id).await;
    assert_eq!(
        detail["status"], "running",
        "the other first-stage job is still running: the pipeline is not over yet"
    );
    assert_eq!(
        job_statuses(&detail),
        vec![
            pair(
                "compile",
                if failing["name"] == "compile" {
                    "failed"
                } else {
                    "running"
                }
            ),
            pair(
                "lint",
                if failing["name"] == "lint" {
                    "failed"
                } else {
                    "running"
                }
            ),
            pair("unit", "skipped"),
            pair("ship", "skipped"),
        ]
    );
    assert!(h.claim().await.is_none(), "skipped jobs are never claimed");

    h.report(other["id"].as_str().unwrap(), "success").await;
    let detail = h.detail(&pipeline_id).await;
    assert_eq!(detail["status"], "failed");
    assert!(detail["finishedAt"].is_string());
    let skipped: Vec<_> = detail["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|j| j["status"] == "skipped")
        .collect();
    assert_eq!(skipped.len(), 2);
    assert!(skipped.iter().all(|j| j["finishedAt"].is_string()));
    assert!(skipped.iter().all(|j| j["startedAt"].is_null()));
    assert!(h.claim().await.is_none());
}

#[sqlx::test]
async fn a_pipeline_with_a_single_failing_first_stage_job_ends_failed_at_once(pool: PgPool) {
    let mut h = Harness::start(pool).await;
    h.push(&[(".ferrisgit-ci.yml", TWO_STAGES)]).await;
    let pipeline_id = h.pipelines().await[0]["id"].as_str().unwrap().to_string();

    let compile = h.claim().await.unwrap();
    h.report(compile["id"].as_str().unwrap(), "failed").await;

    let detail = h.detail(&pipeline_id).await;
    assert_eq!(detail["status"], "failed");
    assert_eq!(
        job_statuses(&detail),
        vec![pair("compile", "failed"), pair("unit", "skipped")]
    );
    let listed = &h.pipelines().await[0];
    assert_eq!(listed["status"], "failed");
    assert!(
        listed["error"].is_null(),
        "no parser error: the file was valid"
    );
}
