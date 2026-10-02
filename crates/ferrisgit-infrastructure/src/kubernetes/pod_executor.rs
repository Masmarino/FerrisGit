use crate::error::infra;
use crate::kubernetes::{
    cache::ensure_cache_pvcs,
    pod_spec::{build_pod, pod_name},
};
use async_trait::async_trait;
use ferrisgit_application::use_cases::report_job_result::mark_pipeline_running;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::{Job, JobStatus, JobStorePort};
use ferrisgit_domain::job_execution::JobExecutionPort;
use ferrisgit_domain::pipeline::PipelineStorePort;
use ferrisgit_domain::pipeline_events::{JobEvent, PipelineEventPublisherPort};
use ferrisgit_domain::settings::{SystemSettings, SystemSettingsStorePort};
use k8s_openapi::api::core::v1::Pod;
use kube::api::{Api, DeleteParams, PostParams};
use std::sync::Arc;

pub struct KubernetesPodExecutor {
    client: kube::Client,
    pipelines: Arc<dyn PipelineStorePort>,
    jobs: Arc<dyn JobStorePort>,
    system_settings: Arc<dyn SystemSettingsStorePort>,
    events: Arc<dyn PipelineEventPublisherPort>,
    /// Used when `SystemSettings.k8s_namespace` is unset; resolved once at boot from the same cluster
    /// config that produced `client`.
    default_namespace: String,
}

impl KubernetesPodExecutor {
    pub fn new(
        client: kube::Client,
        pipelines: Arc<dyn PipelineStorePort>,
        jobs: Arc<dyn JobStorePort>,
        system_settings: Arc<dyn SystemSettingsStorePort>,
        events: Arc<dyn PipelineEventPublisherPort>,
        default_namespace: String,
    ) -> Self {
        Self {
            client,
            pipelines,
            jobs,
            system_settings,
            events,
            default_namespace,
        }
    }

    fn namespace(&self, settings: &SystemSettings) -> String {
        settings
            .k8s_namespace
            .clone()
            .unwrap_or_else(|| self.default_namespace.clone())
    }
}

#[async_trait]
impl JobExecutionPort for KubernetesPodExecutor {
    async fn submit(&self, job: &Job) -> Result<(), DomainError> {
        let settings = self.system_settings.get().await?;
        let namespace = self.namespace(&settings);
        let pipeline = self
            .pipelines
            .find_by_id(job.pipeline_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("pipeline".to_string()))?;

        if !job.cache.is_empty() {
            let storage_class = settings.k8s_cache_storage_class.ok_or_else(|| {
                DomainError::Validation(
                    "job declares cache: keys but k8s_cache_storage_class is not configured"
                        .to_string(),
                )
            })?;
            ensure_cache_pvcs(
                &self.client,
                &namespace,
                pipeline.repository_id,
                &job.cache,
                &storage_class,
            )
            .await?;
        }

        let pod = build_pod(job, pipeline.repository_id, &namespace);
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &namespace);
        pods.create(&PostParams::default(), &pod)
            .await
            .map_err(infra)?;

        let changed = self.jobs.update_status(job.id, JobStatus::Running).await?;
        if changed {
            self.events
                .publish_job_event(
                    job.id,
                    JobEvent::StatusChanged {
                        status: JobStatus::Running,
                    },
                )
                .await
                .ok();
            mark_pipeline_running(&self.pipelines, &self.events, job.pipeline_id).await?;
        }
        Ok(())
    }

    async fn cancel(&self, job: &Job) -> Result<(), DomainError> {
        let settings = self.system_settings.get().await?;
        let namespace = self.namespace(&settings);
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &namespace);
        match pods
            .delete(&pod_name(job.id), &DeleteParams::default())
            .await
        {
            Ok(_) => Ok(()),
            Err(kube::Error::Api(err)) if err.code == 404 => Ok(()),
            Err(err) => Err(infra(err)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kubernetes::job_execution_test_doubles::*;
    use crate::kubernetes::test_support::{TestNamespace, test_client};
    use chrono::Utc;
    use ferrisgit_domain::pipeline::{Pipeline, PipelineStatus};
    use ferrisgit_domain::settings::{ExecutionEngine, SystemSettings, SystemSettingsUpdate};
    use k8s_openapi::api::core::v1::Pod;
    use kube::Api;
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use std::time::Duration;
    use tokio::time::{sleep, timeout};
    use uuid::Uuid;

    struct FakeSettings(SystemSettings);
    #[async_trait]
    impl SystemSettingsStorePort for FakeSettings {
        async fn get(&self) -> Result<SystemSettings, DomainError> {
            Ok(self.0.clone())
        }
        async fn update(
            &self,
            _update: SystemSettingsUpdate,
        ) -> Result<SystemSettings, DomainError> {
            unimplemented!()
        }
    }

    fn settings(namespace: &str, storage_class: Option<&str>) -> SystemSettings {
        SystemSettings {
            execution_engine: ExecutionEngine::Kubernetes,
            k8s_namespace: Some(namespace.to_string()),
            k8s_cache_storage_class: storage_class.map(|s| s.to_string()),
            runner_registration_token: None,
            log_retention_days: None,
            max_concurrent_jobs: None,
            jwt_ttl_hours: 24,
            max_push_size_mb: 50,
        }
    }

    fn job(cache: Vec<String>) -> Job {
        Job {
            id: Uuid::new_v4(),
            pipeline_id: Uuid::new_v4(),
            stage: "build".to_string(),
            name: "compile".to_string(),
            image: "alpine:3.20".to_string(),
            script: vec!["echo hi".to_string()],
            variables: BTreeMap::new(),
            needs: vec![],
            tags: vec![],
            status: JobStatus::Pending,
            runner_id: None,
            logs: String::new(),
            created_at: Utc::now(),
            cache,
            started_at: None,
            finished_at: None,
        }
    }

    fn pipeline() -> Pipeline {
        Pipeline {
            id: Uuid::new_v4(),
            repository_id: Uuid::new_v4(),
            commit_sha: "abc".to_string(),
            status: PipelineStatus::Running,
            execution_engine: ExecutionEngine::Kubernetes,
            triggered_by: Uuid::new_v4(),
            created_at: Utc::now(),
            finished_at: None,
            error: None,
        }
    }

    async fn wait_for_pod_deletion(pods: &Api<Pod>, name: &str) {
        timeout(Duration::from_secs(30), async {
            loop {
                if pods.get(name).await.is_err() {
                    return;
                }
                sleep(Duration::from_millis(300)).await;
            }
        })
        .await
        .expect("pod did not get deleted in time");
    }

    #[tokio::test]
    async fn submit_creates_a_pod_marks_the_job_running_and_publishes_the_event() {
        let client = test_client().await;
        let ns = TestNamespace::create(&client).await;
        let the_job = job(vec![]);
        let jobs = Arc::new(FakeJobs(Mutex::new(vec![the_job.clone()])));
        let events = Arc::new(FakeEvents::default());
        let executor = KubernetesPodExecutor::new(
            client.clone(),
            Arc::new(FakePipelines(pipeline())),
            jobs.clone(),
            Arc::new(FakeSettings(settings(&ns.name, None))),
            events.clone(),
            ns.name.clone(),
        );

        executor.submit(&the_job).await.unwrap();

        let pods: Api<Pod> = Api::namespaced(client, &ns.name);
        assert!(pods.get(&pod_name(the_job.id)).await.is_ok());
        assert_eq!(jobs.0.lock().unwrap()[0].status, JobStatus::Running);
        assert!(
            events
                .0
                .lock()
                .unwrap()
                .iter()
                .any(|e| e.starts_with(&the_job.id.to_string()))
        );
    }

    #[tokio::test]
    async fn submit_falls_back_to_the_constructor_supplied_default_namespace_when_k8s_namespace_is_unset()
     {
        let client = test_client().await;
        let ns = TestNamespace::create(&client).await;
        let the_job = job(vec![]);
        let jobs = Arc::new(FakeJobs(Mutex::new(vec![the_job.clone()])));
        let settings_without_namespace = SystemSettings {
            execution_engine: ExecutionEngine::Kubernetes,
            k8s_namespace: None,
            k8s_cache_storage_class: None,
            runner_registration_token: None,
            log_retention_days: None,
            max_concurrent_jobs: None,
            jwt_ttl_hours: 24,
            max_push_size_mb: 50,
        };
        let executor = KubernetesPodExecutor::new(
            client.clone(),
            Arc::new(FakePipelines(pipeline())),
            jobs.clone(),
            Arc::new(FakeSettings(settings_without_namespace)),
            Arc::new(FakeEvents::default()),
            ns.name.clone(),
        );

        executor.submit(&the_job).await.unwrap();

        let pods: Api<Pod> = Api::namespaced(client, &ns.name);
        assert!(
            pods.get(&pod_name(the_job.id)).await.is_ok(),
            "the Pod must land in the constructor's default_namespace when the setting is unset"
        );
    }

    #[tokio::test]
    async fn submit_does_not_publish_a_running_event_when_the_job_already_went_terminal() {
        // A fast watcher report can make the job terminal before `submit` marks it `Running`. Then
        // `update_status` returns `false`, and `submit` must not publish a stale `Running` event.
        let client = test_client().await;
        let ns = TestNamespace::create(&client).await;
        let the_job = job(vec![]);
        let mut already_terminal = the_job.clone();
        already_terminal.status = JobStatus::Success;
        let jobs = Arc::new(FakeJobs(Mutex::new(vec![already_terminal])));
        let events = Arc::new(FakeEvents::default());
        let executor = KubernetesPodExecutor::new(
            client.clone(),
            Arc::new(FakePipelines(pipeline())),
            jobs.clone(),
            Arc::new(FakeSettings(settings(&ns.name, None))),
            events.clone(),
            ns.name.clone(),
        );

        executor.submit(&the_job).await.unwrap();

        assert_eq!(
            jobs.0.lock().unwrap()[0].status,
            JobStatus::Success,
            "a terminal status must never be overwritten back to Running"
        );
        assert!(
            events.0.lock().unwrap().is_empty(),
            "no Running event should be published once the job is already terminal"
        );
    }

    #[tokio::test]
    async fn submit_provisions_cache_pvcs_before_creating_the_pod() {
        let client = test_client().await;
        let ns = TestNamespace::create(&client).await;
        let pipeline = pipeline();
        let the_job = job(vec!["cargo-registry".to_string()]);
        let jobs = Arc::new(FakeJobs(Mutex::new(vec![the_job.clone()])));
        let executor = KubernetesPodExecutor::new(
            client.clone(),
            Arc::new(FakePipelines(pipeline.clone())),
            jobs,
            Arc::new(FakeSettings(settings(&ns.name, Some("standard")))),
            Arc::new(FakeEvents::default()),
            ns.name.clone(),
        );

        executor.submit(&the_job).await.unwrap();

        use crate::kubernetes::cache::pvc_name;
        use k8s_openapi::api::core::v1::PersistentVolumeClaim;
        let pvcs: Api<PersistentVolumeClaim> = Api::namespaced(client, &ns.name);
        assert!(
            pvcs.get(&pvc_name(pipeline.repository_id, "cargo-registry"))
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn submit_fails_loudly_when_cache_is_declared_but_no_storage_class_is_configured() {
        let client = test_client().await;
        let ns = TestNamespace::create(&client).await;
        let the_job = job(vec!["cargo-registry".to_string()]);
        let jobs = Arc::new(FakeJobs(Mutex::new(vec![the_job.clone()])));
        let executor = KubernetesPodExecutor::new(
            client.clone(),
            Arc::new(FakePipelines(pipeline())),
            jobs.clone(),
            Arc::new(FakeSettings(settings(&ns.name, None))),
            Arc::new(FakeEvents::default()),
            ns.name.clone(),
        );

        let result = executor.submit(&the_job).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
        assert_eq!(
            jobs.0.lock().unwrap()[0].status,
            JobStatus::Pending,
            "must not be marked Running if the Pod was never created"
        );
    }

    #[tokio::test]
    async fn cancel_deletes_the_pods_pod() {
        let client = test_client().await;
        let ns = TestNamespace::create(&client).await;
        let the_job = job(vec![]);
        let jobs = Arc::new(FakeJobs(Mutex::new(vec![the_job.clone()])));
        let executor = KubernetesPodExecutor::new(
            client.clone(),
            Arc::new(FakePipelines(pipeline())),
            jobs,
            Arc::new(FakeSettings(settings(&ns.name, None))),
            Arc::new(FakeEvents::default()),
            ns.name.clone(),
        );
        executor.submit(&the_job).await.unwrap();

        executor.cancel(&the_job).await.unwrap();

        let pods: Api<Pod> = Api::namespaced(client, &ns.name);
        wait_for_pod_deletion(&pods, &pod_name(the_job.id)).await;
    }

    #[tokio::test]
    async fn cancel_on_a_job_whose_pod_was_never_created_does_not_error() {
        let client = test_client().await;
        let ns = TestNamespace::create(&client).await;
        let the_job = job(vec![]);
        let executor = KubernetesPodExecutor::new(
            client,
            Arc::new(FakePipelines(pipeline())),
            Arc::new(FakeJobs::default()),
            Arc::new(FakeSettings(settings(&ns.name, None))),
            Arc::new(FakeEvents::default()),
            ns.name.clone(),
        );

        executor.cancel(&the_job).await.unwrap();
    }
}
