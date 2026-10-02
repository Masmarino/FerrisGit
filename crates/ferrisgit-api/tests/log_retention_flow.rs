// Validation over HTTP of the logRetentionDays, maxConcurrentJobs and runnerRegistrationToken settings, and the
// retention itself: the sweep empties the logs of old finished jobs and the pipeline detail says so. Fixtures go through
// the store ports and finish times are moved back with SQL.

mod common;

use common::http::post_anon;

use chrono::Utc;
use ferrisgit_api::state::AppState;
use ferrisgit_domain::job::{JobStatus, NewJob};
use ferrisgit_domain::pipeline::NewPipeline;
use ferrisgit_domain::settings::ExecutionEngine;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;
use uuid::Uuid;

struct Harness {
    addr: SocketAddr,
    state: AppState,
    pool: PgPool,
    client: reqwest::Client,
    jwt: String,
}

async fn spawn(pool: PgPool) -> Harness {
    let app = common::spawn_app(pool.clone()).await;
    let addr = app.addr;

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
    Harness {
        addr,
        state: app.state,
        pool,
        client,
        jwt,
    }
}

impl Harness {
    async fn put_settings(&self, body: Value) -> reqwest::Response {
        self.client
            .put(format!("http://{}/api/admin/settings", self.addr))
            .bearer_auth(&self.jwt)
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    async fn get_settings(&self) -> Value {
        self.client
            .get(format!("http://{}/api/admin/settings", self.addr))
            .bearer_auth(&self.jwt)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    async fn pipeline_with_jobs(&self, names: &[&str]) -> (Uuid, Vec<Uuid>) {
        let repo: Value = self
            .client
            .post(format!("http://{}/api/repositories", self.addr))
            .bearer_auth(&self.jwt)
            .json(&json!({ "name": "hello", "visibility": "private" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let admin_id = self
            .state
            .users
            .find_by_username("admin")
            .await
            .unwrap()
            .unwrap()
            .id;
        let pipeline = self
            .state
            .pipelines
            .create(NewPipeline {
                repository_id: repo["id"].as_str().unwrap().parse().unwrap(),
                commit_sha: "abc123".to_string(),
                execution_engine: ExecutionEngine::DockerRunners,
                triggered_by: admin_id,
            })
            .await
            .unwrap();
        let mut ids = vec![];
        for name in names {
            let job = self
                .state
                .jobs
                .create(NewJob {
                    pipeline_id: pipeline.id,
                    stage: "test".to_string(),
                    name: name.to_string(),
                    image: "alpine".to_string(),
                    script: vec!["echo hi".to_string()],
                    variables: Default::default(),
                    needs: vec![],
                    tags: vec![],
                    cache: vec![],
                })
                .await
                .unwrap();
            ids.push(job.id);
        }
        (pipeline.id, ids)
    }

    /// Ends the job with `status` and `logs`, `days_ago` days ago.
    async fn finish(&self, job_id: Uuid, status: JobStatus, logs: &str, days_ago: i32) {
        self.state.jobs.append_logs(job_id, logs).await.unwrap();
        self.state.jobs.update_status(job_id, status).await.unwrap();
        sqlx::query(
            "UPDATE jobs SET finished_at = now() - make_interval(days => $1) WHERE id = $2",
        )
        .bind(days_ago)
        .bind(job_id)
        .execute(&self.pool)
        .await
        .unwrap();
    }

    async fn pipeline_detail(&self, pipeline_id: Uuid) -> Value {
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
}

fn job<'a>(detail: &'a Value, name: &str) -> &'a Value {
    detail["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["name"] == name)
        .unwrap()
}

#[sqlx::test]
async fn the_retention_and_the_concurrency_ceiling_are_stored_cleared_and_validated(pool: PgPool) {
    let h = spawn(pool).await;
    let settings = h.get_settings().await;
    assert!(settings["logRetentionDays"].is_null());
    assert!(settings["maxConcurrentJobs"].is_null());

    let saved: Value = h
        .put_settings(json!({ "logRetentionDays": 30, "maxConcurrentJobs": 4 }))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(saved["logRetentionDays"], 30);
    assert_eq!(saved["maxConcurrentJobs"], 4);

    for body in [
        json!({ "logRetentionDays": 0 }),
        json!({ "logRetentionDays": -7 }),
        json!({ "maxConcurrentJobs": 0 }),
        json!({ "maxConcurrentJobs": -1 }),
    ] {
        let res = h.put_settings(body.clone()).await;
        assert_eq!(res.status(), 400, "{body} must be refused");
    }
    let unchanged = h.get_settings().await;
    assert_eq!(
        unchanged["logRetentionDays"], 30,
        "a refused update changes nothing"
    );
    assert_eq!(unchanged["maxConcurrentJobs"], 4);

    let cleared: Value = h
        .put_settings(json!({ "logRetentionDays": null, "maxConcurrentJobs": null }))
        .await
        .json()
        .await
        .unwrap();
    assert!(cleared["logRetentionDays"].is_null());
    assert!(cleared["maxConcurrentJobs"].is_null());
}

#[sqlx::test]
async fn an_empty_registration_token_is_refused_and_a_real_one_only_reports_that_it_is_configured(
    pool: PgPool,
) {
    let h = spawn(pool).await;

    assert_eq!(
        h.put_settings(json!({ "runnerRegistrationToken": "" }))
            .await
            .status(),
        400
    );
    assert_eq!(
        h.get_settings().await["runnerRegistrationTokenConfigured"],
        false
    );

    let saved: Value = h
        .put_settings(json!({ "runnerRegistrationToken": "0123456789abcdef" }))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(saved["runnerRegistrationTokenConfigured"], true);
    assert!(
        !saved.to_string().contains("0123456789abcdef"),
        "the token never comes back"
    );

    let removed: Value = h
        .put_settings(json!({ "runnerRegistrationToken": null }))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(removed["runnerRegistrationTokenConfigured"], false);
}

#[sqlx::test]
async fn the_sweep_empties_the_logs_of_old_finished_jobs_and_the_pipeline_says_so(pool: PgPool) {
    let h = spawn(pool).await;
    let (pipeline_id, ids) = h
        .pipeline_with_jobs(&["old-ok", "old-failed", "recent", "running", "empty"])
        .await;
    h.finish(ids[0], JobStatus::Success, "compiled\n", 40).await;
    h.finish(ids[1], JobStatus::Failed, "boom\n", 31).await;
    h.finish(ids[2], JobStatus::Success, "fresh\n", 5).await;
    h.state
        .jobs
        .append_logs(ids[3], "still going\n")
        .await
        .unwrap();
    h.state
        .jobs
        .update_status(ids[3], JobStatus::Running)
        .await
        .unwrap();
    h.finish(ids[4], JobStatus::Success, "", 60).await;
    h.put_settings(json!({ "logRetentionDays": 30 })).await;

    let purged = h
        .state
        .purge_expired_job_logs
        .execute(Utc::now())
        .await
        .unwrap();

    assert_eq!(purged, 2);
    let detail = h.pipeline_detail(pipeline_id).await;
    for name in ["old-ok", "old-failed"] {
        assert_eq!(job(&detail, name)["logs"], "", "{name}: log emptied");
        assert!(
            job(&detail, name)["logsPurgedAt"].is_string(),
            "{name}: purge date given"
        );
    }
    assert_eq!(
        job(&detail, "old-ok")["status"],
        "success",
        "the status stays"
    );
    assert_eq!(job(&detail, "old-failed")["status"], "failed");
    assert!(job(&detail, "old-ok")["finishedAt"].is_string());
    assert_eq!(job(&detail, "recent")["logs"], "fresh\n");
    assert!(job(&detail, "recent")["logsPurgedAt"].is_null());
    assert_eq!(job(&detail, "running")["logs"], "still going\n");
    assert!(job(&detail, "running")["logsPurgedAt"].is_null());
    assert!(
        job(&detail, "empty")["logsPurgedAt"].is_null(),
        "a job that never had a log was not purged"
    );
    assert_eq!(
        detail["jobs"].as_array().unwrap().len(),
        5,
        "no job disappears"
    );
}

#[sqlx::test]
async fn without_a_retention_the_sweep_keeps_every_log(pool: PgPool) {
    let h = spawn(pool).await;
    let (pipeline_id, ids) = h.pipeline_with_jobs(&["ancient"]).await;
    h.finish(ids[0], JobStatus::Success, "very old log\n", 900)
        .await;

    assert_eq!(
        h.state
            .purge_expired_job_logs
            .execute(Utc::now())
            .await
            .unwrap(),
        0
    );

    let detail = h.pipeline_detail(pipeline_id).await;
    assert_eq!(job(&detail, "ancient")["logs"], "very old log\n");
    assert!(job(&detail, "ancient")["logsPurgedAt"].is_null());
}

#[sqlx::test]
async fn a_second_sweep_does_nothing_and_a_shorter_retention_reaches_more_jobs(pool: PgPool) {
    let h = spawn(pool).await;
    let (pipeline_id, ids) = h.pipeline_with_jobs(&["a", "b"]).await;
    h.finish(ids[0], JobStatus::Success, "a log\n", 40).await;
    h.finish(ids[1], JobStatus::Success, "b log\n", 10).await;

    h.put_settings(json!({ "logRetentionDays": 30 })).await;
    assert_eq!(
        h.state
            .purge_expired_job_logs
            .execute(Utc::now())
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        h.state
            .purge_expired_job_logs
            .execute(Utc::now())
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        job(&h.pipeline_detail(pipeline_id).await, "b")["logs"],
        "b log\n"
    );

    h.put_settings(json!({ "logRetentionDays": 7 })).await;
    assert_eq!(
        h.state
            .purge_expired_job_logs
            .execute(Utc::now())
            .await
            .unwrap(),
        1
    );
    assert_eq!(job(&h.pipeline_detail(pipeline_id).await, "b")["logs"], "");
}
