use async_trait::async_trait;
use ferrisgit_domain::health::{ComponentHealth, DatabaseHealth, HealthCheckPort};
use sqlx::PgPool;

pub struct PostgresHealthCheck {
    pool: PgPool,
}

impl PostgresHealthCheck {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheckPort for PostgresHealthCheck {
    async fn check(&self) -> DatabaseHealth {
        let started = std::time::Instant::now();
        let result = sqlx::query_scalar::<_, String>("SELECT current_setting('server_version')")
            .fetch_one(&self.pool)
            .await;
        let response_time_ms = started.elapsed().as_millis() as u64;
        let max_connections = self.pool.options().get_max_connections();
        let active_connections = self.pool.size();

        match result {
            Ok(server_version) => DatabaseHealth {
                status: ComponentHealth::Up,
                response_time_ms,
                active_connections,
                max_connections,
                server_version: Some(server_version),
            },
            Err(e) => DatabaseHealth {
                status: ComponentHealth::Down(e.to_string()),
                response_time_ms,
                active_connections,
                max_connections,
                server_version: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn a_reachable_database_reports_up_with_a_server_version(pool: PgPool) {
        let checker = PostgresHealthCheck::new(pool);
        let health = checker.check().await;

        assert_eq!(health.status, ComponentHealth::Up);
        assert!(
            health.server_version.is_some(),
            "a reachable Postgres must report its version"
        );
        assert!(health.max_connections > 0);
    }

    #[sqlx::test]
    async fn checking_against_a_closed_pool_reports_down(pool: PgPool) {
        pool.close().await;
        let checker = PostgresHealthCheck::new(pool);
        let health = checker.check().await;

        assert_eq!(health.status.status_str(), "down");
        assert!(
            health.status.detail().is_some(),
            "a failed check must carry an error message as detail"
        );
    }
}
