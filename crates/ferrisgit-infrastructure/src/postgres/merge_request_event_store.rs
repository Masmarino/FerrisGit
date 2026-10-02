use crate::error::infra;
use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::merge_request_event::{
    MergeRequestEvent, MergeRequestEventKind, MergeRequestEventPort, NewMergeRequestEvent,
};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresMergeRequestEventStore {
    pool: PgPool,
}

impl PostgresMergeRequestEventStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct EventRow {
    id: Uuid,
    merge_request_id: Uuid,
    actor_id: Option<Uuid>,
    kind: String,
    payload: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<EventRow> for MergeRequestEvent {
    type Error = DomainError;

    fn try_from(row: EventRow) -> Result<Self, DomainError> {
        Ok(MergeRequestEvent {
            id: row.id,
            merge_request_id: row.merge_request_id,
            actor_id: row.actor_id,
            kind: MergeRequestEventKind::parse(&row.kind)?,
            payload: row.payload,
            created_at: row.created_at,
        })
    }
}

#[async_trait]
impl MergeRequestEventPort for PostgresMergeRequestEventStore {
    async fn record(&self, event: NewMergeRequestEvent) -> Result<MergeRequestEvent, DomainError> {
        let row = sqlx::query_as::<_, EventRow>("INSERT INTO merge_request_events (merge_request_id, actor_id, kind, payload) VALUES ($1, $2, $3, $4) RETURNING id, merge_request_id, actor_id, kind, payload, created_at")
            .bind(event.merge_request_id)
            .bind(event.actor_id)
            .bind(event.kind.as_str())
            .bind(&event.payload)
            .fetch_one(&self.pool)
            .await
            .map_err(infra)?;
        MergeRequestEvent::try_from(row)
    }

    async fn list(&self, merge_request_id: Uuid) -> Result<Vec<MergeRequestEvent>, DomainError> {
        let rows = sqlx::query_as::<_, EventRow>("SELECT id, merge_request_id, actor_id, kind, payload, created_at FROM merge_request_events WHERE merge_request_id = $1 ORDER BY created_at ASC, id ASC")
            .bind(merge_request_id)
            .fetch_all(&self.pool)
            .await
            .map_err(infra)?;
        rows.into_iter().map(MergeRequestEvent::try_from).collect()
    }

    async fn head_sha(&self, merge_request_id: Uuid) -> Result<Option<String>, DomainError> {
        let row = sqlx::query_scalar::<_, Option<String>>(
            "SELECT head_sha FROM merge_requests WHERE id = $1",
        )
        .bind(merge_request_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;
        match row {
            Some(head_sha) => Ok(head_sha),
            None => Err(DomainError::NotFound("merge request".to_string())),
        }
    }

    async fn set_head_sha(&self, merge_request_id: Uuid, sha: &str) -> Result<(), DomainError> {
        sqlx::query("UPDATE merge_requests SET head_sha = $2 WHERE id = $1")
            .bind(merge_request_id)
            .bind(sha)
            .execute(&self.pool)
            .await
            .map_err(infra)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::merge_request_store::PostgresMergeRequestStore;
    use crate::postgres::test_support::seed_owned_repository;
    use crate::postgres::user_repository::PostgresUserRepository;
    use ferrisgit_domain::merge_request::{MergeRequestStorePort, NewMergeRequest};
    use ferrisgit_domain::repository::{NewRepository, RepositoryStorePort, RepositoryVisibility};
    use ferrisgit_domain::user::{NewUser, UserRepositoryPort};
    use serde_json::json;

    async fn seed_repository(pool: &PgPool) -> Uuid {
        seed_owned_repository(pool, "florian").await.1
    }

    fn new_mr(repository_id: Uuid, author_id: Uuid) -> NewMergeRequest {
        NewMergeRequest {
            repository_id,
            author_id,
            source_branch: "feature".to_string(),
            target_branch: "main".to_string(),
            title: "Add feature".to_string(),
            description: "Some description".to_string(),
        }
    }

    /// Seeds a repository, the `florian` user and one open merge request; returns `(merge_request_id, florian_id)`.
    async fn seed_merge_request(pool: &PgPool) -> (Uuid, Uuid) {
        let repository_id = seed_repository(pool).await;
        let florian_id = PostgresUserRepository::new(pool.clone())
            .find_by_username("florian")
            .await
            .unwrap()
            .unwrap()
            .id;
        let merge_request_id = PostgresMergeRequestStore::new(pool.clone())
            .create(new_mr(repository_id, florian_id))
            .await
            .unwrap()
            .id;
        (merge_request_id, florian_id)
    }

    async fn seed_other_repository_id(pool: &PgPool) -> Uuid {
        let owner_id = PostgresUserRepository::new(pool.clone())
            .find_by_username("florian")
            .await
            .unwrap()
            .unwrap()
            .id;
        let repos = crate::postgres::repository_store::PostgresRepositoryStore::new(pool.clone());
        repos
            .create(
                NewRepository {
                    owner_id,
                    name: "other".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                "path2".to_string(),
            )
            .await
            .unwrap()
            .id
    }

    fn event(
        merge_request_id: Uuid,
        actor_id: Option<Uuid>,
        kind: MergeRequestEventKind,
        payload: serde_json::Value,
    ) -> NewMergeRequestEvent {
        NewMergeRequestEvent {
            merge_request_id,
            actor_id,
            kind,
            payload,
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn recorded_events_are_listed_in_insertion_order_with_their_payload_and_actor(
        pool: PgPool,
    ) {
        let (merge_request_id, florian_id) = seed_merge_request(&pool).await;
        let store = PostgresMergeRequestEventStore::new(pool);

        let first = store
            .record(event(
                merge_request_id,
                Some(florian_id),
                MergeRequestEventKind::TitleChanged,
                json!({"from": "a", "to": "b"}),
            ))
            .await
            .unwrap();
        let second = store
            .record(event(
                merge_request_id,
                None,
                MergeRequestEventKind::Closed,
                json!({}),
            ))
            .await
            .unwrap();

        let listed = store.list(merge_request_id).await.unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, first.id);
        assert_eq!(listed[0].kind, MergeRequestEventKind::TitleChanged);
        assert_eq!(listed[0].actor_id, Some(florian_id));
        assert_eq!(listed[0].payload, json!({"from": "a", "to": "b"}));
        assert_eq!(listed[1].id, second.id);
        assert_eq!(listed[1].kind, MergeRequestEventKind::Closed);
        assert_eq!(listed[1].actor_id, None);
        assert_eq!(listed[1].merge_request_id, merge_request_id);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn listing_another_merge_request_returns_nothing(pool: PgPool) {
        let (merge_request_id, florian_id) = seed_merge_request(&pool).await;
        let other_id = PostgresMergeRequestStore::new(pool.clone())
            .create(NewMergeRequest {
                source_branch: "other".to_string(),
                ..new_mr(seed_other_repository_id(&pool).await, florian_id)
            })
            .await
            .unwrap()
            .id;
        let store = PostgresMergeRequestEventStore::new(pool);
        store
            .record(event(
                merge_request_id,
                Some(florian_id),
                MergeRequestEventKind::Closed,
                json!({}),
            ))
            .await
            .unwrap();

        assert!(store.list(other_id).await.unwrap().is_empty());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn head_sha_is_none_until_set_then_returns_the_stored_value(pool: PgPool) {
        let (merge_request_id, _) = seed_merge_request(&pool).await;
        let store = PostgresMergeRequestEventStore::new(pool);

        assert_eq!(store.head_sha(merge_request_id).await.unwrap(), None);
        store.set_head_sha(merge_request_id, "abc").await.unwrap();
        assert_eq!(
            store.head_sha(merge_request_id).await.unwrap(),
            Some("abc".to_string())
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn head_sha_of_a_missing_merge_request_is_not_found(pool: PgPool) {
        let store = PostgresMergeRequestEventStore::new(pool);

        assert!(matches!(
            store.head_sha(Uuid::new_v4()).await,
            Err(DomainError::NotFound(_))
        ));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn deleting_the_actor_keeps_the_event_with_a_null_actor(pool: PgPool) {
        let (merge_request_id, _) = seed_merge_request(&pool).await;
        let reviewer_id = PostgresUserRepository::new(pool.clone())
            .create(NewUser {
                username: "reviewer".to_string(),
                email: "r@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
            })
            .await
            .unwrap()
            .id;
        let store = PostgresMergeRequestEventStore::new(pool.clone());
        store
            .record(event(
                merge_request_id,
                Some(reviewer_id),
                MergeRequestEventKind::ReviewSubmitted,
                json!({"decision": "approved"}),
            ))
            .await
            .unwrap();

        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(reviewer_id)
            .execute(&pool)
            .await
            .unwrap();

        let listed = store.list(merge_request_id).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].actor_id, None);
        assert_eq!(listed[0].kind, MergeRequestEventKind::ReviewSubmitted);
    }
}
