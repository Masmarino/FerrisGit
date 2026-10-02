mod common;

use ferrisgit_application::job_execution_resolver::JobExecutionResolver;
use ferrisgit_application::use_cases::create_pipeline::CreatePipelineUseCase;
use ferrisgit_application::use_cases::report_job_result::{
    AppendJobLogsUseCase, ReportJobResultUseCase,
};
use ferrisgit_domain::job::{JobStatus, JobStorePort};
use ferrisgit_domain::job_execution::JobExecutionPort;
use ferrisgit_domain::pipeline::{PipelineStatus, PipelineStorePort};
use ferrisgit_domain::settings::{ExecutionEngine, SystemSettingsStorePort, SystemSettingsUpdate};
use ferrisgit_infrastructure::docker_runner_executor::DockerRunnerExecutor;
use ferrisgit_infrastructure::kubernetes::cache::pvc_name;
use ferrisgit_infrastructure::kubernetes::pod_executor::KubernetesPodExecutor;
use ferrisgit_infrastructure::kubernetes::test_support::{TestNamespace, test_client};
use ferrisgit_infrastructure::kubernetes::watcher::PodWatcher;
use ferrisgit_infrastructure::postgres::job_store::PostgresJobStore;
use ferrisgit_infrastructure::postgres::notification_store::PostgresNotificationStore;
use ferrisgit_infrastructure::postgres::pipeline_store::PostgresPipelineStore;
use ferrisgit_infrastructure::postgres::repository_store::PostgresRepositoryStore;
use ferrisgit_infrastructure::postgres::system_settings_store::PostgresSystemSettingsStore;
use ferrisgit_infrastructure::postgres::user_repository::PostgresUserRepository;
use k8s_openapi::api::core::v1::{
    HostPathVolumeSource, ObjectReference, PersistentVolume, PersistentVolumeSpec,
};
use k8s_openapi::api::storage::v1::StorageClass;
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use kube::api::{Api, ObjectMeta, PostParams};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use uuid::Uuid;

/// `ensure_cache_pvcs` always requests `ReadWriteMany`, but kind's default `standard` StorageClass
/// (local-path) rejects RWX at provisioning, so this test supplies a `no-provisioner` StorageClass and a
/// pre-created `PersistentVolume` `claimRef`-pinned to the PVC name the production code will create.
async fn provision_static_rwx_storage(
    client: &kube::Client,
    namespace: &str,
    claim_name: &str,
) -> String {
    const STORAGE_CLASS_NAME: &str = "ferrisgit-test-static-rwx";

    let storage_classes: Api<StorageClass> = Api::all(client.clone());
    let sc = StorageClass {
        metadata: ObjectMeta {
            name: Some(STORAGE_CLASS_NAME.to_string()),
            ..Default::default()
        },
        provisioner: "kubernetes.io/no-provisioner".to_string(),
        volume_binding_mode: Some("Immediate".to_string()),
        ..Default::default()
    };
    match storage_classes.create(&PostParams::default(), &sc).await {
        Ok(_) => {}
        Err(kube::Error::Api(err)) if err.code == 409 => {} // created by an earlier test run on this shared cluster — fine, it's idempotent
        Err(err) => panic!("failed to create test StorageClass: {err}"),
    }

    let mut capacity = BTreeMap::new();
    capacity.insert("storage".to_string(), Quantity("5Gi".to_string()));
    let pv_name = format!("pv-{}", Uuid::new_v4());
    let pv = PersistentVolume {
        metadata: ObjectMeta {
            name: Some(pv_name.clone()),
            ..Default::default()
        },
        spec: Some(PersistentVolumeSpec {
            access_modes: Some(vec!["ReadWriteMany".to_string()]),
            capacity: Some(capacity),
            storage_class_name: Some(STORAGE_CLASS_NAME.to_string()),
            persistent_volume_reclaim_policy: Some("Delete".to_string()),
            host_path: Some(HostPathVolumeSource {
                path: format!("/tmp/{pv_name}"),
                type_: Some("DirectoryOrCreate".to_string()),
            }),
            claim_ref: Some(ObjectReference {
                name: Some(claim_name.to_string()),
                namespace: Some(namespace.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    };
    let pvs: Api<PersistentVolume> = Api::all(client.clone());
    pvs.create(&PostParams::default(), &pv)
        .await
        .expect("failed to create static test PV");

    STORAGE_CLASS_NAME.to_string()
}

const PIPELINE_YAML: &str = r#"
stages: [build, test]
jobs:
  build:
    stage: build
    image: busybox:1.36
    cache: [scratch]
    script:
      - "mkdir -p /ferrisgit-cache/scratch"
      - "echo cached-value > /ferrisgit-cache/scratch/marker"
  verify:
    stage: test
    image: busybox:1.36
    needs: [build]
    cache: [scratch]
    script:
      - "cat /ferrisgit-cache/scratch/marker"
"#;

#[sqlx::test(migrations = "../../migrations")]
async fn a_two_stage_pipeline_with_a_shared_cache_runs_to_success_entirely_on_kubernetes(
    pool: sqlx::PgPool,
) {
    let (repository_id, owner_id) = common::insert_repository(&pool).await;
    let pipelines = Arc::new(PostgresPipelineStore::new(pool.clone()));
    let jobs = Arc::new(PostgresJobStore::new(pool.clone()));
    let system_settings = Arc::new(PostgresSystemSettingsStore::new(pool.clone()));
    let events = Arc::new(common::FakeEvents);

    let client = test_client().await;
    let ns = TestNamespace::create(&client).await;
    let cache_pvc_name = pvc_name(repository_id, "scratch");
    let storage_class = provision_static_rwx_storage(&client, &ns.name, &cache_pvc_name).await;
    system_settings
        .update(SystemSettingsUpdate {
            execution_engine: Some(ExecutionEngine::Kubernetes),
            k8s_namespace: Some(Some(ns.name.clone())),
            k8s_cache_storage_class: Some(Some(storage_class)),
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

    let report = Arc::new(ReportJobResultUseCase::new(
        jobs.clone(),
        pipelines.clone(),
        events.clone(),
        job_execution.clone(),
    ));
    let append_logs = Arc::new(AppendJobLogsUseCase::new(jobs.clone()));
    let watcher = Arc::new(PodWatcher::new(
        client.clone(),
        ns.name.clone(),
        report,
        append_logs,
        jobs.clone(),
        pipelines.clone(),
        Arc::new(PostgresRepositoryStore::new(pool.clone())),
        Arc::new(PostgresUserRepository::new(pool.clone())),
        Arc::new(PostgresNotificationStore::new(pool.clone())),
        Arc::new(common::FakeWebhooks),
    ));
    let watcher_handle = tokio::spawn({
        let watcher = watcher.clone();
        async move {
            watcher.run().await;
        }
    });

    let create_pipeline = CreatePipelineUseCase::new(
        Arc::new(common::FakeRepositorySettings),
        system_settings,
        pipelines.clone(),
        jobs.clone(),
        Arc::new(common::FakeFileReader(PIPELINE_YAML.to_string())),
        events,
        job_execution,
        Arc::new(PostgresRepositoryStore::new(pool.clone())),
        Arc::new(PostgresUserRepository::new(pool.clone())),
        Arc::new(PostgresNotificationStore::new(pool.clone())),
        Arc::new(common::FakeWebhooks),
    );
    let pipeline = create_pipeline
        .execute(repository_id, "unused-disk-path", "deadbeef", owner_id)
        .await
        .unwrap()
        .expect("CI is enabled, so a pipeline must be created");

    let result = timeout(Duration::from_secs(90), async {
        loop {
            let current = pipelines.find_by_id(pipeline.id).await.unwrap().unwrap();
            if current.status != PipelineStatus::Running
                && current.status != PipelineStatus::Pending
            {
                return current.status;
            }
            sleep(Duration::from_secs(1)).await;
        }
    })
    .await
    .expect("pipeline did not reach a terminal status within 90s");
    watcher_handle.abort();

    assert_eq!(result, PipelineStatus::Success, "both jobs should succeed");
    let verify_job = jobs
        .list_for_pipeline(pipeline.id)
        .await
        .unwrap()
        .into_iter()
        .find(|j| j.name == "verify")
        .unwrap();
    assert_eq!(verify_job.status, JobStatus::Success);
    assert!(
        verify_job.logs.contains("cached-value"),
        "the verify job's own Pod must be able to read what the build job's Pod wrote to the SAME cache PVC — proving the cache is genuinely shared, not per-Pod"
    );
}
