use super::*;
use crate::test_support::{
    FakeEvents, FakeExecution, FakeJobs, FakeNotifications, FakePipelines, FakeRepositories,
    FakeUsers, FakeWebhooks,
};
use crate::use_cases::fixtures;
use async_trait::async_trait;
use chrono::Utc;
use ferrisgit_domain::job::Job;
use ferrisgit_domain::job_execution::JobExecutionPort;
use ferrisgit_domain::pipeline::Pipeline;
use ferrisgit_domain::settings::ExecutionEngine;
use std::collections::BTreeMap;

struct FailingExecution;
#[async_trait]
impl JobExecutionPort for FailingExecution {
    async fn submit(&self, _job: &Job) -> Result<(), DomainError> {
        Err(DomainError::Infrastructure("simulated failure".to_string()))
    }
    async fn cancel(&self, _job: &Job) -> Result<(), DomainError> {
        unimplemented!()
    }
}

fn job(pipeline_id: Uuid, status: JobStatus) -> Job {
    Job {
        id: Uuid::new_v4(),
        pipeline_id,
        stage: "build".to_string(),
        name: "compile".to_string(),
        image: "rust".to_string(),
        script: vec![],
        variables: BTreeMap::new(),
        needs: vec![],
        tags: vec![],
        cache: vec![],
        status,
        runner_id: None,
        logs: String::new(),
        created_at: Utc::now(),
        started_at: None,
        finished_at: None,
    }
}

fn named_job(pipeline_id: Uuid, name: &str, needs: &[&str], status: JobStatus) -> Job {
    Job {
        name: name.to_string(),
        needs: needs.iter().map(|n| n.to_string()).collect(),
        ..job(pipeline_id, status)
    }
}

fn staged_job(
    pipeline_id: Uuid,
    stage: &str,
    name: &str,
    needs: &[&str],
    status: JobStatus,
) -> Job {
    Job {
        stage: stage.to_string(),
        ..named_job(pipeline_id, name, needs, status)
    }
}

fn pipeline(pipeline_id: Uuid) -> Pipeline {
    Pipeline {
        id: pipeline_id,
        repository_id: Uuid::new_v4(),
        commit_sha: "abc".to_string(),
        execution_engine: ExecutionEngine::DockerRunners,
        status: PipelineStatus::Running,
        triggered_by: Uuid::new_v4(),
        created_at: Utc::now(),
        finished_at: None,
        error: None,
    }
}

struct Harness {
    jobs: Arc<FakeJobs>,
    pipelines: Arc<FakePipelines>,
    events: Arc<FakeEvents>,
    execution: Arc<FakeExecution>,
    use_case: ReportJobResultUseCase,
}

/// A running pipeline with `jobs`. Both engines are the same recording fake.
fn harness(pipeline_id: Uuid, jobs: Vec<Job>) -> Harness {
    harness_over(pipeline(pipeline_id), jobs)
}

fn harness_over(pipeline: Pipeline, jobs: Vec<Job>) -> Harness {
    let jobs = Arc::new(FakeJobs::new(jobs));
    let pipelines = Arc::new(FakePipelines::new(vec![pipeline]));
    let events = Arc::new(FakeEvents::new());
    let execution = Arc::new(FakeExecution::new());
    let use_case = ReportJobResultUseCase::new(
        jobs.clone(),
        pipelines.clone(),
        events.clone(),
        Arc::new(JobExecutionResolver::new(
            execution.clone(),
            execution.clone(),
        )),
    );
    Harness {
        jobs,
        pipelines,
        events,
        execution,
        use_case,
    }
}

fn status_of(jobs: &FakeJobs, id: Uuid) -> JobStatus {
    jobs.get(id).unwrap().status
}

#[tokio::test]
async fn append_job_logs_appends_to_the_existing_log_text() {
    let existing = job(Uuid::new_v4(), JobStatus::Running);
    let jobs = Arc::new(FakeJobs::new(vec![existing.clone()]));
    let use_case = AppendJobLogsUseCase::new(jobs.clone());

    use_case.execute(existing.id, "line one\n").await.unwrap();
    use_case.execute(existing.id, "line two\n").await.unwrap();

    assert_eq!(jobs.snapshot()[0].logs, "line one\nline two\n");
}

#[tokio::test]
async fn reporting_the_only_jobs_success_marks_the_pipeline_success() {
    let pipeline_id = Uuid::new_v4();
    let the_job = job(pipeline_id, JobStatus::Running);
    let h = harness(pipeline_id, vec![the_job.clone()]);

    h.use_case
        .execute(the_job.id, JobStatus::Success)
        .await
        .unwrap();

    assert_eq!(h.pipelines.snapshot()[0].status, PipelineStatus::Success);
    assert!(
        h.events
            .pipeline_events()
            .iter()
            .any(|(_, e)| e.event_type() == "PipelineStatusChanged")
    );
}

#[tokio::test]
async fn one_failed_job_marks_the_whole_pipeline_failed_even_if_others_succeeded() {
    let pipeline_id = Uuid::new_v4();
    let succeeded = job(pipeline_id, JobStatus::Success);
    let still_running = job(pipeline_id, JobStatus::Running);
    let h = harness(pipeline_id, vec![succeeded, still_running.clone()]);

    let notify = h
        .use_case
        .execute(still_running.id, JobStatus::Failed)
        .await
        .unwrap();

    assert_eq!(h.pipelines.snapshot()[0].status, PipelineStatus::Failed);
    assert_eq!(
        notify,
        Some(h.pipelines.snapshot()[0].triggered_by),
        "resolving the pipeline to Failed must report its pusher for notification"
    );
}

#[tokio::test]
async fn a_job_finishing_while_a_sibling_is_still_running_does_not_touch_the_pipeline() {
    let pipeline_id = Uuid::new_v4();
    let finished = job(pipeline_id, JobStatus::Running);
    let still_running = job(pipeline_id, JobStatus::Running);
    let h = harness(pipeline_id, vec![finished.clone(), still_running]);

    h.use_case
        .execute(finished.id, JobStatus::Success)
        .await
        .unwrap();

    assert_eq!(
        h.pipelines.snapshot()[0].status,
        PipelineStatus::Running,
        "pipeline must stay Running while a sibling job is unfinished"
    );
    assert!(h.events.pipeline_events().is_empty());
}

#[tokio::test]
async fn a_failed_job_beats_a_canceled_sibling_for_pipeline_status() {
    let pipeline_id = Uuid::new_v4();
    let canceled = job(pipeline_id, JobStatus::Canceled);
    let succeeded = job(pipeline_id, JobStatus::Success);
    let still_running = job(pipeline_id, JobStatus::Running);
    let h = harness(
        pipeline_id,
        vec![canceled, succeeded, still_running.clone()],
    );

    h.use_case
        .execute(still_running.id, JobStatus::Failed)
        .await
        .unwrap();

    assert_eq!(
        h.pipelines.snapshot()[0].status,
        PipelineStatus::Failed,
        "Failed must win over Canceled even when both are present"
    );
}

#[tokio::test]
async fn a_pipeline_with_every_job_canceled_and_no_failures_is_marked_canceled() {
    let pipeline_id = Uuid::new_v4();
    let already_canceled = job(pipeline_id, JobStatus::Canceled);
    let still_running = job(pipeline_id, JobStatus::Running);
    let h = harness(pipeline_id, vec![already_canceled, still_running.clone()]);

    h.use_case
        .execute(still_running.id, JobStatus::Canceled)
        .await
        .unwrap();

    assert_eq!(h.pipelines.snapshot()[0].status, PipelineStatus::Canceled);
}

#[tokio::test]
async fn a_failed_job_transitively_skips_everything_downstream_and_terminates_the_pipeline() {
    let pipeline_id = Uuid::new_v4();
    let build = named_job(pipeline_id, "build", &[], JobStatus::Running);
    let test = named_job(pipeline_id, "test", &["build"], JobStatus::Pending);
    let deploy = named_job(pipeline_id, "deploy", &["test"], JobStatus::Pending);
    let h = harness(
        pipeline_id,
        vec![build.clone(), test.clone(), deploy.clone()],
    );

    let notify = h
        .use_case
        .execute(build.id, JobStatus::Failed)
        .await
        .unwrap();
    assert_eq!(notify, Some(h.pipelines.snapshot()[0].triggered_by));

    assert_eq!(status_of(&h.jobs, build.id), JobStatus::Failed);
    assert_eq!(
        status_of(&h.jobs, test.id),
        JobStatus::Skipped,
        "a job needing the failed job can never be claimed and must be skipped"
    );
    assert_eq!(
        status_of(&h.jobs, deploy.id),
        JobStatus::Skipped,
        "skipping must propagate transitively through the needs chain, not just one hop"
    );
    assert_eq!(
        h.pipelines.snapshot()[0].status,
        PipelineStatus::Failed,
        "the pipeline must now reach a terminal status instead of hanging forever"
    );
    assert_eq!(
        h.events.job_events().len(),
        3,
        "one job event per status change, including the transitively skipped ones"
    );
}

#[tokio::test]
async fn a_failed_job_does_not_cancel_an_independent_parallel_job() {
    let pipeline_id = Uuid::new_v4();
    let build = named_job(pipeline_id, "build", &[], JobStatus::Running);
    let lint = named_job(pipeline_id, "lint", &[], JobStatus::Pending);
    let h = harness(pipeline_id, vec![build.clone(), lint.clone()]);

    h.use_case
        .execute(build.id, JobStatus::Failed)
        .await
        .unwrap();

    assert_eq!(
        status_of(&h.jobs, lint.id),
        JobStatus::Pending,
        "a job that needs nothing is still perfectly claimable"
    );
    assert_eq!(
        h.pipelines.snapshot()[0].status,
        PipelineStatus::Running,
        "the pipeline stays open while an independent job can still run"
    );
}

#[tokio::test]
async fn a_late_report_cannot_resurrect_an_already_canceled_job_or_its_pipeline() {
    let pipeline_id = Uuid::new_v4();
    let canceled = job(pipeline_id, JobStatus::Canceled);
    let h = harness_over(
        Pipeline {
            status: PipelineStatus::Canceled,
            ..pipeline(pipeline_id)
        },
        vec![canceled.clone()],
    );

    h.use_case
        .execute(canceled.id, JobStatus::Success)
        .await
        .unwrap();

    assert_eq!(
        h.jobs.snapshot()[0].status,
        JobStatus::Canceled,
        "a runner's late success report must not overwrite a canceled job"
    );
    assert_eq!(
        h.pipelines.snapshot()[0].status,
        PipelineStatus::Canceled,
        "and must not re-run aggregation to un-cancel the pipeline"
    );
    assert!(
        h.events.pipeline_events().is_empty() && h.events.job_events().is_empty(),
        "no event should be published for a dropped late report"
    );
}

#[tokio::test]
async fn reporting_a_result_for_a_job_that_does_not_exist_is_a_not_found_error() {
    let h = harness(Uuid::new_v4(), vec![]);

    let err = h
        .use_case
        .execute(Uuid::new_v4(), JobStatus::Success)
        .await
        .unwrap_err();

    assert!(matches!(err, DomainError::NotFound(_)));
}

#[tokio::test]
async fn a_successful_job_causes_its_newly_runnable_dependent_to_be_submitted() {
    let pipeline_id = Uuid::new_v4();
    let build = named_job(pipeline_id, "build", &[], JobStatus::Running);
    let test_job = named_job(pipeline_id, "test", &["build"], JobStatus::Pending);
    let h = harness(pipeline_id, vec![build.clone(), test_job.clone()]);

    h.use_case
        .execute(build.id, JobStatus::Success)
        .await
        .unwrap();

    assert_eq!(
        h.execution.submitted(),
        vec![test_job.id],
        "test's only need (build) just succeeded, so it must now be submitted"
    );
}

#[tokio::test]
async fn progression_routes_to_the_pipelines_own_engine() {
    let pipeline_id = Uuid::new_v4();
    let build = named_job(pipeline_id, "build", &[], JobStatus::Running);
    let test_job = named_job(pipeline_id, "test", &["build"], JobStatus::Pending);
    let docker_execution = Arc::new(FakeExecution::new());
    let kubernetes_execution = Arc::new(FakeExecution::new());
    let use_case = ReportJobResultUseCase::new(
        Arc::new(FakeJobs::new(vec![build.clone(), test_job.clone()])),
        Arc::new(FakePipelines::new(vec![Pipeline {
            execution_engine: ExecutionEngine::Kubernetes,
            ..pipeline(pipeline_id)
        }])),
        Arc::new(FakeEvents::new()),
        Arc::new(JobExecutionResolver::new(
            docker_execution.clone(),
            kubernetes_execution.clone(),
        )),
    );

    use_case
        .execute(build.id, JobStatus::Success)
        .await
        .unwrap();

    assert_eq!(kubernetes_execution.submitted(), vec![test_job.id]);
    assert!(docker_execution.submitted().is_empty());
}

#[tokio::test]
async fn a_failure_to_submit_a_newly_runnable_job_marks_it_failed_and_still_terminates_the_pipeline()
 {
    let pipeline_id = Uuid::new_v4();
    let build = named_job(pipeline_id, "build", &[], JobStatus::Running);
    let test_job = named_job(pipeline_id, "test", &["build"], JobStatus::Pending);
    let jobs = Arc::new(FakeJobs::new(vec![build.clone(), test_job.clone()]));
    let pipelines = Arc::new(FakePipelines::new(vec![pipeline(pipeline_id)]));
    let failing = Arc::new(FailingExecution);
    let use_case = ReportJobResultUseCase::new(
        jobs.clone(),
        pipelines.clone(),
        Arc::new(FakeEvents::new()),
        Arc::new(JobExecutionResolver::new(failing.clone(), failing.clone())),
    );

    let notify = use_case
        .execute(build.id, JobStatus::Success)
        .await
        .unwrap();

    assert_eq!(status_of(&jobs, test_job.id), JobStatus::Failed);
    assert_eq!(pipelines.snapshot()[0].status, PipelineStatus::Failed);
    assert_eq!(
        notify,
        Some(pipelines.snapshot()[0].triggered_by),
        "the pipeline-failure notification discovered inside the recursive cascade (submission failure -> execute(Failed)) must still reach the caller of the OUTER execute() call, not be silently dropped just because the outer call's own re-aggregation now correctly sees the pipeline as already Failed"
    );
}

#[tokio::test]
async fn a_pipeline_already_failed_does_not_notify_again_for_a_second_failing_job() {
    let pipeline_id = Uuid::new_v4();
    let first = named_job(pipeline_id, "build", &[], JobStatus::Failed);
    let second = named_job(pipeline_id, "lint", &[], JobStatus::Running);
    let h = harness_over(
        Pipeline {
            status: PipelineStatus::Failed,
            ..pipeline(pipeline_id)
        },
        vec![first, second.clone()],
    );

    let notify = h
        .use_case
        .execute(second.id, JobStatus::Failed)
        .await
        .unwrap();

    assert_eq!(
        notify, None,
        "the pipeline was already Failed before this call, so a second job failing must not trigger a second notification"
    );
    assert_eq!(
        h.pipelines.snapshot()[0].status,
        PipelineStatus::Failed,
        "the pipeline correctly stays Failed"
    );
}

#[tokio::test]
async fn a_pipeline_resolving_to_success_reports_no_one_to_notify() {
    let pipeline_id = Uuid::new_v4();
    let the_job = job(pipeline_id, JobStatus::Running);
    let h = harness(pipeline_id, vec![the_job.clone()]);

    let notify = h
        .use_case
        .execute(the_job.id, JobStatus::Success)
        .await
        .unwrap();

    assert_eq!(
        notify, None,
        "a pipeline resolving to Success has no failure to notify anyone about"
    );
}

#[tokio::test]
async fn notify_pipeline_failure_dispatches_a_webhook_and_a_notification() {
    let owner = fixtures::user("owner");
    let repository = fixtures::repository(owner.id);
    let pusher_id = Uuid::new_v4();
    let failed = Pipeline {
        repository_id: repository.id,
        commit_sha: "abc123".to_string(),
        status: PipelineStatus::Failed,
        triggered_by: pusher_id,
        ..pipeline(Uuid::new_v4())
    };
    let notifications = Arc::new(FakeNotifications::empty());
    let webhooks = Arc::new(FakeWebhooks::default());

    notify_pipeline_failure(
        Some(pusher_id),
        failed.id,
        &(Arc::new(FakePipelines::new(vec![failed])) as Arc<dyn PipelineStorePort>),
        &(Arc::new(FakeRepositories::new(vec![repository.clone()]))
            as Arc<dyn RepositoryStorePort>),
        &(Arc::new(FakeUsers::new(vec![owner])) as Arc<dyn UserRepositoryPort>),
        &(notifications.clone() as Arc<dyn NotificationStorePort>),
        &(webhooks.clone() as Arc<dyn WebhookDispatcherPort>),
    )
    .await;

    assert_eq!(notifications.snapshot().len(), 1);
    let dispatched = webhooks.dispatched();
    assert_eq!(dispatched.len(), 1);
    assert_eq!(dispatched[0].0, repository.id);
    assert!(
        matches!(&dispatched[0].1, WebhookEvent::PipelineFailed { commit_sha, .. } if commit_sha == "abc123")
    );
}

#[tokio::test]
async fn notify_pipeline_failure_dispatches_nothing_when_triggered_by_is_none() {
    let owner = fixtures::user("owner");
    let repository = fixtures::repository(owner.id);
    let notifications = Arc::new(FakeNotifications::empty());
    let webhooks = Arc::new(FakeWebhooks::default());

    notify_pipeline_failure(
        None,
        Uuid::new_v4(),
        &(Arc::new(FakePipelines::empty()) as Arc<dyn PipelineStorePort>),
        &(Arc::new(FakeRepositories::new(vec![repository])) as Arc<dyn RepositoryStorePort>),
        &(Arc::new(FakeUsers::new(vec![owner])) as Arc<dyn UserRepositoryPort>),
        &(notifications.clone() as Arc<dyn NotificationStorePort>),
        &(webhooks.clone() as Arc<dyn WebhookDispatcherPort>),
    )
    .await;

    assert!(notifications.snapshot().is_empty());
    assert!(webhooks.dispatched().is_empty());
}

#[tokio::test]
async fn the_next_stage_starts_only_once_every_job_of_the_previous_one_succeeded() {
    let pipeline_id = Uuid::new_v4();
    let compile = staged_job(pipeline_id, "build", "compile", &[], JobStatus::Running);
    let lint = staged_job(pipeline_id, "build", "lint", &[], JobStatus::Running);
    let unit = staged_job(pipeline_id, "test", "unit", &[], JobStatus::Pending);
    let h = harness(
        pipeline_id,
        vec![compile.clone(), lint.clone(), unit.clone()],
    );

    h.use_case
        .execute(compile.id, JobStatus::Success)
        .await
        .unwrap();
    assert!(
        h.execution.submitted().is_empty(),
        "lint is still running: the stage barrier holds"
    );

    h.use_case
        .execute(lint.id, JobStatus::Success)
        .await
        .unwrap();
    assert_eq!(h.execution.submitted(), vec![unit.id]);
    assert_eq!(
        h.pipelines.snapshot()[0].status,
        PipelineStatus::Running,
        "the unit job has not run yet"
    );
}

#[tokio::test]
async fn a_job_with_needs_is_released_by_them_without_waiting_for_the_whole_previous_stage() {
    let pipeline_id = Uuid::new_v4();
    let fast = staged_job(pipeline_id, "build", "fast", &[], JobStatus::Running);
    let slow = staged_job(pipeline_id, "build", "slow", &[], JobStatus::Running);
    let early = staged_job(pipeline_id, "test", "early", &["fast"], JobStatus::Pending);
    let late = staged_job(pipeline_id, "test", "late", &[], JobStatus::Pending);
    let h = harness(
        pipeline_id,
        vec![fast.clone(), slow, early.clone(), late.clone()],
    );

    h.use_case
        .execute(fast.id, JobStatus::Success)
        .await
        .unwrap();

    assert_eq!(h.execution.submitted(), vec![early.id]);
    assert_eq!(status_of(&h.jobs, late.id), JobStatus::Pending);
}

#[tokio::test]
async fn a_failure_skips_every_later_stage_and_the_pipeline_ends_failed() {
    let pipeline_id = Uuid::new_v4();
    let compile = staged_job(pipeline_id, "build", "compile", &[], JobStatus::Running);
    let unit = staged_job(pipeline_id, "test", "unit", &[], JobStatus::Pending);
    let docs = staged_job(pipeline_id, "test", "docs", &[], JobStatus::Pending);
    let ship = staged_job(pipeline_id, "deploy", "ship", &[], JobStatus::Pending);
    let h = harness(
        pipeline_id,
        vec![compile.clone(), unit.clone(), docs.clone(), ship.clone()],
    );

    let notify = h
        .use_case
        .execute(compile.id, JobStatus::Failed)
        .await
        .unwrap();

    for skipped in [&unit, &docs, &ship] {
        assert_eq!(status_of(&h.jobs, skipped.id), JobStatus::Skipped);
    }
    assert!(h.execution.submitted().is_empty());
    assert_eq!(h.pipelines.snapshot()[0].status, PipelineStatus::Failed);
    assert_eq!(notify, Some(h.pipelines.snapshot()[0].triggered_by));
}

#[tokio::test]
async fn a_failure_waits_for_the_jobs_still_running_before_ending_the_pipeline() {
    let pipeline_id = Uuid::new_v4();
    let broken = staged_job(pipeline_id, "build", "broken", &[], JobStatus::Running);
    let slow = staged_job(pipeline_id, "build", "slow", &[], JobStatus::Running);
    let unit = staged_job(pipeline_id, "test", "unit", &[], JobStatus::Pending);
    let h = harness(
        pipeline_id,
        vec![broken.clone(), slow.clone(), unit.clone()],
    );

    h.use_case
        .execute(broken.id, JobStatus::Failed)
        .await
        .unwrap();
    assert_eq!(status_of(&h.jobs, unit.id), JobStatus::Skipped);
    assert_eq!(
        h.pipelines.snapshot()[0].status,
        PipelineStatus::Running,
        "`slow` is still running: the pipeline is not over"
    );

    h.use_case
        .execute(slow.id, JobStatus::Success)
        .await
        .unwrap();
    assert_eq!(h.pipelines.snapshot()[0].status, PipelineStatus::Failed);
}

#[tokio::test]
async fn a_job_with_needs_still_runs_when_only_an_unrelated_job_of_the_previous_stage_failed() {
    let pipeline_id = Uuid::new_v4();
    let good = staged_job(pipeline_id, "build", "good", &[], JobStatus::Running);
    let bad = staged_job(pipeline_id, "build", "bad", &[], JobStatus::Running);
    let follow = staged_job(pipeline_id, "test", "follow", &["good"], JobStatus::Pending);
    let barrier = staged_job(pipeline_id, "test", "barrier", &[], JobStatus::Pending);
    let h = harness(
        pipeline_id,
        vec![good.clone(), bad.clone(), follow.clone(), barrier.clone()],
    );

    h.use_case.execute(bad.id, JobStatus::Failed).await.unwrap();
    assert_eq!(status_of(&h.jobs, follow.id), JobStatus::Pending);
    h.use_case
        .execute(good.id, JobStatus::Success)
        .await
        .unwrap();

    assert_eq!(h.execution.submitted(), vec![follow.id]);
    assert_eq!(status_of(&h.jobs, barrier.id), JobStatus::Skipped);
}

#[tokio::test]
async fn a_pipeline_whose_jobs_all_succeeded_across_stages_ends_successful() {
    let pipeline_id = Uuid::new_v4();
    let compile = staged_job(pipeline_id, "build", "compile", &[], JobStatus::Running);
    let unit = staged_job(pipeline_id, "test", "unit", &[], JobStatus::Pending);
    let h = harness(pipeline_id, vec![compile.clone(), unit.clone()]);

    h.use_case
        .execute(compile.id, JobStatus::Success)
        .await
        .unwrap();
    h.jobs
        .update_status(unit.id, JobStatus::Running)
        .await
        .unwrap();
    h.use_case
        .execute(unit.id, JobStatus::Success)
        .await
        .unwrap();

    assert_eq!(h.pipelines.snapshot()[0].status, PipelineStatus::Success);
}

#[tokio::test]
async fn a_canceled_job_skips_its_dependents_and_the_pipeline_ends_canceled() {
    let pipeline_id = Uuid::new_v4();
    let build = staged_job(pipeline_id, "build", "build", &[], JobStatus::Running);
    let test = staged_job(pipeline_id, "test", "unit", &[], JobStatus::Pending);
    let h = harness(pipeline_id, vec![build.clone(), test.clone()]);

    h.use_case
        .execute(build.id, JobStatus::Canceled)
        .await
        .unwrap();

    assert_eq!(status_of(&h.jobs, test.id), JobStatus::Skipped);
    assert_eq!(h.pipelines.snapshot()[0].status, PipelineStatus::Canceled);
}

#[tokio::test]
async fn skipped_jobs_alone_never_make_a_successful_pipeline() {
    // Only skipped jobs left means a failed one was swept elsewhere, so count it as failed.
    let pipeline_id = Uuid::new_v4();
    let ok = staged_job(pipeline_id, "build", "ok", &[], JobStatus::Running);
    let skipped = staged_job(pipeline_id, "test", "skipped", &[], JobStatus::Skipped);
    let h = harness(pipeline_id, vec![ok.clone(), skipped]);

    h.use_case.execute(ok.id, JobStatus::Success).await.unwrap();

    assert_eq!(h.pipelines.snapshot()[0].status, PipelineStatus::Failed);
}
