use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::milestone::{Milestone, MilestoneState, MilestoneStorePort, NewMilestone};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresMilestoneStore {
    pool: PgPool,
}

impl PostgresMilestoneStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct Row {
    id: Uuid,
    title: String,
    description: String,
    due_date: Option<DateTime<Utc>>,
    state: String,
    repository_id: Option<Uuid>,
    group_id: Option<Uuid>,
    created_at: DateTime<Utc>,
}

impl TryFrom<Row> for Milestone {
    type Error = DomainError;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        Ok(Milestone {
            id: row.id,
            title: row.title,
            description: row.description,
            due_date: row.due_date,
            state: MilestoneState::parse(&row.state)?,
            repository_id: row.repository_id,
            group_id: row.group_id,
            created_at: row.created_at,
        })
    }
}

#[async_trait]
impl MilestoneStorePort for PostgresMilestoneStore {
    async fn create(&self, new_milestone: NewMilestone) -> Result<Milestone, DomainError> {
        let row = sqlx::query_as!(
            Row,
            "INSERT INTO milestones (title, description, due_date, repository_id, group_id) VALUES ($1, $2, $3, $4, $5) \
             RETURNING id, title, description, due_date, state, repository_id, group_id, created_at",
            new_milestone.title,
            new_milestone.description,
            new_milestone.due_date,
            new_milestone.repository_id,
            new_milestone.group_id,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        row.try_into()
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Milestone>, DomainError> {
        let row = sqlx::query_as!(Row, "SELECT id, title, description, due_date, state, repository_id, group_id, created_at FROM milestones WHERE id = $1", id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        row.map(TryInto::try_into).transpose()
    }

    async fn update(
        &self,
        id: Uuid,
        title: String,
        description: String,
        due_date: Option<DateTime<Utc>>,
        state: MilestoneState,
    ) -> Result<(), DomainError> {
        sqlx::query!(
            "UPDATE milestones SET title = $1, description = $2, due_date = $3, state = $4 WHERE id = $5",
            title,
            description,
            due_date,
            state.as_str(),
            id,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        sqlx::query!("DELETE FROM milestones WHERE id = $1", id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(())
    }

    async fn list_for_repository(
        &self,
        repository_id: Uuid,
    ) -> Result<Vec<Milestone>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT id, title, description, due_date, state, repository_id, group_id, created_at FROM milestones WHERE repository_id = $1 ORDER BY title ASC",
            repository_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn list_for_group(&self, group_id: Uuid) -> Result<Vec<Milestone>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT id, title, description, due_date, state, repository_id, group_id, created_at FROM milestones WHERE group_id = $1 ORDER BY title ASC",
            group_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        rows.into_iter().map(TryInto::try_into).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn seed_repository(pool: &PgPool) -> Uuid {
        let owner_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO users (id, username, email, password_hash) VALUES ($1, $2, $3, $4)",
            owner_id,
            "owner",
            "owner@example.com",
            "not-a-real-hash"
        )
        .execute(pool)
        .await
        .unwrap();
        let repository_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO repositories (id, owner_id, name, disk_path) VALUES ($1, $2, $3, $4)",
            repository_id,
            owner_id,
            "hello",
            "hello.git"
        )
        .execute(pool)
        .await
        .unwrap();
        repository_id
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_milestone_round_trips_through_create_and_find(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let store = PostgresMilestoneStore::new(pool);
        let created = store
            .create(NewMilestone {
                title: "v1.0".to_string(),
                description: "First release".to_string(),
                due_date: None,
                repository_id: Some(repository_id),
                group_id: None,
            })
            .await
            .unwrap();

        let found = store.find_by_id(created.id).await.unwrap().unwrap();
        assert_eq!(found.title, "v1.0");
        assert_eq!(found.description, "First release");
        assert_eq!(found.due_date, None);
        assert_eq!(found.state, MilestoneState::Open);
        assert_eq!(found.repository_id, Some(repository_id));
        assert_eq!(found.group_id, None);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn updating_a_milestone_changes_its_fields(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let store = PostgresMilestoneStore::new(pool);
        let created = store
            .create(NewMilestone {
                title: "v1.0".to_string(),
                description: "First release".to_string(),
                due_date: None,
                repository_id: Some(repository_id),
                group_id: None,
            })
            .await
            .unwrap();
        let due_date = DateTime::parse_from_rfc3339("2026-12-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        store
            .update(
                created.id,
                "v1.0-final".to_string(),
                "Final release".to_string(),
                Some(due_date),
                MilestoneState::Closed,
            )
            .await
            .unwrap();

        let updated = store.find_by_id(created.id).await.unwrap().unwrap();
        assert_eq!(updated.title, "v1.0-final");
        assert_eq!(updated.description, "Final release");
        assert_eq!(updated.due_date, Some(due_date));
        assert_eq!(updated.state, MilestoneState::Closed);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn deleting_a_milestone_removes_the_row(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let store = PostgresMilestoneStore::new(pool);
        let created = store
            .create(NewMilestone {
                title: "v1.0".to_string(),
                description: String::new(),
                due_date: None,
                repository_id: Some(repository_id),
                group_id: None,
            })
            .await
            .unwrap();

        store.delete(created.id).await.unwrap();

        assert!(store.find_by_id(created.id).await.unwrap().is_none());
    }

    /// Deleting a milestone must detach, not delete, referencing issues/MRs (`ON DELETE SET NULL`).
    #[sqlx::test(migrations = "../../migrations")]
    async fn deleting_a_milestone_nulls_it_out_of_issues_and_merge_requests_without_deleting_them(
        pool: PgPool,
    ) {
        let repository_id = seed_repository(&pool).await;
        let author_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO users (id, username, email, password_hash) VALUES ($1, $2, $3, $4)",
            author_id,
            "author",
            "author@example.com",
            "not-a-real-hash"
        )
        .execute(&pool)
        .await
        .unwrap();

        let store = PostgresMilestoneStore::new(pool.clone());
        let milestone = store
            .create(NewMilestone {
                title: "v1.0".to_string(),
                description: String::new(),
                due_date: None,
                repository_id: Some(repository_id),
                group_id: None,
            })
            .await
            .unwrap();

        let issue_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO issues (id, repository_id, number, author_id, title, description, status, kind, milestone_id) VALUES ($1, $2, 1, $3, 'an issue', '', 'todo', 'bug', $4)",
            issue_id,
            repository_id,
            author_id,
            milestone.id,
        )
        .execute(&pool)
        .await
        .unwrap();

        let merge_request_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO merge_requests (id, repository_id, author_id, source_branch, target_branch, title, description, milestone_id) VALUES ($1, $2, $3, 'feature', 'main', 'an MR', '', $4)",
            merge_request_id,
            repository_id,
            author_id,
            milestone.id,
        )
        .execute(&pool)
        .await
        .unwrap();

        let issue_before = sqlx::query!("SELECT milestone_id FROM issues WHERE id = $1", issue_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(issue_before.milestone_id, Some(milestone.id));
        let mr_before = sqlx::query!(
            "SELECT milestone_id FROM merge_requests WHERE id = $1",
            merge_request_id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(mr_before.milestone_id, Some(milestone.id));

        store.delete(milestone.id).await.unwrap();

        assert!(
            store.find_by_id(milestone.id).await.unwrap().is_none(),
            "the milestone itself must be gone"
        );

        let issue_after = sqlx::query!(
            "SELECT id, title, milestone_id FROM issues WHERE id = $1",
            issue_id
        )
        .fetch_optional(&pool)
        .await
        .unwrap()
        .expect(
            "the issue must survive its milestone being deleted — ON DELETE SET NULL, not CASCADE",
        );
        assert_eq!(
            issue_after.title, "an issue",
            "the surviving issue must be intact, not a husk"
        );
        assert_eq!(
            issue_after.milestone_id, None,
            "the deleted milestone must be nulled out of the issue"
        );

        let mr_after = sqlx::query!("SELECT id, title, milestone_id FROM merge_requests WHERE id = $1", merge_request_id)
            .fetch_optional(&pool)
            .await
            .unwrap()
            .expect("the merge request must survive its milestone being deleted — ON DELETE SET NULL, not CASCADE");
        assert_eq!(
            mr_after.title, "an MR",
            "the surviving merge request must be intact, not a husk"
        );
        assert_eq!(
            mr_after.milestone_id, None,
            "the deleted milestone must be nulled out of the merge request"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_milestone_with_both_repository_and_group_scope_is_rejected_by_the_db(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let group_id = Uuid::new_v4();
        // No group row is seeded. The CHECK fires before the FK would, so this still tests the XOR constraint.
        let result = sqlx::query!(
            "INSERT INTO milestones (title, repository_id, group_id) VALUES ($1, $2, $3)",
            "v1.0",
            repository_id,
            group_id
        )
        .execute(&pool)
        .await;
        assert!(
            result.is_err(),
            "a milestone naming both a repository and a group must violate milestones_scope_xor"
        );
    }
}
