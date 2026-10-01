use std::sync::Arc;

use chrono::{DateTime, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::metrics_snapshot::{MetricsSnapshot, MetricsSnapshotRepositoryPort};

pub struct GetMetricsHistoryUseCase {
    metrics_snapshots: Arc<dyn MetricsSnapshotRepositoryPort>,
}

impl GetMetricsHistoryUseCase {
    pub fn new(metrics_snapshots: Arc<dyn MetricsSnapshotRepositoryPort>) -> Self {
        Self { metrics_snapshots }
    }

    pub async fn execute(&self, since: DateTime<Utc>) -> Result<Vec<MetricsSnapshot>, DomainError> {
        self.metrics_snapshots.list_since(since).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeMetricsSnapshots;
    use uuid::Uuid;

    #[tokio::test]
    async fn delegates_to_the_port_with_the_given_cutoff() {
        let snapshots = Arc::new(FakeMetricsSnapshots::empty());
        snapshots
            .save(&MetricsSnapshot {
                id: Uuid::new_v4(),
                recorded_at: Utc::now(),
                total_users: 1,
                total_repositories: 1,
                total_storage_bytes: 1,
            })
            .await
            .unwrap();
        let use_case = GetMetricsHistoryUseCase::new(snapshots);

        let result = use_case
            .execute(Utc::now() - chrono::Duration::days(1))
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
    }
}
