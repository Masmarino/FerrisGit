use crate::error::infra;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::metrics_snapshot::{MetricsSnapshot, MetricsSnapshotRepositoryPort};
use sqlx::PgPool;

pub struct PostgresMetricsSnapshotStore {
    pool: PgPool,
}

impl PostgresMetricsSnapshotStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MetricsSnapshotRepositoryPort for PostgresMetricsSnapshotStore {
    async fn save(&self, snapshot: &MetricsSnapshot) -> Result<(), DomainError> {
        sqlx::query!(
            "INSERT INTO metrics_snapshots (id, recorded_at, total_users, total_repositories, total_storage_bytes) VALUES ($1, $2, $3, $4, $5)",
            snapshot.id,
            snapshot.recorded_at,
            snapshot.total_users,
            snapshot.total_repositories,
            snapshot.total_storage_bytes,
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }

    async fn list_since(&self, since: DateTime<Utc>) -> Result<Vec<MetricsSnapshot>, DomainError> {
        let rows = sqlx::query_as!(
            MetricsSnapshot,
            "SELECT id, recorded_at, total_users, total_repositories, total_storage_bytes FROM metrics_snapshots WHERE recorded_at >= $1 ORDER BY recorded_at ASC",
            since
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use uuid::Uuid;

    fn snapshot(recorded_at: DateTime<Utc>) -> MetricsSnapshot {
        MetricsSnapshot {
            id: Uuid::new_v4(),
            recorded_at,
            total_users: 3,
            total_repositories: 5,
            total_storage_bytes: 1024,
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn save_then_list_since_returns_it(pool: PgPool) {
        let store = PostgresMetricsSnapshotStore::new(pool);
        let saved = snapshot(Utc::now());
        store.save(&saved).await.unwrap();

        let rows = store
            .list_since(saved.recorded_at - Duration::minutes(1))
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].total_users, 3);
        assert_eq!(rows[0].total_repositories, 5);
        assert_eq!(rows[0].total_storage_bytes, 1024);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_since_excludes_rows_recorded_before_the_cutoff(pool: PgPool) {
        let store = PostgresMetricsSnapshotStore::new(pool);
        let old = snapshot(Utc::now() - Duration::hours(2));
        let recent = snapshot(Utc::now());
        store.save(&old).await.unwrap();
        store.save(&recent).await.unwrap();

        let rows = store
            .list_since(Utc::now() - Duration::hours(1))
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, recent.id);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_since_orders_oldest_first(pool: PgPool) {
        let store = PostgresMetricsSnapshotStore::new(pool);
        let earlier = snapshot(Utc::now() - Duration::minutes(30));
        let later = snapshot(Utc::now());
        // Insert in reverse so the test fails if the query just returns insertion order.
        store.save(&later).await.unwrap();
        store.save(&earlier).await.unwrap();

        let rows = store
            .list_since(Utc::now() - Duration::hours(1))
            .await
            .unwrap();
        assert_eq!(
            rows.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![earlier.id, later.id]
        );
    }
}
