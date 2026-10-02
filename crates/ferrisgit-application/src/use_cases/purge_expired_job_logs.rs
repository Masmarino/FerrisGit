use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::job::JobLogRetentionPort;
use ferrisgit_domain::settings::SystemSettingsStorePort;
use tokio::sync::Mutex;

/// Jobs per UPDATE, to keep statements short when the logs column is big.
const BATCH_SIZE: i64 = 500;

/// Empties the logs of finished jobs older than the retention setting. Pipelines, jobs and statuses stay.
pub struct PurgeExpiredJobLogsUseCase {
    system_settings: Arc<dyn SystemSettingsStorePort>,
    retention: Arc<dyn JobLogRetentionPort>,
    sweeping: Mutex<()>,
}

impl PurgeExpiredJobLogsUseCase {
    pub fn new(
        system_settings: Arc<dyn SystemSettingsStorePort>,
        retention: Arc<dyn JobLogRetentionPort>,
    ) -> Self {
        Self {
            system_settings,
            retention,
            sweeping: Mutex::new(()),
        }
    }

    /// Returns how many logs were emptied. A no-op without a retention or while another sweep is running.
    pub async fn execute(&self, now: DateTime<Utc>) -> Result<u64, DomainError> {
        let Some(days) = self.system_settings.get().await?.log_retention_days else {
            return Ok(0);
        };
        if days < 1 {
            return Ok(0);
        }
        let Ok(_guard) = self.sweeping.try_lock() else {
            return Ok(0);
        };
        let cutoff = now - Duration::days(days as i64);
        let mut total = 0;
        loop {
            let purged = self
                .retention
                .purge_logs_finished_before(cutoff, BATCH_SIZE)
                .await?;
            total += purged;
            if purged < BATCH_SIZE as u64 {
                return Ok(total);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeSystemSettings;
    use async_trait::async_trait;
    use ferrisgit_domain::settings::{ExecutionEngine, SystemSettings};
    use std::collections::HashMap;
    use std::sync::Mutex as StdMutex;
    use uuid::Uuid;

    struct FakeRetention {
        eligible: StdMutex<u64>,
        calls: StdMutex<Vec<(DateTime<Utc>, i64)>>,
    }

    impl FakeRetention {
        fn with_eligible(eligible: u64) -> Arc<Self> {
            Arc::new(Self {
                eligible: StdMutex::new(eligible),
                calls: StdMutex::new(vec![]),
            })
        }
    }

    #[async_trait]
    impl JobLogRetentionPort for FakeRetention {
        async fn purge_logs_finished_before(
            &self,
            cutoff: DateTime<Utc>,
            limit: i64,
        ) -> Result<u64, DomainError> {
            self.calls.lock().unwrap().push((cutoff, limit));
            let mut eligible = self.eligible.lock().unwrap();
            let purged = (*eligible).min(limit as u64);
            *eligible -= purged;
            Ok(purged)
        }

        async fn logs_purged_at(
            &self,
            _pipeline_id: Uuid,
        ) -> Result<HashMap<Uuid, DateTime<Utc>>, DomainError> {
            unimplemented!()
        }
    }

    fn settings(log_retention_days: Option<i32>) -> Arc<FakeSystemSettings> {
        Arc::new(FakeSystemSettings::new(SystemSettings {
            execution_engine: ExecutionEngine::DockerRunners,
            k8s_namespace: None,
            k8s_cache_storage_class: None,
            runner_registration_token: None,
            log_retention_days,
            max_concurrent_jobs: None,
            jwt_ttl_hours: 12,
            max_push_size_mb: 500,
        }))
    }

    #[tokio::test]
    async fn without_a_retention_nothing_is_touched() {
        let retention = FakeRetention::with_eligible(10);
        let use_case = PurgeExpiredJobLogsUseCase::new(settings(None), retention.clone());

        assert_eq!(use_case.execute(Utc::now()).await.unwrap(), 0);

        assert!(retention.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_cutoff_is_the_retention_in_days_before_now() {
        let retention = FakeRetention::with_eligible(3);
        let use_case = PurgeExpiredJobLogsUseCase::new(settings(Some(30)), retention.clone());
        let now = Utc::now();

        assert_eq!(use_case.execute(now).await.unwrap(), 3);

        let calls = retention.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, now - Duration::days(30));
    }

    #[tokio::test]
    async fn a_sweep_keeps_going_in_batches_until_nothing_eligible_is_left() {
        let retention = FakeRetention::with_eligible(BATCH_SIZE as u64 * 2 + 7);
        let use_case = PurgeExpiredJobLogsUseCase::new(settings(Some(7)), retention.clone());

        let purged = use_case.execute(Utc::now()).await.unwrap();

        assert_eq!(purged, BATCH_SIZE as u64 * 2 + 7);
        assert_eq!(retention.calls.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn a_sweep_is_skipped_while_another_one_is_running() {
        let retention = FakeRetention::with_eligible(5);
        let use_case = PurgeExpiredJobLogsUseCase::new(settings(Some(7)), retention.clone());
        let _running = use_case.sweeping.lock().await;

        assert_eq!(use_case.execute(Utc::now()).await.unwrap(), 0);

        assert!(retention.calls.lock().unwrap().is_empty());
    }
}
