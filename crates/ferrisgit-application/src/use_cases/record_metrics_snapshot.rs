use std::sync::Arc;

use chrono::Utc;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::metrics_snapshot::{MetricsSnapshot, MetricsSnapshotRepositoryPort};
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::storage_size::DirectorySizePort;
use ferrisgit_domain::user::UserRepositoryPort;
use uuid::Uuid;

pub struct RecordMetricsSnapshotUseCase {
    repositories: Arc<dyn RepositoryStorePort>,
    users: Arc<dyn UserRepositoryPort>,
    directory_size: Arc<dyn DirectorySizePort>,
    metrics_snapshots: Arc<dyn MetricsSnapshotRepositoryPort>,
}

impl RecordMetricsSnapshotUseCase {
    pub fn new(
        repositories: Arc<dyn RepositoryStorePort>,
        users: Arc<dyn UserRepositoryPort>,
        directory_size: Arc<dyn DirectorySizePort>,
        metrics_snapshots: Arc<dyn MetricsSnapshotRepositoryPort>,
    ) -> Self {
        Self {
            repositories,
            users,
            directory_size,
            metrics_snapshots,
        }
    }

    pub async fn execute(&self) -> Result<(), DomainError> {
        let repositories = self.repositories.list_all().await?;
        let total_users = self.users.count().await?;

        let mut total_storage_bytes: i64 = 0;
        for repo in &repositories {
            let directory_size = self.directory_size.clone();
            let disk_path = repo.disk_path.clone();
            // Blocking I/O, hence spawn_blocking. A repo that fails (deleted on disk, permissions) counts as 0 instead
            // of failing the whole snapshot.
            let size =
                tokio::task::spawn_blocking(move || directory_size.directory_size(&disk_path))
                    .await
                    .ok()
                    .and_then(|r| r.ok())
                    .unwrap_or(0);
            total_storage_bytes += size as i64;
        }

        let snapshot = MetricsSnapshot {
            id: Uuid::new_v4(),
            recorded_at: Utc::now(),
            total_users,
            total_repositories: repositories.len() as i64,
            total_storage_bytes,
        };
        self.metrics_snapshots.save(&snapshot).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeDirectorySize, FakeMetricsSnapshots, FakeRepositories, FakeUsers,
    };
    use crate::use_cases::fixtures;
    use ferrisgit_domain::repository::Repository;
    use ferrisgit_domain::user::User;
    use std::collections::HashMap;
    use uuid::Uuid;

    fn a_user() -> User {
        fixtures::user("alice")
    }

    fn a_repo(owner_id: Uuid, disk_path: &str) -> Repository {
        Repository {
            disk_path: disk_path.to_string(),
            ..fixtures::repository(owner_id)
        }
    }

    #[tokio::test]
    async fn saves_a_snapshot_with_current_totals() {
        let owner = a_user();
        let repo = a_repo(owner.id, "a/hello.git");
        let repositories = Arc::new(FakeRepositories::new(vec![repo]));
        let users = Arc::new(FakeUsers::new(vec![owner]));
        let directory_size = Arc::new(FakeDirectorySize::new(HashMap::from([(
            "a/hello.git".to_string(),
            2048u64,
        )])));
        let metrics_snapshots = Arc::new(FakeMetricsSnapshots::empty());
        let use_case = RecordMetricsSnapshotUseCase::new(
            repositories,
            users,
            directory_size,
            metrics_snapshots.clone(),
        );

        use_case.execute().await.unwrap();

        let saved = metrics_snapshots.snapshot();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].total_users, 1);
        assert_eq!(saved[0].total_repositories, 1);
        assert_eq!(saved[0].total_storage_bytes, 2048);
    }

    #[tokio::test]
    async fn sums_storage_across_every_repository() {
        let owner = a_user();
        let repo_a = a_repo(owner.id, "a.git");
        let repo_b = a_repo(owner.id, "b.git");
        let repositories = Arc::new(FakeRepositories::new(vec![repo_a, repo_b]));
        let users = Arc::new(FakeUsers::new(vec![owner]));
        let directory_size = Arc::new(FakeDirectorySize::new(HashMap::from([
            ("a.git".to_string(), 100u64),
            ("b.git".to_string(), 250u64),
        ])));
        let metrics_snapshots = Arc::new(FakeMetricsSnapshots::empty());
        let use_case = RecordMetricsSnapshotUseCase::new(
            repositories,
            users,
            directory_size,
            metrics_snapshots.clone(),
        );

        use_case.execute().await.unwrap();

        assert_eq!(metrics_snapshots.snapshot()[0].total_storage_bytes, 350);
    }
}
