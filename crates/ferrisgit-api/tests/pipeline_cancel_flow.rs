// `POST /api/pipelines/{id}/cancel`. Pipeline and job fixtures are seeded through the store ports, so no runner or Docker is needed.
// Cancelling an already-terminal pipeline must leave its status untouched.

mod common;

use common::http::{create_user, get_json, post_empty, post_json, post_ok};

use common::http::login;
use ferrisgit_api::state::AppState;
use ferrisgit_domain::job::{JobStatus, NewJob};
use ferrisgit_domain::pipeline::{NewPipeline, PipelineStatus};
use ferrisgit_domain::settings::ExecutionEngine;
use serde_json::json;
use sqlx::PgPool;
use std::net::SocketAddr;
use uuid::Uuid;

async fn spawn_server(pool: PgPool) -> (SocketAddr, AppState) {
    let app = common::spawn_app(pool).await;
    (app.addr, app.state)
}

#[sqlx::test]
async fn canceling_a_pending_pipeline_cancels_it_and_its_jobs_over_http(pool: PgPool) {
    let (addr, state) = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let jwt = login(&client, addr, "admin", "adminpassword123").await;

    let repo_res: serde_json::Value = post_json(
        &client,
        addr,
        &jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    let repository_id: Uuid = repo_res["id"].as_str().unwrap().parse().unwrap();

    let admin_id: Uuid = state
        .users
        .find_by_username("admin")
        .await
        .unwrap()
        .unwrap()
        .id;

    let pipeline = state
        .pipelines
        .create(NewPipeline {
            repository_id,
            commit_sha: "abc123".to_string(),
            execution_engine: ExecutionEngine::DockerRunners,
            triggered_by: admin_id,
        })
        .await
        .unwrap();
    assert_eq!(pipeline.status, PipelineStatus::Pending);

    let job = state
        .jobs
        .create(NewJob {
            pipeline_id: pipeline.id,
            stage: "build".to_string(),
            name: "compile".to_string(),
            image: "alpine".to_string(),
            script: vec!["echo hi".to_string()],
            variables: Default::default(),
            needs: vec![],
            tags: vec![],
            cache: vec![],
        })
        .await
        .unwrap();
    assert_eq!(job.status, JobStatus::Pending);

    let cancel_res = post_empty(
        &client,
        addr,
        &jwt,
        &format!("/pipelines/{}/cancel", pipeline.id),
    )
    .await;
    assert_eq!(cancel_res.status(), 204);

    let detail: serde_json::Value =
        get_json(&client, addr, &jwt, &format!("/pipelines/{}", pipeline.id)).await;
    assert_eq!(detail["status"], "canceled");
    let jobs = detail["jobs"].as_array().unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(
        jobs[0]["status"], "canceled",
        "a Pending job on a canceled pipeline must itself become canceled"
    );
    assert!(
        detail["finishedAt"].is_string(),
        "a canceled pipeline is finished"
    );
    assert!(
        jobs[0]["finishedAt"].is_string(),
        "a canceled job is finished"
    );
    assert!(
        jobs[0]["startedAt"].is_null(),
        "a job that never ran has no start time"
    );
}

#[sqlx::test]
async fn canceling_an_already_terminal_pipeline_over_http_is_a_no_op(pool: PgPool) {
    let (addr, state) = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let jwt = login(&client, addr, "admin", "adminpassword123").await;

    let repo_res: serde_json::Value = post_json(
        &client,
        addr,
        &jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    let repository_id: Uuid = repo_res["id"].as_str().unwrap().parse().unwrap();
    let admin_id: Uuid = state
        .users
        .find_by_username("admin")
        .await
        .unwrap()
        .unwrap()
        .id;

    for terminal_status in [
        PipelineStatus::Success,
        PipelineStatus::Failed,
        PipelineStatus::Canceled,
    ] {
        let pipeline = state
            .pipelines
            .create(NewPipeline {
                repository_id,
                commit_sha: "abc123".to_string(),
                execution_engine: ExecutionEngine::DockerRunners,
                triggered_by: admin_id,
            })
            .await
            .unwrap();
        state
            .pipelines
            .update_status(pipeline.id, terminal_status)
            .await
            .unwrap();
        let job = state
            .jobs
            .create(NewJob {
                pipeline_id: pipeline.id,
                stage: "build".to_string(),
                name: "compile".to_string(),
                image: "alpine".to_string(),
                script: vec!["echo hi".to_string()],
                variables: Default::default(),
                needs: vec![],
                tags: vec![],
                cache: vec![],
            })
            .await
            .unwrap();
        // The job mirrors the pipeline's terminal status, so it is a real non-cancelable outcome.
        let job_terminal_status = match terminal_status {
            PipelineStatus::Success => JobStatus::Success,
            PipelineStatus::Failed => JobStatus::Failed,
            PipelineStatus::Canceled => JobStatus::Canceled,
            _ => unreachable!(),
        };
        state
            .jobs
            .update_status(job.id, job_terminal_status)
            .await
            .unwrap();

        let cancel_res = post_empty(
            &client,
            addr,
            &jwt,
            &format!("/pipelines/{}/cancel", pipeline.id),
        )
        .await;
        assert_eq!(
            cancel_res.status(),
            204,
            "the cancel route itself must still report success even though it did nothing (mirrors the use case's own Ok(()) no-op)"
        );

        let detail: serde_json::Value =
            get_json(&client, addr, &jwt, &format!("/pipelines/{}", pipeline.id)).await;
        assert_eq!(
            detail["status"],
            terminal_status.as_str(),
            "an already-terminal pipeline's status must NOT be overwritten with 'canceled' by a cancel request"
        );
        let jobs = detail["jobs"].as_array().unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(
            jobs[0]["status"],
            job_terminal_status.as_str(),
            "a job belonging to an already-terminal pipeline must not be touched by a cancel request"
        );
    }
}

#[sqlx::test]
async fn canceling_a_pipeline_requires_contributor_access_to_its_repository(pool: PgPool) {
    let (addr, state) = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let admin_jwt = login(&client, addr, "admin", "adminpassword123").await;

    create_user(&client, addr, &admin_jwt, "reader").await;
    let reader_jwt = login(&client, addr, "reader", "password12345").await;

    let repo_res: serde_json::Value = post_json(
        &client,
        addr,
        &admin_jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    let repository_id: Uuid = repo_res["id"].as_str().unwrap().parse().unwrap();
    let repo_id_str = repo_res["id"].as_str().unwrap();

    // A Reader can see pipelines, but `cancel` requires Contributor+.
    post_ok(
        &client,
        addr,
        &admin_jwt,
        &format!("/repositories/{repo_id_str}/collaborators"),
        &json!({ "username": "reader", "role": "reader" }),
    )
    .await;

    let admin_id: Uuid = state
        .users
        .find_by_username("admin")
        .await
        .unwrap()
        .unwrap()
        .id;
    let pipeline = state
        .pipelines
        .create(NewPipeline {
            repository_id,
            commit_sha: "abc123".to_string(),
            execution_engine: ExecutionEngine::DockerRunners,
            triggered_by: admin_id,
        })
        .await
        .unwrap();

    let reader_cancel_attempt = post_empty(
        &client,
        addr,
        &reader_jwt,
        &format!("/pipelines/{}/cancel", pipeline.id),
    )
    .await;
    assert_eq!(
        reader_cancel_attempt.status(),
        404,
        "a Reader must not be able to cancel a pipeline (masked as NotFound, same convention as require_role_by_id elsewhere)"
    );

    let detail: serde_json::Value = get_json(
        &client,
        addr,
        &admin_jwt,
        &format!("/pipelines/{}", pipeline.id),
    )
    .await;
    assert_eq!(detail["status"], "pending");
}
