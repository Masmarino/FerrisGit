use std::sync::Arc;
use std::time::Duration;

use ferrisgit_application::use_cases::purge_expired_job_logs::PurgeExpiredJobLogsUseCase;
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// How often the retention runs.
pub const SWEEP_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// Sweeps at once, then every `every`, one sweep at a time. Stops when `shutdown` turns true, even mid-sweep: each
/// batch is one atomic statement, so nothing is left half done. A failed sweep is logged and retried on the next tick.
pub fn spawn(
    purge_expired_job_logs: Arc<PurgeExpiredJobLogsUseCase>,
    every: Duration,
    mut shutdown: watch::Receiver<bool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(every);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = shutdown.wait_for(|stopped| *stopped) => return,
            }
            tokio::select! {
                result = purge_expired_job_logs.execute(chrono::Utc::now()) => match result {
                    Ok(0) => {}
                    Ok(purged) => tracing::info!(purged, "emptied the logs of expired jobs"),
                    Err(e) => tracing::warn!("failed to apply the log retention: {e}"),
                },
                _ = shutdown.wait_for(|stopped| *stopped) => return,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use ferrisgit_domain::error::DomainError;
    use ferrisgit_domain::job::JobLogRetentionPort;
    use ferrisgit_domain::settings::{
        ExecutionEngine, SystemSettings, SystemSettingsStorePort, SystemSettingsUpdate,
    };
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uuid::Uuid;

    struct FixedSettings;
    #[async_trait]
    impl SystemSettingsStorePort for FixedSettings {
        async fn get(&self) -> Result<SystemSettings, DomainError> {
            Ok(SystemSettings {
                execution_engine: ExecutionEngine::DockerRunners,
                k8s_namespace: None,
                k8s_cache_storage_class: None,
                runner_registration_token: None,
                log_retention_days: Some(30),
                max_concurrent_jobs: None,
                jwt_ttl_hours: 12,
                max_push_size_mb: 500,
            })
        }
        async fn update(
            &self,
            _update: SystemSettingsUpdate,
        ) -> Result<SystemSettings, DomainError> {
            unimplemented!()
        }
    }

    /// Counts sweeps, fails the first `failures` of them, and can keep one open.
    struct CountingRetention {
        sweeps: AtomicUsize,
        failures: usize,
        hold: Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
    }

    impl CountingRetention {
        fn new(failures: usize) -> Arc<Self> {
            Arc::new(Self {
                sweeps: AtomicUsize::new(0),
                failures,
                hold: Mutex::new(None),
            })
        }
    }

    #[async_trait]
    impl JobLogRetentionPort for CountingRetention {
        async fn purge_logs_finished_before(
            &self,
            _cutoff: DateTime<Utc>,
            _limit: i64,
        ) -> Result<u64, DomainError> {
            let n = self.sweeps.fetch_add(1, Ordering::SeqCst);
            let hold = self.hold.lock().unwrap().take();
            if let Some(hold) = hold {
                let _ = hold.await;
            }
            if n < self.failures {
                return Err(DomainError::Infrastructure("db down".to_string()));
            }
            Ok(0)
        }
        async fn logs_purged_at(
            &self,
            _pipeline_id: Uuid,
        ) -> Result<HashMap<Uuid, DateTime<Utc>>, DomainError> {
            unimplemented!()
        }
    }

    fn use_case(retention: Arc<CountingRetention>) -> Arc<PurgeExpiredJobLogsUseCase> {
        Arc::new(PurgeExpiredJobLogsUseCase::new(
            Arc::new(FixedSettings),
            retention,
        ))
    }

    async fn until(condition: impl Fn() -> bool) {
        for _ in 0..200 {
            if condition() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("condition not met in time");
    }

    #[tokio::test]
    async fn it_sweeps_at_startup_and_then_on_every_tick() {
        let retention = CountingRetention::new(0);
        let (shutdown, rx) = watch::channel(false);

        let task = spawn(use_case(retention.clone()), Duration::from_millis(60), rx);

        until(|| retention.sweeps.load(Ordering::SeqCst) >= 1).await;
        let after_startup = retention.sweeps.load(Ordering::SeqCst);
        until(|| retention.sweeps.load(Ordering::SeqCst) > after_startup).await;
        shutdown.send_replace(true);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn it_waits_for_the_next_tick_instead_of_sweeping_in_a_loop() {
        let retention = CountingRetention::new(0);
        let (shutdown, rx) = watch::channel(false);

        let task = spawn(use_case(retention.clone()), Duration::from_secs(3600), rx);

        until(|| retention.sweeps.load(Ordering::SeqCst) == 1).await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(retention.sweeps.load(Ordering::SeqCst), 1);
        shutdown.send_replace(true);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn a_failed_sweep_does_not_stop_the_task() {
        let retention = CountingRetention::new(1);
        let (shutdown, rx) = watch::channel(false);

        let task = spawn(use_case(retention.clone()), Duration::from_millis(40), rx);

        until(|| retention.sweeps.load(Ordering::SeqCst) >= 3).await;
        assert!(!task.is_finished(), "the loop survives an error");
        shutdown.send_replace(true);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn it_stops_when_the_server_shuts_down_while_idle() {
        let retention = CountingRetention::new(0);
        let (shutdown, rx) = watch::channel(false);
        let task = spawn(use_case(retention.clone()), Duration::from_secs(3600), rx);
        until(|| retention.sweeps.load(Ordering::SeqCst) == 1).await;

        shutdown.send_replace(true);

        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("the task ends promptly")
            .unwrap();
    }

    #[tokio::test]
    async fn it_stops_even_in_the_middle_of_a_sweep() {
        let retention = CountingRetention::new(0);
        let (release, held) = tokio::sync::oneshot::channel();
        *retention.hold.lock().unwrap() = Some(held);
        let (shutdown, rx) = watch::channel(false);
        let task = spawn(use_case(retention.clone()), Duration::from_secs(3600), rx);
        until(|| retention.sweeps.load(Ordering::SeqCst) == 1).await;

        shutdown.send_replace(true);

        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("a stuck sweep does not block the shutdown")
            .unwrap();
        drop(release);
    }

    #[tokio::test]
    async fn it_never_runs_two_sweeps_at_the_same_time() {
        // The loop awaits each sweep before the next tick, so a slow one delays the next (missed ticks are skipped).
        let retention = CountingRetention::new(0);
        let (release, held) = tokio::sync::oneshot::channel();
        *retention.hold.lock().unwrap() = Some(held);
        let (shutdown, rx) = watch::channel(false);
        let task = spawn(use_case(retention.clone()), Duration::from_millis(20), rx);
        until(|| retention.sweeps.load(Ordering::SeqCst) == 1).await;

        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(
            retention.sweeps.load(Ordering::SeqCst),
            1,
            "ticks pass while the first sweep is still running, and none starts another"
        );

        release.send(()).unwrap();
        until(|| retention.sweeps.load(Ordering::SeqCst) >= 2).await;
        shutdown.send_replace(true);
        task.await.unwrap();
    }
}
