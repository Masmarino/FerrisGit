mod common;

use ferrisgit_application::job_execution_resolver::JobExecutionResolver;
use ferrisgit_application::use_cases::cancel_pipeline::CancelPipelineUseCase;
use ferrisgit_application::use_cases::create_pipeline::CreatePipelineUseCase;
use ferrisgit_domain::job::{JobStatus, JobStorePort};
use ferrisgit_domain::job_execution::JobExecutionPort;
use ferrisgit_domain::pipeline::{PipelineStatus, PipelineStorePort};
use ferrisgit_domain::settings::{ExecutionEngine, SystemSettingsStorePort, SystemSettingsUpdate};
use ferrisgit_infrastructure::docker_runner_executor::DockerRunnerExecutor;
use ferrisgit_infrastructure::kubernetes::pod_executor::KubernetesPodExecutor;
use ferrisgit_infrastructure::kubernetes::pod_spec::pod_name;
use ferrisgit_infrastructure::kubernetes::test_support::{TestNamespace, test_client};
use ferrisgit_infrastructure::postgres::job_store::PostgresJobStore;
use ferrisgit_infrastructure::postgres::notification_store::PostgresNotificationStore;
use ferrisgit_infrastructure::postgres::pipeline_store::PostgresPipelineStore;
use ferrisgit_infrastructure::postgres::repository_store::PostgresRepositoryStore;
use ferrisgit_infrastructure::postgres::system_settings_store::PostgresSystemSettingsStore;
use ferrisgit_infrastructure::postgres::user_repository::PostgresUserRepository;
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{sleep, timeout};

const LONG_RUNNING_PIPELINE_YAML: &str = r#"
stages: [build]
jobs:
  build:
    stage: build
    image: busybox:1.36
    script:
      - "sleep 60"
"#;

#[sqlx::test(migrations = "../../migrations")]
async fn canceling_a_kubernetes_pipeline_still_deletes_its_real_pod_after_the_live_setting_moves_to_docker_runners(
    pool: sqlx::PgPool,
) {
    let (repository_id, owner_id) = common::insert_repository(&pool).await;

    let pipelines = Arc::new(PostgresPipelineStore::new(pool.clone()));
    let jobs = Arc::new(PostgresJobStore::new(pool.clone()));
    let system_settings = Arc::new(PostgresSystemSettingsStore::new(pool.clone()));
    let events = Arc::new(common::FakeEvents);

    let client = test_client().await;
    let ns = TestNamespace::create(&client).await;
    system_settings
        .update(SystemSettingsUpdate {
            execution_engine: Some(ExecutionEngine::Kubernetes),
            k8s_namespace: Some(Some(ns.name.clone())),
            ..Default::default()
        })
        .await
        .unwrap();

    let docker_executor: Arc<dyn JobExecutionPort> =
        Arc::new(DockerRunnerExecutor::new(jobs.clone()));
    let kubernetes_executor: Arc<dyn JobExecutionPort> = Arc::new(KubernetesPodExecutor::new(
        client.clone(),
        pipelines.clone(),
        jobs.clone(),
        system_settings.clone(),
        events.clone(),
        ns.name.clone(),
    ));
    let job_execution = Arc::new(JobExecutionResolver::new(
        docker_executor,
        kubernetes_executor,
    ));

    let create_pipeline = CreatePipelineUseCase::new(
        Arc::new(common::FakeRepositorySettings),
        system_settings.clone(),
        pipelines.clone(),
        jobs.clone(),
        Arc::new(common::FakeFileReader(
            LONG_RUNNING_PIPELINE_YAML.to_string(),
        )),
        events.clone(),
        job_execution.clone(),
        Arc::new(PostgresRepositoryStore::new(pool.clone())),
        Arc::new(PostgresUserRepository::new(pool.clone())),
        Arc::new(PostgresNotificationStore::new(pool.clone())),
        Arc::new(common::FakeWebhooks),
    );
    let pipeline = create_pipeline
        .execute(repository_id, "unused-disk-path", "deadbeef", owner_id)
        .await
        .unwrap()
        .unwrap();
    let build_job = jobs
        .list_for_pipeline(pipeline.id)
        .await
        .unwrap()
        .into_iter()
        .find(|j| j.name == "build")
        .unwrap();

    let pods: Api<Pod> = Api::namespaced(client, &ns.name);
    let build_pod_name = pod_name(build_job.id);
    timeout(Duration::from_secs(30), async {
        while pods.get(&build_pod_name).await.is_err() {
            sleep(Duration::from_millis(500)).await;
        }
    })
    .await
    .expect("the build job's Pod should have been created");

    // The setting flips after the pipeline was tagged kubernetes; the pipeline keeps its original engine.
    system_settings
        .update(SystemSettingsUpdate {
            execution_engine: Some(ExecutionEngine::DockerRunners),
            ..Default::default()
        })
        .await
        .unwrap();

    let cancel_pipeline =
        CancelPipelineUseCase::new(pipelines.clone(), jobs.clone(), job_execution, events);
    cancel_pipeline.execute(pipeline.id).await.unwrap();

    assert_eq!(
        pipelines
            .find_by_id(pipeline.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        PipelineStatus::Canceled
    );
    assert_eq!(
        jobs.find_by_id(build_job.id).await.unwrap().unwrap().status,
        JobStatus::Canceled
    );
    let deleted = timeout(Duration::from_secs(15), async {
        while pods.get(&build_pod_name).await.is_ok() {
            sleep(Duration::from_millis(500)).await;
        }
    })
    .await;
    assert!(
        deleted.is_ok(),
        "the real Pod must actually be deleted — if cancellation had silently routed to DockerRunnerExecutor instead, this Pod would run for the full 60s untouched"
    );
}
