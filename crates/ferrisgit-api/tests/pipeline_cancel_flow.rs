// `POST /api/pipelines/{id}/cancel`. Pipeline and job fixtures are seeded through the store ports, so no runner or Docker is needed.
// Cancelling an already-terminal pipeline must leave its status untouched.

use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use ferrisgit_domain::job::{JobStatus, NewJob};
use ferrisgit_domain::pipeline::{NewPipeline, PipelineStatus};
use ferrisgit_domain::settings::ExecutionEngine;
use serde_json::json;
use sqlx::PgPool;
use std::net::SocketAddr;
use uuid::Uuid;

async fn spawn_server(pool: PgPool) -> (SocketAddr, AppState) {
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

    let storage_dir = tempfile::tempdir().unwrap().keep();
    let static_dir = tempfile::tempdir().unwrap().keep();
    std::fs::write(static_dir.join("index.html"), "<html></html>").unwrap();

    let config = Config {
        database_url: String::new(),
        jwt_secret: "test-secret-that-is-at-least-32-characters-long".to_string(),
        storage_root: storage_dir.to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:0".to_string(),
        static_dir: static_dir.to_string_lossy().to_string(),
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

    // Cloned before `build_router` consumes `state`, to seed fixtures the HTTP API has no route to create.
    let state_for_fixtures = state.clone();

    let app = build_router(state, &static_dir);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, state_for_fixtures)
}

async fn login(
    client: &reqwest::Client,
    addr: SocketAddr,
    username: &str,
    password: &str,
) -> String {
    let res: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    res["token"].as_str().unwrap().to_string()
}

#[sqlx::test]
async fn canceling_a_pending_pipeline_cancels_it_and_its_jobs_over_http(pool: PgPool) {
    let (addr, state) = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let jwt = login(&client, addr, "admin", "adminpassword123").await;

    let repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(&jwt)
        .json(&json!({ "name": "hello", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
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

    let cancel_res = client
        .post(format!(
            "http://{addr}/api/pipelines/{}/cancel",
            pipeline.id
        ))
        .bearer_auth(&jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(cancel_res.status(), 204);

    let detail: serde_json::Value = client
        .get(format!("http://{addr}/api/pipelines/{}", pipeline.id))
        .bearer_auth(&jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
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

    let repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(&jwt)
        .json(&json!({ "name": "hello", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
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

        let cancel_res = client
            .post(format!(
                "http://{addr}/api/pipelines/{}/cancel",
                pipeline.id
            ))
            .bearer_auth(&jwt)
            .send()
            .await
            .unwrap();
        assert_eq!(
            cancel_res.status(),
            204,
            "the cancel route itself must still report success even though it did nothing (mirrors the use case's own Ok(()) no-op)"
        );

        let detail: serde_json::Value = client
            .get(format!("http://{addr}/api/pipelines/{}", pipeline.id))
            .bearer_auth(&jwt)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
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

    client
        .post(format!("http://{addr}/api/admin/users"))
        .bearer_auth(&admin_jwt)
        .json(&json!({ "username": "reader", "email": "reader@example.com", "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let reader_jwt = login(&client, addr, "reader", "password12345").await;

    let repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(&admin_jwt)
        .json(&json!({ "name": "hello", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let repository_id: Uuid = repo_res["id"].as_str().unwrap().parse().unwrap();
    let repo_id_str = repo_res["id"].as_str().unwrap();

    // A Reader can see pipelines, but `cancel` requires Contributor+.
    client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id_str}/collaborators"
        ))
        .bearer_auth(&admin_jwt)
        .json(&json!({ "username": "reader", "role": "reader" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

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

    let reader_cancel_attempt = client
        .post(format!(
            "http://{addr}/api/pipelines/{}/cancel",
            pipeline.id
        ))
        .bearer_auth(&reader_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        reader_cancel_attempt.status(),
        404,
        "a Reader must not be able to cancel a pipeline (masked as NotFound, same convention as require_role_by_id elsewhere)"
    );

    let detail: serde_json::Value = client
        .get(format!("http://{addr}/api/pipelines/{}", pipeline.id))
        .bearer_auth(&admin_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(detail["status"], "pending");
}
