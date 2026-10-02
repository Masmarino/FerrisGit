use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Running,
    Success,
    Failed,
    Canceled,
    /// Never started because a job it depends on (by `needs`, or through a stage barrier) did not succeed.
    Skipped,
}

impl JobStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            JobStatus::Success | JobStatus::Failed | JobStatus::Canceled | JobStatus::Skipped
        )
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Pending => "pending",
            JobStatus::Running => "running",
            JobStatus::Success => "success",
            JobStatus::Failed => "failed",
            JobStatus::Canceled => "canceled",
            JobStatus::Skipped => "skipped",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "pending" => Ok(JobStatus::Pending),
            "running" => Ok(JobStatus::Running),
            "success" => Ok(JobStatus::Success),
            "failed" => Ok(JobStatus::Failed),
            "canceled" => Ok(JobStatus::Canceled),
            "skipped" => Ok(JobStatus::Skipped),
            other => Err(DomainError::Validation(format!(
                "unknown job status: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Job {
    pub id: Uuid,
    pub pipeline_id: Uuid,
    pub stage: String,
    pub name: String,
    pub image: String,
    pub script: Vec<String>,
    pub variables: std::collections::BTreeMap<String, String>,
    pub needs: Vec<String>,
    pub tags: Vec<String>,
    pub cache: Vec<String>,
    pub status: JobStatus,
    pub runner_id: Option<Uuid>,
    pub logs: String,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

pub struct NewJob {
    pub pipeline_id: Uuid,
    pub stage: String,
    pub name: String,
    pub image: String,
    pub script: Vec<String>,
    pub variables: std::collections::BTreeMap<String, String>,
    pub needs: Vec<String>,
    pub tags: Vec<String>,
    pub cache: Vec<String>,
}

/// Where the server decides which jobs may start, for every engine: `claim_next` (Docker runners poll) and
/// `list_runnable` (the server pushes, as with Kubernetes) both call this. `jobs` are one pipeline's jobs in creation
/// order, which is stage order, so a stage's position is that of its first job.
///
/// A pending job is released when:
/// - it has `needs`: all of them succeeded;
/// - otherwise, and it is not in the first stage: every job of every earlier stage succeeded;
/// - otherwise (first stage, no `needs`): right away.
pub fn runnable_jobs(jobs: &[Job]) -> Vec<&Job> {
    jobs.iter()
        .filter(|job| job.status == JobStatus::Pending)
        .filter(|job| {
            dependencies(job, jobs)
                .is_some_and(|deps| deps.iter().all(|dep| dep.status == JobStatus::Success))
        })
        .collect()
}

/// The pending jobs that can never start because something they wait on (see `runnable_jobs`) failed, was canceled or
/// was itself skipped, transitively. The caller marks them `Skipped`, which is what lets the pipeline finish.
pub fn unreachable_jobs(jobs: &[Job]) -> Vec<&Job> {
    let mut dead: std::collections::HashSet<Uuid> = jobs
        .iter()
        .filter(|job| {
            matches!(
                job.status,
                JobStatus::Failed | JobStatus::Canceled | JobStatus::Skipped
            )
        })
        .map(|job| job.id)
        .collect();
    let mut unreachable = Vec::new();
    loop {
        let mut progressed = false;
        for job in jobs.iter().filter(|j| j.status == JobStatus::Pending) {
            if dead.contains(&job.id) {
                continue;
            }
            let is_dead = match dependencies(job, jobs) {
                Some(deps) => deps.iter().any(|dep| dead.contains(&dep.id)),
                None => true,
            };
            if is_dead {
                dead.insert(job.id);
                unreachable.push(job);
                progressed = true;
            }
        }
        if !progressed {
            return unreachable;
        }
    }
}

/// The jobs `job` waits on. `None` when a `needs` entry names no job of the pipeline: it can never be satisfied.
fn dependencies<'a>(job: &Job, jobs: &'a [Job]) -> Option<Vec<&'a Job>> {
    if !job.needs.is_empty() {
        return job
            .needs
            .iter()
            .map(|need| jobs.iter().find(|j| &j.name == need))
            .collect();
    }
    let first_of_stage = jobs.iter().position(|j| j.stage == job.stage)?;
    Some(jobs[..first_of_stage].iter().collect())
}

#[async_trait]
pub trait JobStorePort: Send + Sync {
    async fn create(&self, new_job: NewJob) -> Result<Job, DomainError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Job>, DomainError>;
    async fn list_for_pipeline(&self, pipeline_id: Uuid) -> Result<Vec<Job>, DomainError>;
    /// Eligible if the job's `tags` is empty or a subset of `runner_tags` and `runnable_jobs` releases it. Sets it
    /// `running`. `None` if nothing is claimable.
    async fn claim_next(
        &self,
        runner_id: Uuid,
        runner_tags: &[String],
    ) -> Result<Option<Job>, DomainError>;
    async fn append_logs(&self, id: Uuid, chunk: &str) -> Result<(), DomainError>;
    /// Moves a job to `status` only from a non-terminal state. A terminal job is left untouched and `false` is returned
    /// (also when the job doesn't exist). This keeps a cancellation final: a late runner report must not bring the job
    /// back.
    async fn update_status(&self, id: Uuid, status: JobStatus) -> Result<bool, DomainError>;
    /// Puts every `running` job claimed by `runner_id` back to `pending` (runner revoked): `jobs.runner_id` is `ON
    /// DELETE SET NULL` and `claim_next` only looks at `pending`, so such a job would be stranded. Returns how many
    /// were released.
    async fn release_jobs_claimed_by(&self, runner_id: Uuid) -> Result<u64, DomainError>;
    /// Across every pipeline; enforces `system_settings.max_concurrent_jobs`.
    async fn count_running(&self) -> Result<i64, DomainError>;
    /// The pipeline's jobs that `runnable_jobs` releases, without claiming them (unlike `claim_next`). Lets an engine
    /// with no polling of its own, like Kubernetes, progress. Their `logs` are left empty.
    async fn list_runnable(&self, pipeline_id: Uuid) -> Result<Vec<Job>, DomainError>;
}

/// The log retention policy (`system_settings.log_retention_days`). Kept apart from `JobStorePort`: only a background
/// sweep and the pipeline detail page need it.
#[async_trait]
pub trait JobLogRetentionPort: Send + Sync {
    /// Empties the logs of at most `limit` terminal jobs that finished before `cutoff` and remembers when. Jobs,
    /// pipelines and statuses stay. Returns how many jobs were purged; a result below `limit` means nothing eligible
    /// is left.
    async fn purge_logs_finished_before(
        &self,
        cutoff: DateTime<Utc>,
        limit: i64,
    ) -> Result<u64, DomainError>;
    /// When the logs of the jobs of a pipeline were purged, for the jobs that were.
    async fn logs_purged_at(
        &self,
        pipeline_id: Uuid,
    ) -> Result<std::collections::HashMap<Uuid, DateTime<Utc>>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_round_trips_through_its_string_form() {
        assert_eq!(JobStatus::parse("failed").unwrap(), JobStatus::Failed);
        assert_eq!(JobStatus::Canceled.as_str(), "canceled");
    }

    #[test]
    fn parsing_an_unknown_status_is_a_validation_error() {
        assert!(matches!(
            JobStatus::parse("bogus"),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn skipped_round_trips_and_is_terminal() {
        assert_eq!(JobStatus::parse("skipped").unwrap(), JobStatus::Skipped);
        assert_eq!(JobStatus::Skipped.as_str(), "skipped");
        assert!(JobStatus::Skipped.is_terminal());
        assert!(!JobStatus::Pending.is_terminal());
        assert!(!JobStatus::Running.is_terminal());
    }

    fn job(stage: &str, name: &str, needs: &[&str], status: JobStatus) -> Job {
        Job {
            id: Uuid::new_v4(),
            pipeline_id: Uuid::nil(),
            stage: stage.to_string(),
            name: name.to_string(),
            image: "alpine".to_string(),
            script: vec![],
            variables: Default::default(),
            needs: needs.iter().map(|n| n.to_string()).collect(),
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

    fn names(jobs: Vec<&Job>) -> Vec<&str> {
        jobs.into_iter().map(|j| j.name.as_str()).collect()
    }

    use JobStatus::{Failed, Pending, Running, Skipped, Success};

    #[test]
    fn the_first_stage_is_released_at_once_and_later_stages_wait() {
        let jobs = [
            job("build", "a", &[], Pending),
            job("build", "b", &[], Pending),
            job("test", "c", &[], Pending),
        ];
        assert_eq!(names(runnable_jobs(&jobs)), ["a", "b"]);
    }

    #[test]
    fn a_stage_is_a_barrier_for_every_job_of_every_earlier_stage() {
        let mut jobs = [
            job("build", "a", &[], Success),
            job("build", "b", &[], Running),
            job("test", "c", &[], Pending),
            job("deploy", "d", &[], Pending),
        ];
        assert!(runnable_jobs(&jobs).is_empty(), "b is still running");

        jobs[1].status = Success;
        assert_eq!(names(runnable_jobs(&jobs)), ["c"]);

        jobs[2].status = Success;
        assert_eq!(names(runnable_jobs(&jobs)), ["d"]);
    }

    #[test]
    fn needs_replace_the_barrier_for_the_job_that_declares_them() {
        let jobs = [
            job("build", "fast", &[], Success),
            job("build", "slow", &[], Running),
            job("test", "early", &["fast"], Pending),
            job("test", "late", &[], Pending),
        ];
        assert_eq!(names(runnable_jobs(&jobs)), ["early"]);
    }

    #[test]
    fn a_job_with_needs_waits_for_all_of_them_even_within_its_stage() {
        let mut jobs = [
            job("build", "a", &[], Success),
            job("build", "b", &["a", "c"], Pending),
            job("build", "c", &[], Running),
        ];
        assert!(runnable_jobs(&jobs).is_empty());
        jobs[2].status = Success;
        assert_eq!(names(runnable_jobs(&jobs)), ["b"]);
    }

    #[test]
    fn jobs_that_are_not_pending_are_never_released() {
        let jobs = [
            job("build", "a", &[], Running),
            job("build", "b", &[], Success),
        ];
        assert!(runnable_jobs(&jobs).is_empty());
    }

    #[test]
    fn a_failure_makes_its_needs_dependents_and_the_following_stages_unreachable() {
        let jobs = [
            job("build", "compile", &[], Failed),
            job("build", "lint", &[], Success),
            job("test", "unit", &["compile"], Pending),
            job("test", "docs", &[], Pending),
            job("deploy", "ship", &["docs"], Pending),
            job("deploy", "notify", &[], Pending),
        ];
        // `docs` waits on the build barrier, `ship` on `docs` (transitively), `notify` on the test barrier.
        assert_eq!(
            names(unreachable_jobs(&jobs)),
            ["unit", "docs", "ship", "notify"]
        );
    }

    #[test]
    fn a_job_with_needs_survives_a_failure_it_does_not_depend_on() {
        let jobs = [
            job("build", "ok", &[], Success),
            job("build", "broken", &[], Failed),
            job("test", "independent", &["ok"], Pending),
            job("test", "barrier", &[], Pending),
        ];
        assert_eq!(names(unreachable_jobs(&jobs)), ["barrier"]);
        assert_eq!(names(runnable_jobs(&jobs)), ["independent"]);
    }

    #[test]
    fn skipped_and_canceled_jobs_block_their_dependents_like_a_failure() {
        let jobs = [
            job("build", "a", &[], Skipped),
            job("test", "b", &[], Pending),
        ];
        assert_eq!(names(unreachable_jobs(&jobs)), ["b"]);
        let jobs = [
            job("build", "a", &[], JobStatus::Canceled),
            job("test", "b", &["a"], Pending),
        ];
        assert_eq!(names(unreachable_jobs(&jobs)), ["b"]);
    }

    #[test]
    fn a_running_job_does_not_make_anything_unreachable() {
        let jobs = [
            job("build", "a", &[], Running),
            job("test", "b", &[], Pending),
        ];
        assert!(unreachable_jobs(&jobs).is_empty());
    }

    #[test]
    fn a_need_naming_no_job_is_never_released_and_unreachable() {
        let jobs = [job("build", "a", &["ghost"], Pending)];
        assert!(runnable_jobs(&jobs).is_empty());
        assert_eq!(names(unreachable_jobs(&jobs)), ["a"]);
    }
}
