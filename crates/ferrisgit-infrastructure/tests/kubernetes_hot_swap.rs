use async_trait::async_trait;
use ferrisgit_application::job_execution_resolver::JobExecutionResolver;
use ferrisgit_application::use_cases::cancel_pipeline::CancelPipelineUseCase;
use ferrisgit_application::use_cases::create_pipeline::CreatePipelineUseCase;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::{JobStatus, JobStorePort};
use ferrisgit_domain::job_execution::JobExecutionPort;
use ferrisgit_domain::pipeline::{PipelineStatus, PipelineStorePort};
use ferrisgit_domain::pipeline_events::{JobEvent, PipelineEvent, PipelineEventPublisherPort};
use ferrisgit_domain::pipeline_file_reader::PipelineFileReaderPort;
use ferrisgit_domain::settings::{
    ExecutionEngine, RepositorySettings, RepositorySettingsStorePort, SystemSettingsStorePort,
    SystemSettingsUpdate,
};
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use ferrisgit_domain::webhook_event::WebhookEvent;
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
use uuid::Uuid;

struct FakeRepositorySettings;
#[async_trait]
impl RepositorySettingsStorePort for FakeRepositorySettings {
    async fn get_or_create_default(
        &self,
        repository_id: Uuid,
    ) -> Result<RepositorySettings, DomainError> {
        Ok(RepositorySettings {
            repository_id,
            pipeline_file_path: ".ferrisgit-ci.yml".to_string(),
            ci_enabled: true,
            required_approvals: 0,
        })
    }
    async fn update(
        &self,
        _repository_id: Uuid,
        _update: ferrisgit_domain::settings::RepositorySettingsUpdate,
    ) -> Result<RepositorySettings, DomainError> {
        unimplemented!()
    }
    async fn list_ci_variables(
        &self,
        _repository_id: Uuid,
    ) -> Result<Vec<ferrisgit_domain::settings::CiVariable>, DomainError> {
        unimplemented!()
    }
    async fn set_ci_variable(
        &self,
        _new_variable: ferrisgit_domain::settings::NewCiVariable,
    ) -> Result<ferrisgit_domain::settings::CiVariable, DomainError> {
        unimplemented!()
    }
    async fn delete_ci_variable(&self, _id: Uuid, _repository_id: Uuid) -> Result<(), DomainError> {
        unimplemented!()
    }
    async fn resolve_ci_variables_plaintext(
        &self,
        _repository_id: Uuid,
    ) -> Result<std::collections::BTreeMap<String, String>, DomainError> {
        unimplemented!()
    }
}

struct FakeFileReader(String);
#[async_trait]
impl PipelineFileReaderPort for FakeFileReader {
    async fn read_file_at_revision(
        &self,
        _repository_disk_path: &str,
        _commit_sha: &str,
        _path: &str,
    ) -> Result<Option<Vec<u8>>, DomainError> {
        Ok(Some(self.0.clone().into_bytes()))
    }
}

#[derive(Default)]
struct FakeEvents;
#[async_trait]
impl PipelineEventPublisherPort for FakeEvents {
    async fn publish_pipeline_event(
        &self,
        _pipeline_id: Uuid,
        _event: PipelineEvent,
    ) -> Result<(), DomainError> {
        Ok(())
    }
    async fn publish_job_event(&self, _job_id: Uuid, _event: JobEvent) -> Result<(), DomainError> {
        Ok(())
    }
}

struct FakeWebhooks;
#[async_trait]
impl WebhookDispatcherPort for FakeWebhooks {
    async fn dispatch(
        &self,
        _repository_id: Uuid,
        _event: WebhookEvent,
    ) -> Result<(), DomainError> {
        Ok(())
    }
}

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
    let repository_id = Uuid::new_v4();
    // Inserted directly: the foreign keys into `repositories`/`users` are plumbing this test isn't proving.
    let owner_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, username, email, password_hash) VALUES ($1, $2, $3, $4)")
        .bind(owner_id)
        .bind(format!("owner-{owner_id}"))
        .bind(format!("owner-{owner_id}@example.com"))
        .bind("not-a-real-hash")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO repositories (id, owner_id, name, disk_path) VALUES ($1, $2, $3, $4)")
        .bind(repository_id)
        .bind(owner_id)
        .bind(format!("repo-{repository_id}"))
        .bind("unused-disk-path")
        .execute(&pool)
        .await
        .unwrap();

    let pipelines = Arc::new(PostgresPipelineStore::new(pool.clone()));
    let jobs = Arc::new(PostgresJobStore::new(pool.clone()));
    let system_settings = Arc::new(PostgresSystemSettingsStore::new(pool.clone()));
    let events = Arc::new(FakeEvents);

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
        Arc::new(FakeRepositorySettings),
        system_settings.clone(),
        pipelines.clone(),
        jobs.clone(),
        Arc::new(FakeFileReader(LONG_RUNNING_PIPELINE_YAML.to_string())),
        events.clone(),
        job_execution.clone(),
        Arc::new(PostgresRepositoryStore::new(pool.clone())),
        Arc::new(PostgresUserRepository::new(pool.clone())),
        Arc::new(PostgresNotificationStore::new(pool.clone())),
        Arc::new(FakeWebhooks),
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

    // The setting flips after the pipeline was tagged `kubernetes`; the pipeline keeps its original engine.
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
