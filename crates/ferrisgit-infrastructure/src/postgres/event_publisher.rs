use async_trait::async_trait;
use ferrisgit_domain::audit::{EventPublisherPort, SecurityEvent};
use ferrisgit_domain::error::DomainError;
#[cfg(test)]
use ferrisgit_domain::job::JobStatus;
#[cfg(test)]
use ferrisgit_domain::pipeline::PipelineStatus;
use ferrisgit_domain::pipeline_events::{JobEvent, PipelineEvent, PipelineEventPublisherPort};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresEventPublisher {
    pool: PgPool,
}

impl PostgresEventPublisher {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EventPublisherPort for PostgresEventPublisher {
    async fn publish_security_event(
        &self,
        event: SecurityEvent,
        actor_id: Option<Uuid>,
    ) -> Result<(), DomainError> {
        let payload =
            serde_json::to_value(&event).map_err(|e| DomainError::Infrastructure(e.to_string()))?;

        // Use the event's subject id when it names one, so the audit log can filter by aggregate_id. Only
        // events with no natural subject (anonymous actor) get a fresh id.
        let aggregate_id = match &event {
            SecurityEvent::LoginSucceeded { user_id }
            | SecurityEvent::MfaVerificationFailed { user_id }
            | SecurityEvent::MfaEnrolled { user_id }
            | SecurityEvent::MfaDisabled { user_id }
            | SecurityEvent::PasskeyAdded { user_id }
            | SecurityEvent::PasskeyDeleted { user_id }
            | SecurityEvent::PasskeyVerificationFailed { user_id } => user_id.to_string(),
            SecurityEvent::MfaResetByAdmin { target_user_id }
            | SecurityEvent::PasswordResetByAdmin { target_user_id }
            | SecurityEvent::AdminGranted { target_user_id }
            | SecurityEvent::AdminRevoked { target_user_id }
            | SecurityEvent::UserDeletedByAdmin { target_user_id, .. } => {
                target_user_id.to_string()
            }
            SecurityEvent::GitAccessDenied { .. }
            | SecurityEvent::GitTokenInvalid { .. }
            | SecurityEvent::LoginFailed { .. } => {
                actor_id.map_or_else(|| Uuid::new_v4().to_string(), |id| id.to_string())
            }
        };

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        sqlx::query!("SELECT pg_advisory_xact_lock(hashtext($1))", aggregate_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;

        let current_version: i64 = sqlx::query_scalar!(
            "SELECT version FROM domain_events WHERE aggregate_type = 'Security' AND aggregate_id = $1 ORDER BY version DESC LIMIT 1 FOR UPDATE",
            aggregate_id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?
        .unwrap_or(0);

        sqlx::query!(
            "INSERT INTO domain_events (aggregate_type, aggregate_id, event_type, payload, version, actor_id) VALUES ('Security', $1, $2, $3, $4, $5)",
            aggregate_id,
            event.event_type(),
            payload,
            current_version + 1,
            actor_id,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl PipelineEventPublisherPort for PostgresEventPublisher {
    async fn publish_pipeline_event(
        &self,
        pipeline_id: Uuid,
        event: PipelineEvent,
    ) -> Result<(), DomainError> {
        publish_versioned_event(
            &self.pool,
            "Pipeline",
            &pipeline_id.to_string(),
            event.event_type(),
            &event,
            None,
        )
        .await
    }

    async fn publish_job_event(&self, job_id: Uuid, event: JobEvent) -> Result<(), DomainError> {
        publish_versioned_event(
            &self.pool,
            "Job",
            &job_id.to_string(),
            event.event_type(),
            &event,
            None,
        )
        .await
    }
}

/// Advisory-lock-then-append shared by `publish_security_event` and the `Pipeline`/`Job` events.
async fn publish_versioned_event<E: serde::Serialize>(
    pool: &sqlx::PgPool,
    aggregate_type: &str,
    aggregate_id: &str,
    event_type: &str,
    event: &E,
    actor_id: Option<Uuid>,
) -> Result<(), DomainError> {
    let payload =
        serde_json::to_value(event).map_err(|e| DomainError::Infrastructure(e.to_string()))?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
    sqlx::query!("SELECT pg_advisory_xact_lock(hashtext($1))", aggregate_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;

    let current_version: i64 = sqlx::query_scalar!(
        "SELECT version FROM domain_events WHERE aggregate_type = $1 AND aggregate_id = $2 ORDER BY version DESC LIMIT 1 FOR UPDATE",
        aggregate_type,
        aggregate_id
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| DomainError::Infrastructure(e.to_string()))?
    .unwrap_or(0);

    sqlx::query!(
        "INSERT INTO domain_events (aggregate_type, aggregate_id, event_type, payload, version, actor_id) VALUES ($1, $2, $3, $4, $5, $6)",
        aggregate_type,
        aggregate_id,
        event_type,
        payload,
        current_version + 1,
        actor_id,
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| DomainError::Infrastructure(e.to_string()))?;

    tx.commit()
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "../../migrations")]
    async fn publishing_a_security_event_inserts_a_row(pool: PgPool) {
        let publisher = PostgresEventPublisher::new(pool.clone());
        publisher
            .publish_security_event(
                SecurityEvent::LoginFailed {
                    username: "florian".to_string(),
                },
                None,
            )
            .await
            .unwrap();

        let count: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM domain_events WHERE event_type = 'LoginFailed'"
        )
        .fetch_one(&pool)
        .await
        .unwrap()
        .unwrap_or(0);
        assert_eq!(count, 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn two_login_succeeded_events_for_the_same_user_do_not_collide_on_version(pool: PgPool) {
        let publisher = PostgresEventPublisher::new(pool.clone());
        let user_id = Uuid::new_v4();

        publisher
            .publish_security_event(SecurityEvent::LoginSucceeded { user_id }, Some(user_id))
            .await
            .unwrap();
        publisher
            .publish_security_event(SecurityEvent::LoginSucceeded { user_id }, Some(user_id))
            .await
            .unwrap();

        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT aggregate_id, version FROM domain_events WHERE aggregate_type = 'Security' AND event_type = 'LoginSucceeded' ORDER BY version",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, user_id.to_string());
        assert_eq!(rows[1].0, user_id.to_string());
        assert_eq!(rows[0].1, 1);
        assert_eq!(rows[1].1, 2);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn an_admin_mfa_reset_is_recorded_against_the_target_user_not_the_acting_admin(
        pool: PgPool,
    ) {
        let publisher = PostgresEventPublisher::new(pool.clone());
        let admin_id = Uuid::new_v4();
        let target_user_id = Uuid::new_v4();

        publisher
            .publish_security_event(
                SecurityEvent::MfaResetByAdmin { target_user_id },
                Some(admin_id),
            )
            .await
            .unwrap();

        let (aggregate_id, actor_id): (String, Option<Uuid>) = sqlx::query_as(
            "SELECT aggregate_id, actor_id FROM domain_events WHERE event_type = 'MfaResetByAdmin'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(aggregate_id, target_user_id.to_string());
        assert_eq!(actor_id, Some(admin_id));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn admin_password_resets_and_admin_flag_changes_are_recorded_against_the_target_user(
        pool: PgPool,
    ) {
        let publisher = PostgresEventPublisher::new(pool.clone());
        let admin_id = Uuid::new_v4();
        let target_user_id = Uuid::new_v4();

        for event in [
            SecurityEvent::PasswordResetByAdmin { target_user_id },
            SecurityEvent::AdminGranted { target_user_id },
            SecurityEvent::AdminRevoked { target_user_id },
        ] {
            let event_type = event.event_type();
            publisher
                .publish_security_event(event, Some(admin_id))
                .await
                .unwrap();

            let (aggregate_id, actor_id): (String, Option<Uuid>) = sqlx::query_as(
                "SELECT aggregate_id, actor_id FROM domain_events WHERE event_type = $1",
            )
            .bind(event_type)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(aggregate_id, target_user_id.to_string(), "{event_type}");
            assert_eq!(actor_id, Some(admin_id), "{event_type}");
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn user_scoped_mfa_events_are_recorded_against_the_user_id(pool: PgPool) {
        let publisher = PostgresEventPublisher::new(pool.clone());
        let user_id = Uuid::new_v4();

        publisher
            .publish_security_event(SecurityEvent::MfaVerificationFailed { user_id }, None)
            .await
            .unwrap();
        publisher
            .publish_security_event(SecurityEvent::MfaEnrolled { user_id }, Some(user_id))
            .await
            .unwrap();
        publisher
            .publish_security_event(SecurityEvent::MfaDisabled { user_id }, Some(user_id))
            .await
            .unwrap();

        let rows: Vec<(String, String, i64)> = sqlx::query_as("SELECT event_type, aggregate_id, version FROM domain_events WHERE aggregate_type = 'Security' ORDER BY version").fetch_all(&pool).await.unwrap();
        assert_eq!(
            rows,
            vec![
                ("MfaVerificationFailed".to_string(), user_id.to_string(), 1),
                ("MfaEnrolled".to_string(), user_id.to_string(), 2),
                ("MfaDisabled".to_string(), user_id.to_string(), 3),
            ]
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn passkey_events_are_recorded_against_the_user_id(pool: PgPool) {
        let publisher = PostgresEventPublisher::new(pool.clone());
        let user_id = Uuid::new_v4();

        publisher
            .publish_security_event(SecurityEvent::PasskeyAdded { user_id }, Some(user_id))
            .await
            .unwrap();
        publisher
            .publish_security_event(SecurityEvent::PasskeyVerificationFailed { user_id }, None)
            .await
            .unwrap();
        publisher
            .publish_security_event(SecurityEvent::PasskeyDeleted { user_id }, Some(user_id))
            .await
            .unwrap();

        let rows: Vec<(String, String, i64)> = sqlx::query_as("SELECT event_type, aggregate_id, version FROM domain_events WHERE aggregate_type = 'Security' ORDER BY version").fetch_all(&pool).await.unwrap();
        assert_eq!(
            rows,
            vec![
                ("PasskeyAdded".to_string(), user_id.to_string(), 1),
                (
                    "PasskeyVerificationFailed".to_string(),
                    user_id.to_string(),
                    2
                ),
                ("PasskeyDeleted".to_string(), user_id.to_string(), 3),
            ]
        );
    }
}

#[cfg(test)]
mod pipeline_job_event_tests {
    use super::*;

    #[sqlx::test(migrations = "../../migrations")]
    async fn publishing_two_status_changes_for_the_same_pipeline_versions_correctly(pool: PgPool) {
        let publisher = PostgresEventPublisher::new(pool.clone());
        let pipeline_id = Uuid::new_v4();

        publisher
            .publish_pipeline_event(
                pipeline_id,
                PipelineEvent::StatusChanged {
                    status: PipelineStatus::Running,
                },
            )
            .await
            .unwrap();
        publisher
            .publish_pipeline_event(
                pipeline_id,
                PipelineEvent::StatusChanged {
                    status: PipelineStatus::Success,
                },
            )
            .await
            .unwrap();

        let rows: Vec<(String, i64)> = sqlx::query_as("SELECT aggregate_id, version FROM domain_events WHERE aggregate_type = 'Pipeline' ORDER BY version").fetch_all(&pool).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], (pipeline_id.to_string(), 1));
        assert_eq!(rows[1], (pipeline_id.to_string(), 2));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn publishing_a_job_event_uses_the_real_job_id_as_aggregate_id(pool: PgPool) {
        let publisher = PostgresEventPublisher::new(pool.clone());
        let job_id = Uuid::new_v4();

        publisher
            .publish_job_event(
                job_id,
                JobEvent::StatusChanged {
                    status: JobStatus::Failed,
                },
            )
            .await
            .unwrap();

        let aggregate_id: String = sqlx::query_scalar(
            "SELECT aggregate_id FROM domain_events WHERE aggregate_type = 'Job'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(aggregate_id, job_id.to_string());
    }
}
