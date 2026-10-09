// A pipeline run end to end on Kubernetes: the real server, a kind Pod that clones the repository with its job token,
// reads the repository's secrets from its Secret, builds and runs an image with its own Docker daemon, and whose logs
// come back with the masked values hidden. Needs the kind cluster `kind-ferrisgit-test` and Docker Desktop (the Pod
// reaches this machine at host.docker.internal).

mod common;

use common::git::push_files;
use common::http::{get_json, login, post_json, post_ok};
use common::{Options, poll_until};

use serde_json::json;
use sqlx::PgPool;
use std::process::Command;
use std::time::Duration;

const CONTEXT: &str = "kind-ferrisgit-test";

/// The server reads the cluster of the current kubectl context: anything but the throwaway kind cluster is refused.
fn assert_on_the_kind_cluster() {
    let output = Command::new("kubectl")
        .args(["config", "current-context"])
        .output()
        .expect("kubectl must be installed");
    let context = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(
        context, CONTEXT,
        "these tests create Pods in the current context's cluster: switch to {CONTEXT} first"
    );
}

/// A namespace of its own, deleted at the end, panics included.
struct Namespace(String);

impl Namespace {
    fn create() -> Self {
        let name = format!(
            "ferrisgit-e2e-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        );
        let status = Command::new("kubectl")
            .args(["--context", CONTEXT, "create", "namespace", &name])
            .status()
            .unwrap();
        assert!(status.success());
        Self(name)
    }
}

impl Drop for Namespace {
    fn drop(&mut self) {
        let _ = Command::new("kubectl")
            .args([
                "--context",
                CONTEXT,
                "delete",
                "namespace",
                &self.0,
                "--wait=false",
            ])
            .status();
    }
}

const DOCKERFILE: &[u8] =
    b"FROM alpine:3.24\nRUN echo built-inside-the-pod > /proof\nCMD [\"cat\", \"/proof\"]\n";

/// Pushes `pipeline` with a file and a Dockerfile to a new repository that has a masked `DEPLOY_TOKEN`, on a server
/// running jobs on kind, Docker in jobs on or off, and returns the pipeline's status and its job's logs.
async fn run_on_kind(pool: PgPool, docker_in_jobs: bool, pipeline: &str) -> (String, String) {
    assert_on_the_kind_cluster();
    let namespace = Namespace::create();
    // Before the server starts: its watcher reads the namespace once, at boot.
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::query("INSERT INTO system_settings (id, execution_engine, k8s_namespace, docker_in_jobs) VALUES (true, 'kubernetes', $1, $2)")
        .bind(&namespace.0)
        .bind(docker_in_jobs)
        .execute(&pool)
        .await
        .unwrap();
    let addr = common::spawn_app_with(
        pool,
        Options {
            reachable_from_kind: true,
            ..Options::default()
        },
        |_| {},
    )
    .await
    .addr;

    let client = reqwest::Client::new();
    let jwt = login(&client, addr, "admin", "adminpassword123").await;
    let token: serde_json::Value =
        post_json(&client, addr, &jwt, "/tokens", &json!({ "name": "push" })).await;
    let token = token["token"].as_str().unwrap().to_string();
    let repo: serde_json::Value = post_json(
        &client,
        addr,
        &jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    let repo_id = repo["id"].as_str().unwrap().to_string();
    post_ok(
        &client,
        addr,
        &jwt,
        &format!("/repositories/{repo_id}/ci-variables"),
        &json!({ "key": "DEPLOY_TOKEN", "value": "kube-s3cret-value", "masked": true }),
    )
    .await;

    assert!(
        push_files(
            &format!("http://admin:{token}@{addr}/admin/hello.git"),
            "main",
            "add the pipeline",
            &[
                (".ferrisgit-ci.yml", pipeline.as_bytes()),
                ("hello.txt", b"hello from the repository\n"),
                ("Dockerfile", DOCKERFILE),
            ],
        )
        .await
    );

    let pipeline = poll_until(
        || {
            let (client, jwt, repo_id) = (client.clone(), jwt.clone(), repo_id.clone());
            async move {
                let list: serde_json::Value = get_json(
                    &client,
                    addr,
                    &jwt,
                    &format!("/repositories/{repo_id}/pipelines"),
                )
                .await;
                list.as_array()
                    .and_then(|pipelines| pipelines.first().cloned())
            }
        },
        Duration::from_secs(10),
        "a pipeline after the push",
    )
    .await;
    let pipeline_id = pipeline["id"].as_str().unwrap().to_string();

    let finished = poll_until(
        || {
            let (client, jwt, pipeline_id) = (client.clone(), jwt.clone(), pipeline_id.clone());
            async move {
                let pipeline: serde_json::Value =
                    get_json(&client, addr, &jwt, &format!("/pipelines/{pipeline_id}")).await;
                let status = pipeline["status"].as_str().unwrap_or("").to_string();
                matches!(status.as_str(), "success" | "failed" | "canceled").then_some(pipeline)
            }
        },
        Duration::from_secs(600),
        "the pipeline to finish on kind (the first run pulls docker:dind, alpine/git and alpine)",
    )
    .await;
    (
        finished["status"].as_str().unwrap_or_default().to_string(),
        finished["jobs"][0]["logs"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    )
}

#[sqlx::test]
async fn a_kubernetes_job_clones_its_repository_reads_its_secrets_and_builds_an_image(
    pool: PgPool,
) {
    let pipeline = r#"stages: [build]
jobs:
  image:
    stage: build
    image: alpine:3.24
    script:
      - cat hello.txt
      - echo "the token is $DEPLOY_TOKEN"
      - docker build -q -t proof .
      - docker run --rm proof
"#;

    let (status, logs) = run_on_kind(pool, true, pipeline).await;

    assert_eq!(status, "success", "logs:\n{logs}");
    assert!(
        logs.contains("hello from the repository"),
        "the repository was cloned:\n{logs}"
    );
    assert!(
        logs.contains("the token is ***"),
        "the secret reached the job, masked:\n{logs}"
    );
    assert!(!logs.contains("kube-s3cret-value"));
    assert!(
        logs.contains("built-inside-the-pod"),
        "the job built and ran an image:\n{logs}"
    );
}

#[sqlx::test]
async fn without_docker_in_jobs_a_kubernetes_job_has_its_repository_and_no_docker(pool: PgPool) {
    let pipeline = r#"stages: [test]
jobs:
  check:
    stage: test
    image: alpine:3.24
    script:
      - cat hello.txt
      - test -z "$DOCKER_HOST"
      - "! command -v docker"
      - echo no-docker-here
"#;

    let (status, logs) = run_on_kind(pool, false, pipeline).await;

    assert_eq!(status, "success", "logs:\n{logs}");
    assert!(logs.contains("hello from the repository"));
    assert!(logs.contains("no-docker-here"));
}
