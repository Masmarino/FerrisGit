use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::error::DomainError;

#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    pub id: Uuid,
    pub recorded_at: DateTime<Utc>,
    pub total_users: i64,
    pub total_repositories: i64,
    pub total_storage_bytes: i64,
}

#[async_trait]
pub trait MetricsSnapshotRepositoryPort: Send + Sync {
    async fn save(&self, snapshot: &MetricsSnapshot) -> Result<(), DomainError>;
    async fn list_since(&self, since: DateTime<Utc>) -> Result<Vec<MetricsSnapshot>, DomainError>;
}
