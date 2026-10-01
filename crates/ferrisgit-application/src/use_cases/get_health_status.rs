use std::sync::Arc;
use std::time::Instant;

use ferrisgit_domain::health::{
    DatabaseHealth, HealthCheckPort, StorageHealth, StorageHealthCheckPort,
};

pub struct HealthStatus {
    pub database: DatabaseHealth,
    pub storage: StorageHealth,
    pub uptime_seconds: u64,
}

pub struct GetHealthStatusUseCase {
    health_check: Arc<dyn HealthCheckPort>,
    storage_health: Arc<dyn StorageHealthCheckPort>,
    started_at: Instant,
}

impl GetHealthStatusUseCase {
    pub fn new(
        health_check: Arc<dyn HealthCheckPort>,
        storage_health: Arc<dyn StorageHealthCheckPort>,
        started_at: Instant,
    ) -> Self {
        Self {
            health_check,
            storage_health,
            started_at,
        }
    }

    pub async fn execute(&self) -> HealthStatus {
        let database = self.health_check.check().await;
        let storage = self.storage_health.check().await;
        HealthStatus {
            database,
            storage,
            uptime_seconds: self.started_at.elapsed().as_secs(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeHealthCheck, FakeStorageHealthCheck};
    use ferrisgit_domain::health::ComponentHealth;

    #[tokio::test]
    async fn combines_database_and_storage_health_with_uptime() {
        let health_check = Arc::new(FakeHealthCheck::new(DatabaseHealth {
            status: ComponentHealth::Up,
            response_time_ms: 5,
            active_connections: 2,
            max_connections: 10,
            server_version: Some("18.0".to_string()),
        }));
        let storage_health = Arc::new(FakeStorageHealthCheck::new(StorageHealth {
            status: ComponentHealth::Up,
            used_bytes: 10,
            free_bytes: 90,
            total_bytes: 100,
        }));
        let started_at = Instant::now() - std::time::Duration::from_secs(42);
        let use_case = GetHealthStatusUseCase::new(health_check, storage_health, started_at);

        let status = use_case.execute().await;

        assert_eq!(status.database.server_version, Some("18.0".to_string()));
        assert_eq!(status.storage.total_bytes, 100);
        assert!(status.uptime_seconds >= 42);
    }
}
