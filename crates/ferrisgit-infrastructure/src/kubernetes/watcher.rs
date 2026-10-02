use crate::kubernetes::pod_spec::JOB_ID_LABEL;
use ferrisgit_application::use_cases::report_job_result::{
    AppendJobLogsUseCase, ReportJobResultUseCase,
};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::{JobStatus, JobStorePort};
use ferrisgit_domain::notification::NotificationStorePort;
use ferrisgit_domain::pipeline::PipelineStorePort;
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::user::UserRepositoryPort;
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::api::{Api, DeleteParams, LogParams};
use kube::runtime::{WatchStreamExt, watcher};
use std::sync::Arc;
use uuid::Uuid;

pub struct PodWatcher {
    client: kube::Client,
    namespace: String,
    report: Arc<ReportJobResultUseCase>,
    append_logs: Arc<AppendJobLogsUseCase>,
    jobs: Arc<dyn JobStorePort>,
    pipelines: Arc<dyn PipelineStorePort>,
    repositories: Arc<dyn RepositoryStorePort>,
    users: Arc<dyn UserRepositoryPort>,
    notifications: Arc<dyn NotificationStorePort>,
    webhooks: Arc<dyn WebhookDispatcherPort>,
}

impl PodWatcher {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client: kube::Client,
        namespace: String,
        report: Arc<ReportJobResultUseCase>,
        append_logs: Arc<AppendJobLogsUseCase>,
        jobs: Arc<dyn JobStorePort>,
        pipelines: Arc<dyn PipelineStorePort>,
        repositories: Arc<dyn RepositoryStorePort>,
        users: Arc<dyn UserRepositoryPort>,
        notifications: Arc<dyn NotificationStorePort>,
        webhooks: Arc<dyn WebhookDispatcherPort>,
    ) -> Self {
        Self {
            client,
            namespace,
            report,
            append_logs,
            jobs,
            pipelines,
            repositories,
            users,
            notifications,
            webhooks,
        }
    }

    pub async fn run(&self) {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);
        let config = watcher::Config::default().labels(JOB_ID_LABEL);
        let mut stream = watcher(pods.clone(), config)
            .default_backoff()
            .applied_objects()
            .boxed();
        while let Some(event) = stream.next().await {
            match event {
                Ok(pod) => {
                    if let Err(err) = self.handle_pod(&pods, pod).await {
                        tracing::error!(error = %err, "failed to process a watched Kubernetes Pod event");
                    }
                }
                Err(err) => tracing::error!(error = %err, "Kubernetes watch stream error"),
            }
        }
    }

    async fn handle_pod(&self, pods: &Api<Pod>, pod: Pod) -> Result<(), DomainError> {
        let Some(phase) = pod.status.as_ref().and_then(|s| s.phase.as_deref()) else {
            return Ok(());
        };
        if phase != "Succeeded" && phase != "Failed" {
            return Ok(());
        }
        let Some(job_id) = pod
            .metadata
            .labels
            .as_ref()
            .and_then(|l| l.get(JOB_ID_LABEL))
            .and_then(|v| Uuid::parse_str(v).ok())
        else {
            return Ok(());
        };
        let name = pod.metadata.name.clone().unwrap_or_default();
        let pipeline_id = self
            .jobs
            .find_by_id(job_id)
            .await
            .ok()
            .flatten()
            .map(|j| j.pipeline_id);

        match pods.logs(&name, &LogParams::default()).await {
            Ok(logs) => self.append_logs.execute(job_id, &logs).await?,
            Err(err) => {
                tracing::warn!(error = %err, job_id = %job_id, pod = %name, "failed to fetch logs from a terminal Pod; the job's logs may be incomplete")
            }
        }

        let status = if phase == "Succeeded" {
            JobStatus::Success
        } else {
            JobStatus::Failed
        };
        let triggered_by = self.report.execute(job_id, status).await?;
        if let Some(pipeline_id) = pipeline_id {
            ferrisgit_application::use_cases::report_job_result::notify_pipeline_failure(
                triggered_by,
                pipeline_id,
                &self.pipelines,
                &self.repositories,
                &self.users,
                &self.notifications,
                &self.webhooks,
            )
            .await;
        }

        if let Err(err) = pods.delete(&name, &DeleteParams::default()).await {
            tracing::warn!(error = %err, job_id = %job_id, pod = %name, "failed to delete a terminal Pod; it may be left behind in the cluster");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kubernetes::job_execution_test_doubles::*;
    use crate::kubernetes::pod_spec::{build_pod, pod_name};
    use crate::kubernetes::test_support::{TestNamespace, test_client};
    use async_trait::async_trait;
    use chrono::Utc;
    use ferrisgit_application::job_execution_resolver::JobExecutionResolver;
    use ferrisgit_domain::job::Job;
    use ferrisgit_domain::job_execution::JobExecutionPort;
    use ferrisgit_domain::pipeline::{Pipeline, PipelineStatus};
    use ferrisgit_domain::settings::ExecutionEngine;
    use ferrisgit_domain::webhook_event::WebhookEvent;
    use std::collections::BTreeMap;
    use std::time::Duration;
    use tokio::time::{sleep, timeout};

    /// Never called here (single-job pipelines, `list_runnable` returns nothing). It only satisfies the constructor.
    #[derive(Default)]
    struct NoopExecution;
    #[async_trait]
    impl JobExecutionPort for NoopExecution {
        async fn submit(&self, _job: &Job) -> Result<(), DomainError> {
            Ok(())
        }
        async fn cancel(&self, _job: &Job) -> Result<(), DomainError> {
            Ok(())
        }
    }

    fn job_execution_resolver() -> Arc<JobExecutionResolver> {
        let noop = Arc::new(NoopExecution);
        Arc::new(JobExecutionResolver::new(noop.clone(), noop))
    }

    fn job(script: Vec<String>) -> Job {
        Job {
            id: Uuid::new_v4(),
            pipeline_id: Uuid::new_v4(),
            stage: "build".to_string(),
            name: "compile".to_string(),
            image: "busybox:1.36".to_string(),
            script,
            variables: BTreeMap::new(),
            needs: vec![],
            tags: vec![],
            status: JobStatus::Running,
            runner_id: None,
            logs: String::new(),
            created_at: Utc::now(),
            cache: vec![],
            started_at: None,
            finished_at: None,
        }
    }

    fn pipeline_for(the_job: &Job) -> Pipeline {
        Pipeline {
            id: the_job.pipeline_id,
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

    struct FakeRepositories(ferrisgit_domain::repository::Repository);
    #[async_trait]
    impl ferrisgit_domain::repository::RepositoryStorePort for FakeRepositories {
        async fn create(
            &self,
            _new_repo: ferrisgit_domain::repository::NewRepository,
            _disk_path: String,
        ) -> Result<ferrisgit_domain::repository::Repository, DomainError> {
            unimplemented!()
        }
        async fn list_for_owner(
            &self,
            _owner_id: Uuid,
        ) -> Result<Vec<ferrisgit_domain::repository::Repository>, DomainError> {
            unimplemented!()
        }
        async fn find_by_owner_and_name(
            &self,
            _owner_id: Uuid,
            _name: &str,
        ) -> Result<Option<ferrisgit_domain::repository::Repository>, DomainError> {
            unimplemented!()
        }
        async fn find_by_group_and_name(
            &self,
            _group_id: Uuid,
            _name: &str,
        ) -> Result<Option<ferrisgit_domain::repository::Repository>, DomainError> {
            unimplemented!()
        }
        async fn find_by_id(
            &self,
            _id: Uuid,
        ) -> Result<Option<ferrisgit_domain::repository::Repository>, DomainError> {
            Ok(Some(self.0.clone()))
        }
        async fn list_for_group(
            &self,
            _group_id: Uuid,
        ) -> Result<Vec<ferrisgit_domain::repository::Repository>, DomainError> {
            unimplemented!()
        }
    }

    struct FakeUsers(ferrisgit_domain::user::User);
    #[async_trait]
    impl ferrisgit_domain::user::UserRepositoryPort for FakeUsers {
        async fn find_by_username(
            &self,
            _username: &str,
        ) -> Result<Option<ferrisgit_domain::user::User>, DomainError> {
            unimplemented!()
        }
        async fn find_by_id(
            &self,
            _id: Uuid,
        ) -> Result<Option<ferrisgit_domain::user::User>, DomainError> {
            Ok(Some(self.0.clone()))
        }
        async fn create(
            &self,
            _new_user: ferrisgit_domain::user::NewUser,
        ) -> Result<ferrisgit_domain::user::User, DomainError> {
            unimplemented!()
        }
        async fn count(&self) -> Result<i64, DomainError> {
            unimplemented!()
        }
        async fn update_email(
            &self,
            _user_id: Uuid,
            _email: String,
        ) -> Result<ferrisgit_domain::user::User, DomainError> {
            unimplemented!()
        }
        async fn update_password_hash(
            &self,
            _user_id: Uuid,
            _password_hash: String,
        ) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn count_admins(&self) -> Result<i64, DomainError> {
            unimplemented!()
        }
        async fn set_admin(&self, _user_id: Uuid, _is_admin: bool) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn delete(
            &self,
            _user_id: Uuid,
            _heir_id: Uuid,
        ) -> Result<Vec<ferrisgit_domain::repository::Repository>, DomainError> {
            unimplemented!()
        }
        async fn search(
            &self,
            _query: &str,
            _limit: i64,
        ) -> Result<Vec<ferrisgit_domain::user::User>, DomainError> {
            unimplemented!()
        }
        async fn find_by_username_ignore_case(
            &self,
            _username: &str,
        ) -> Result<Option<ferrisgit_domain::user::User>, DomainError> {
            Ok(None)
        }
        async fn find_by_email_ignore_case(
            &self,
            _email: &str,
        ) -> Result<Option<ferrisgit_domain::user::User>, DomainError> {
            Ok(None)
        }
    }

    struct FakeNotifications;
    #[async_trait]
    impl ferrisgit_domain::notification::NotificationStorePort for FakeNotifications {
        async fn create(
            &self,
            _notification: ferrisgit_domain::notification::NewNotification,
        ) -> Result<(), DomainError> {
            Ok(())
        }
        async fn list_for_recipient(
            &self,
            _recipient_id: Uuid,
            _limit: i64,
        ) -> Result<Vec<ferrisgit_domain::notification::Notification>, DomainError> {
            unimplemented!()
        }
        async fn unread_count(&self, _recipient_id: Uuid) -> Result<i64, DomainError> {
            unimplemented!()
        }
        async fn mark_read(
            &self,
            _notification_id: Uuid,
            _recipient_id: Uuid,
        ) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn mark_all_read(&self, _recipient_id: Uuid) -> Result<(), DomainError> {
            unimplemented!()
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

    fn repository(id: Uuid) -> ferrisgit_domain::repository::Repository {
        ferrisgit_domain::repository::Repository {
            id,
            owner_id: Uuid::new_v4(),
            name: "hello".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "hello.git".to_string(),
            visibility: ferrisgit_domain::repository::RepositoryVisibility::Private,
            created_at: Utc::now(),
        }
    }

    fn user(id: Uuid) -> ferrisgit_domain::user::User {
        ferrisgit_domain::user::User {
            id,
            username: "florian".to_string(),
            email: "florian@example.com".to_string(),
            password_hash: "hash".to_string(),
            is_admin: false,
            created_at: Utc::now(),
        }
    }

    async fn wait_for_terminal_phase(pods: &Api<Pod>, name: &str) -> Pod {
        timeout(Duration::from_secs(60), async {
            loop {
                let pod = pods.get(name).await.unwrap();
                if let Some(phase) = pod.status.as_ref().and_then(|s| s.phase.as_deref())
                    && (phase == "Succeeded" || phase == "Failed")
                {
                    return pod;
                }
                sleep(Duration::from_millis(500)).await;
            }
        })
        .await
        .expect("pod did not reach a terminal phase in time")
    }

    #[tokio::test]
    async fn handle_pod_reports_success_captures_logs_and_deletes_a_succeeded_pod() {
        let client = test_client().await;
        let ns = TestNamespace::create(&client).await;
        let the_job = job(vec!["echo hello-from-the-job".to_string()]);
        let jobs = Arc::new(FakeJobs::default());
        jobs.0.lock().unwrap().push(the_job.clone());
        let pipelines = Arc::new(FakePipelines(pipeline_for(&the_job)));
        let events = Arc::new(FakeEvents::default());
        let report = Arc::new(ReportJobResultUseCase::new(
            jobs.clone(),
            pipelines.clone(),
            events,
            job_execution_resolver(),
        ));
        let append_logs = Arc::new(AppendJobLogsUseCase::new(jobs.clone()));
        let watcher = PodWatcher::new(
            client.clone(),
            ns.name.clone(),
            report,
            append_logs,
            jobs.clone(),
            pipelines.clone(),
            Arc::new(FakeRepositories(repository(Uuid::new_v4()))),
            Arc::new(FakeUsers(user(Uuid::new_v4()))),
            Arc::new(FakeNotifications),
            Arc::new(FakeWebhooks),
        );

        let pod = build_pod(&the_job, Uuid::new_v4(), &ns.name);
        let pods: Api<Pod> = Api::namespaced(client, &ns.name);
        pods.create(&kube::api::PostParams::default(), &pod)
            .await
            .unwrap();
        let terminal_pod = wait_for_terminal_phase(&pods, &pod_name(the_job.id)).await;

        watcher.handle_pod(&pods, terminal_pod).await.unwrap();

        assert_eq!(jobs.0.lock().unwrap()[0].status, JobStatus::Success);
        assert!(
            jobs.0.lock().unwrap()[0]
                .logs
                .contains("hello-from-the-job")
        );
        assert!(
            pods.get(&pod_name(the_job.id)).await.is_err(),
            "pod should have been deleted"
        );
    }

    #[tokio::test]
    async fn handle_pod_reports_failure_for_a_failed_pod() {
        let client = test_client().await;
        let ns = TestNamespace::create(&client).await;
        let the_job = job(vec!["exit 1".to_string()]);
        let jobs = Arc::new(FakeJobs::default());
        jobs.0.lock().unwrap().push(the_job.clone());
        let pipelines = Arc::new(FakePipelines(pipeline_for(&the_job)));
        let report = Arc::new(ReportJobResultUseCase::new(
            jobs.clone(),
            pipelines.clone(),
            Arc::new(FakeEvents::default()),
            job_execution_resolver(),
        ));
        let append_logs = Arc::new(AppendJobLogsUseCase::new(jobs.clone()));
        let watcher = PodWatcher::new(
            client.clone(),
            ns.name.clone(),
            report,
            append_logs,
            jobs.clone(),
            pipelines.clone(),
            Arc::new(FakeRepositories(repository(Uuid::new_v4()))),
            Arc::new(FakeUsers(user(Uuid::new_v4()))),
            Arc::new(FakeNotifications),
            Arc::new(FakeWebhooks),
        );

        let pod = build_pod(&the_job, Uuid::new_v4(), &ns.name);
        let pods: Api<Pod> = Api::namespaced(client, &ns.name);
        pods.create(&kube::api::PostParams::default(), &pod)
            .await
            .unwrap();
        let terminal_pod = wait_for_terminal_phase(&pods, &pod_name(the_job.id)).await;

        watcher.handle_pod(&pods, terminal_pod).await.unwrap();

        assert_eq!(jobs.0.lock().unwrap()[0].status, JobStatus::Failed);
    }

    #[tokio::test]
    async fn a_pod_still_pending_or_running_is_left_alone() {
        let client = test_client().await;
        let ns = TestNamespace::create(&client).await;
        let the_job = job(vec!["sleep 30".to_string()]);
        let jobs = Arc::new(FakeJobs::default());
        jobs.0.lock().unwrap().push(the_job.clone());
        let pipelines = Arc::new(FakePipelines(pipeline_for(&the_job)));
        let report = Arc::new(ReportJobResultUseCase::new(
            jobs.clone(),
            pipelines.clone(),
            Arc::new(FakeEvents::default()),
            job_execution_resolver(),
        ));
        let append_logs = Arc::new(AppendJobLogsUseCase::new(jobs.clone()));
        let watcher = PodWatcher::new(
            client.clone(),
            ns.name.clone(),
            report,
            append_logs,
            jobs.clone(),
            pipelines.clone(),
            Arc::new(FakeRepositories(repository(Uuid::new_v4()))),
            Arc::new(FakeUsers(user(Uuid::new_v4()))),
            Arc::new(FakeNotifications),
            Arc::new(FakeWebhooks),
        );

        let pod = build_pod(&the_job, Uuid::new_v4(), &ns.name);
        let pods: Api<Pod> = Api::namespaced(client, &ns.name);
        pods.create(&kube::api::PostParams::default(), &pod)
            .await
            .unwrap();
        let name = pod_name(the_job.id);
        sleep(Duration::from_secs(3)).await;
        let live_pod = pods.get(&name).await.unwrap();

        watcher.handle_pod(&pods, live_pod).await.unwrap();

        assert_eq!(
            jobs.0.lock().unwrap()[0].status,
            JobStatus::Running,
            "must not be touched while the pod is still running"
        );
        assert!(
            pods.get(&name).await.is_ok(),
            "must not delete a pod that hasn't finished"
        );
        pods.delete(&name, &DeleteParams::default()).await.ok();
    }

    #[tokio::test]
    async fn run_drives_a_real_pod_to_a_reported_terminal_status_through_the_actual_watch_stream() {
        let client = test_client().await;
        let ns = TestNamespace::create(&client).await;
        let the_job = job(vec!["true".to_string()]);
        let jobs = Arc::new(FakeJobs::default());
        jobs.0.lock().unwrap().push(the_job.clone());
        let pipelines = Arc::new(FakePipelines(pipeline_for(&the_job)));
        let report = Arc::new(ReportJobResultUseCase::new(
            jobs.clone(),
            pipelines.clone(),
            Arc::new(FakeEvents::default()),
            job_execution_resolver(),
        ));
        let append_logs = Arc::new(AppendJobLogsUseCase::new(jobs.clone()));
        let watcher = Arc::new(PodWatcher::new(
            client.clone(),
            ns.name.clone(),
            report,
            append_logs,
            jobs.clone(),
            pipelines.clone(),
            Arc::new(FakeRepositories(repository(Uuid::new_v4()))),
            Arc::new(FakeUsers(user(Uuid::new_v4()))),
            Arc::new(FakeNotifications),
            Arc::new(FakeWebhooks),
        ));
        let watcher_handle = tokio::spawn({
            let watcher = watcher.clone();
            async move {
                watcher.run().await;
            }
        });

        let pod = build_pod(&the_job, Uuid::new_v4(), &ns.name);
        let pods: Api<Pod> = Api::namespaced(client, &ns.name);
        pods.create(&kube::api::PostParams::default(), &pod)
            .await
            .unwrap();

        let reported = timeout(Duration::from_secs(60), async {
            loop {
                if jobs.0.lock().unwrap()[0].status == JobStatus::Success {
                    return;
                }
                sleep(Duration::from_millis(500)).await;
            }
        })
        .await;

        watcher_handle.abort();
        reported.expect("the watch loop should have reported the job Success within the timeout");
    }
}
