use crate::error::infra;
use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::notification::{
    NewNotification, Notification, NotificationKind, NotificationStorePort,
};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresNotificationStore {
    pool: PgPool,
}

impl PostgresNotificationStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct Row {
    id: Uuid,
    recipient_id: Uuid,
    kind: String,
    repository_owner: String,
    repository_name: String,
    actor_username: Option<String>,
    merge_request_id: Option<Uuid>,
    merge_request_title: Option<String>,
    pipeline_id: Option<Uuid>,
    commit_sha: Option<String>,
    issue_id: Option<Uuid>,
    issue_number: Option<i32>,
    issue_title: Option<String>,
    role: Option<String>,
    read_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<Row> for Notification {
    type Error = DomainError;
    fn try_from(row: Row) -> Result<Self, DomainError> {
        Ok(Notification {
            id: row.id,
            recipient_id: row.recipient_id,
            kind: NotificationKind::parse(&row.kind)?,
            repository_owner: row.repository_owner,
            repository_name: row.repository_name,
            actor_username: row.actor_username,
            merge_request_id: row.merge_request_id,
            merge_request_title: row.merge_request_title,
            pipeline_id: row.pipeline_id,
            commit_sha: row.commit_sha,
            issue_id: row.issue_id,
            issue_number: row.issue_number,
            issue_title: row.issue_title,
            role: row.role,
            read_at: row.read_at,
            created_at: row.created_at,
        })
    }
}

#[async_trait]
impl NotificationStorePort for PostgresNotificationStore {
    async fn create(&self, n: NewNotification) -> Result<(), DomainError> {
        sqlx::query!(
            "INSERT INTO notifications (recipient_id, kind, repository_owner, repository_name, actor_username, merge_request_id, merge_request_title, pipeline_id, commit_sha, role, issue_id, issue_number, issue_title) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
            n.recipient_id,
            n.kind.as_str(),
            n.repository_owner,
            n.repository_name,
            n.actor_username,
            n.merge_request_id,
            n.merge_request_title,
            n.pipeline_id,
            n.commit_sha,
            n.role,
            n.issue_id,
            n.issue_number,
            n.issue_title,
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }

    async fn list_for_recipient(
        &self,
        recipient_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Notification>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT id, recipient_id, kind, repository_owner, repository_name, actor_username, merge_request_id, merge_request_title, pipeline_id, commit_sha, role, issue_id, issue_number, issue_title, read_at, created_at \
             FROM notifications WHERE recipient_id = $1 ORDER BY created_at DESC, id DESC LIMIT $2",
            recipient_id,
            limit,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        rows.into_iter().map(TryFrom::try_from).collect()
    }

    async fn unread_count(&self, recipient_id: Uuid) -> Result<i64, DomainError> {
        let row = sqlx::query!("SELECT count(*) AS count FROM notifications WHERE recipient_id = $1 AND read_at IS NULL", recipient_id)
            .fetch_one(&self.pool)
            .await
            .map_err(infra)?;
        Ok(row.count.unwrap_or(0))
    }

    async fn mark_read(
        &self,
        notification_id: Uuid,
        recipient_id: Uuid,
    ) -> Result<(), DomainError> {
        sqlx::query!("UPDATE notifications SET read_at = now() WHERE id = $1 AND recipient_id = $2 AND read_at IS NULL", notification_id, recipient_id)
            .execute(&self.pool)
            .await
            .map_err(infra)?;
        Ok(())
    }

    async fn mark_all_read(&self, recipient_id: Uuid) -> Result<(), DomainError> {
        sqlx::query!(
            "UPDATE notifications SET read_at = now() WHERE recipient_id = $1 AND read_at IS NULL",
            recipient_id
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::test_support::seed_user;

    fn minimal(recipient_id: Uuid) -> NewNotification {
        NewNotification {
            recipient_id,
            kind: NotificationKind::CollaboratorAdded,
            repository_owner: "owner".to_string(),
            repository_name: "hello".to_string(),
            actor_username: Some("actor".to_string()),
            merge_request_id: None,
            merge_request_title: None,
            pipeline_id: None,
            commit_sha: None,
            issue_id: None,
            issue_number: None,
            issue_title: None,
            role: Some("reader".to_string()),
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn creating_then_listing_a_notification_returns_it(pool: PgPool) {
        let recipient_id = seed_user(&pool, "alice").await;
        let store = PostgresNotificationStore::new(pool);

        store.create(minimal(recipient_id)).await.unwrap();

        let listed = store.list_for_recipient(recipient_id, 50).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].kind, NotificationKind::CollaboratorAdded);
        assert_eq!(listed[0].role.as_deref(), Some("reader"));
        assert!(listed[0].read_at.is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn unread_count_reflects_unread_notifications_only(pool: PgPool) {
        let recipient_id = seed_user(&pool, "alice").await;
        let store = PostgresNotificationStore::new(pool);
        store.create(minimal(recipient_id)).await.unwrap();
        store.create(minimal(recipient_id)).await.unwrap();

        assert_eq!(store.unread_count(recipient_id).await.unwrap(), 2);

        let listed = store.list_for_recipient(recipient_id, 50).await.unwrap();
        store.mark_read(listed[0].id, recipient_id).await.unwrap();

        assert_eq!(store.unread_count(recipient_id).await.unwrap(), 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn mark_read_cannot_affect_another_recipients_notification(pool: PgPool) {
        let recipient_id = seed_user(&pool, "alice").await;
        let stranger_id = seed_user(&pool, "stranger").await;
        let store = PostgresNotificationStore::new(pool);
        store.create(minimal(recipient_id)).await.unwrap();
        let listed = store.list_for_recipient(recipient_id, 50).await.unwrap();

        store.mark_read(listed[0].id, stranger_id).await.unwrap();

        let still = store.list_for_recipient(recipient_id, 50).await.unwrap();
        assert!(
            still[0].read_at.is_none(),
            "a notification must not be markable read by anyone other than its own recipient"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn mark_all_read_clears_every_unread_notification_for_that_recipient(pool: PgPool) {
        let recipient_id = seed_user(&pool, "alice").await;
        let store = PostgresNotificationStore::new(pool);
        store.create(minimal(recipient_id)).await.unwrap();
        store.create(minimal(recipient_id)).await.unwrap();

        store.mark_all_read(recipient_id).await.unwrap();

        assert_eq!(store.unread_count(recipient_id).await.unwrap(), 0);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_for_recipient_returns_most_recent_first_up_to_the_limit(pool: PgPool) {
        let recipient_id = seed_user(&pool, "alice").await;
        let store = PostgresNotificationStore::new(pool);
        for _ in 0..3 {
            store.create(minimal(recipient_id)).await.unwrap();
        }

        let listed = store.list_for_recipient(recipient_id, 2).await.unwrap();
        assert_eq!(listed.len(), 2);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn issue_context_round_trips_through_create_and_list(pool: PgPool) {
        let recipient_id = seed_user(&pool, "alice").await;
        let author_id = seed_user(&pool, "bob").await;

        let repo_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO repositories (id, owner_id, name, group_id, description, disk_path, visibility) VALUES ($1, $2, $3, $4, $5, $6, $7)",
            repo_id,
            author_id,
            "test-repo",
            None::<Uuid>,
            "test repo",
            "/test-repo.git",
            "private",
        )
        .execute(&pool)
        .await
        .unwrap();

        let issue_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO issues (id, repository_id, number, author_id, title, description, status, kind) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            issue_id,
            repo_id,
            7,
            author_id,
            "Fix the thing",
            "description",
            "todo",
            "bug",
        )
        .execute(&pool)
        .await
        .unwrap();

        let store = PostgresNotificationStore::new(pool);
        store
            .create(NewNotification {
                issue_id: Some(issue_id),
                issue_number: Some(7),
                issue_title: Some("Fix the thing".to_string()),
                ..minimal(recipient_id)
            })
            .await
            .unwrap();

        let listed = store.list_for_recipient(recipient_id, 50).await.unwrap();
        assert_eq!(listed[0].issue_id, Some(issue_id));
        assert_eq!(listed[0].issue_number, Some(7));
        assert_eq!(listed[0].issue_title.as_deref(), Some("Fix the thing"));
    }
}
