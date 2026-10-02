use crate::error::infra;
use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::label::{Label, LabelStorePort, NewLabel};
use sqlx::PgPool;
use std::collections::HashSet;
use uuid::Uuid;

/// Both join tables are keyed on `(owner_id, label_id)`, so a repeated label id would be a primary-key
/// violation (500). Repeats collapse, keeping first-seen order.
fn dedup_label_ids(label_ids: &[Uuid]) -> Vec<Uuid> {
    let mut seen = HashSet::with_capacity(label_ids.len());
    label_ids
        .iter()
        .copied()
        .filter(|id| seen.insert(*id))
        .collect()
}

pub struct PostgresLabelStore {
    pool: PgPool,
}

impl PostgresLabelStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct Row {
    id: Uuid,
    name: String,
    color: String,
    repository_id: Option<Uuid>,
    group_id: Option<Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<Row> for Label {
    fn from(row: Row) -> Self {
        Label {
            id: row.id,
            name: row.name,
            color: row.color,
            repository_id: row.repository_id,
            group_id: row.group_id,
            created_at: row.created_at,
        }
    }
}

#[async_trait]
impl LabelStorePort for PostgresLabelStore {
    async fn create(&self, new_label: NewLabel) -> Result<Label, DomainError> {
        let row = sqlx::query_as!(
            Row,
            "INSERT INTO labels (name, color, repository_id, group_id) VALUES ($1, $2, $3, $4) \
             RETURNING id, name, color, repository_id, group_id, created_at",
            new_label.name,
            new_label.color,
            new_label.repository_id,
            new_label.group_id,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(infra)?;
        Ok(row.into())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Label>, DomainError> {
        let row = sqlx::query_as!(
            Row,
            "SELECT id, name, color, repository_id, group_id, created_at FROM labels WHERE id = $1",
            id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;
        Ok(row.map(Into::into))
    }

    async fn update(&self, id: Uuid, name: String, color: String) -> Result<(), DomainError> {
        sqlx::query!(
            "UPDATE labels SET name = $1, color = $2 WHERE id = $3",
            name,
            color,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        sqlx::query!("DELETE FROM labels WHERE id = $1", id)
            .execute(&self.pool)
            .await
            .map_err(infra)?;
        Ok(())
    }

    async fn list_for_repository(&self, repository_id: Uuid) -> Result<Vec<Label>, DomainError> {
        let rows = sqlx::query_as!(Row, "SELECT id, name, color, repository_id, group_id, created_at FROM labels WHERE repository_id = $1 ORDER BY name ASC", repository_id)
            .fetch_all(&self.pool)
            .await
            .map_err(infra)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn list_for_group(&self, group_id: Uuid) -> Result<Vec<Label>, DomainError> {
        let rows = sqlx::query_as!(Row, "SELECT id, name, color, repository_id, group_id, created_at FROM labels WHERE group_id = $1 ORDER BY name ASC", group_id)
            .fetch_all(&self.pool)
            .await
            .map_err(infra)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn set_labels_for_issue(
        &self,
        issue_id: Uuid,
        label_ids: &[Uuid],
    ) -> Result<(), DomainError> {
        let mut tx = self.pool.begin().await.map_err(infra)?;
        sqlx::query!("DELETE FROM issue_labels WHERE issue_id = $1", issue_id)
            .execute(&mut *tx)
            .await
            .map_err(infra)?;
        for label_id in dedup_label_ids(label_ids) {
            sqlx::query!(
                "INSERT INTO issue_labels (issue_id, label_id) VALUES ($1, $2)",
                issue_id,
                label_id
            )
            .execute(&mut *tx)
            .await
            .map_err(infra)?;
        }
        tx.commit().await.map_err(infra)?;
        Ok(())
    }

    async fn list_for_issue(&self, issue_id: Uuid) -> Result<Vec<Label>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT l.id, l.name, l.color, l.repository_id, l.group_id, l.created_at FROM labels l \
             JOIN issue_labels il ON il.label_id = l.id WHERE il.issue_id = $1 ORDER BY l.name ASC",
            issue_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn list_for_issues(&self, issue_ids: &[Uuid]) -> Result<Vec<(Uuid, Label)>, DomainError> {
        struct PairRow {
            issue_id: Uuid,
            id: Uuid,
            name: String,
            color: String,
            repository_id: Option<Uuid>,
            group_id: Option<Uuid>,
            created_at: chrono::DateTime<chrono::Utc>,
        }
        let rows = sqlx::query_as!(
            PairRow,
            "SELECT il.issue_id, l.id, l.name, l.color, l.repository_id, l.group_id, l.created_at FROM labels l \
             JOIN issue_labels il ON il.label_id = l.id WHERE il.issue_id = ANY($1::uuid[])",
            issue_ids,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    r.issue_id,
                    Label {
                        id: r.id,
                        name: r.name,
                        color: r.color,
                        repository_id: r.repository_id,
                        group_id: r.group_id,
                        created_at: r.created_at,
                    },
                )
            })
            .collect())
    }

    async fn set_labels_for_merge_request(
        &self,
        merge_request_id: Uuid,
        label_ids: &[Uuid],
    ) -> Result<(), DomainError> {
        let mut tx = self.pool.begin().await.map_err(infra)?;
        sqlx::query!(
            "DELETE FROM merge_request_labels WHERE merge_request_id = $1",
            merge_request_id
        )
        .execute(&mut *tx)
        .await
        .map_err(infra)?;
        for label_id in dedup_label_ids(label_ids) {
            sqlx::query!(
                "INSERT INTO merge_request_labels (merge_request_id, label_id) VALUES ($1, $2)",
                merge_request_id,
                label_id
            )
            .execute(&mut *tx)
            .await
            .map_err(infra)?;
        }
        tx.commit().await.map_err(infra)?;
        Ok(())
    }

    async fn list_for_merge_request(
        &self,
        merge_request_id: Uuid,
    ) -> Result<Vec<Label>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT l.id, l.name, l.color, l.repository_id, l.group_id, l.created_at FROM labels l \
             JOIN merge_request_labels mrl ON mrl.label_id = l.id WHERE mrl.merge_request_id = $1 ORDER BY l.name ASC",
            merge_request_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn list_for_merge_requests(
        &self,
        merge_request_ids: &[Uuid],
    ) -> Result<Vec<(Uuid, Label)>, DomainError> {
        struct PairRow {
            merge_request_id: Uuid,
            id: Uuid,
            name: String,
            color: String,
            repository_id: Option<Uuid>,
            group_id: Option<Uuid>,
            created_at: chrono::DateTime<chrono::Utc>,
        }
        let rows = sqlx::query_as!(
            PairRow,
            "SELECT mrl.merge_request_id, l.id, l.name, l.color, l.repository_id, l.group_id, l.created_at FROM labels l \
             JOIN merge_request_labels mrl ON mrl.label_id = l.id WHERE mrl.merge_request_id = ANY($1::uuid[])",
            merge_request_ids,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    r.merge_request_id,
                    Label {
                        id: r.id,
                        name: r.name,
                        color: r.color,
                        repository_id: r.repository_id,
                        group_id: r.group_id,
                        created_at: r.created_at,
                    },
                )
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::test_support::seed_owned_repository;

    async fn seed_repository(pool: &PgPool) -> Uuid {
        seed_owned_repository(pool, "owner").await.1
    }

    async fn seed_issue(pool: &PgPool, repository_id: Uuid) -> Uuid {
        let author_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO users (id, username, email, password_hash) VALUES ($1, $2, $3, $4)",
            author_id,
            format!("author-{author_id}"),
            format!("{author_id}@example.com"),
            "not-a-real-hash"
        )
        .execute(pool)
        .await
        .unwrap();
        let issue_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO issues (id, repository_id, number, author_id, title, description, status, kind) VALUES ($1, $2, 1, $3, 't', '', 'todo', 'bug')",
            issue_id,
            repository_id,
            author_id,
        )
        .execute(pool)
        .await
        .unwrap();
        issue_id
    }

    async fn seed_merge_request(pool: &PgPool, repository_id: Uuid) -> Uuid {
        let author_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO users (id, username, email, password_hash) VALUES ($1, $2, $3, $4)",
            author_id,
            format!("mr-author-{author_id}"),
            format!("{author_id}@example.com"),
            "not-a-real-hash"
        )
        .execute(pool)
        .await
        .unwrap();
        let merge_request_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO merge_requests (id, repository_id, author_id, source_branch, target_branch, title, description) VALUES ($1, $2, $3, 'feature', 'main', 't', '')",
            merge_request_id,
            repository_id,
            author_id,
        )
        .execute(pool)
        .await
        .unwrap();
        merge_request_id
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_label_round_trips_through_create_and_find(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let store = PostgresLabelStore::new(pool);
        let created = store
            .create(NewLabel {
                name: "Bug".to_string(),
                color: "#dc2626".to_string(),
                repository_id: Some(repository_id),
                group_id: None,
            })
            .await
            .unwrap();
        let found = store.find_by_id(created.id).await.unwrap().unwrap();
        assert_eq!(found.name, "Bug");
        assert_eq!(found.color, "#dc2626");
        assert_eq!(found.repository_id, Some(repository_id));
        assert_eq!(found.group_id, None);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn setting_an_issues_labels_replaces_the_previous_set(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let issue_id = seed_issue(&pool, repository_id).await;
        let store = PostgresLabelStore::new(pool);
        let bug = store
            .create(NewLabel {
                name: "Bug".to_string(),
                color: "#dc2626".to_string(),
                repository_id: Some(repository_id),
                group_id: None,
            })
            .await
            .unwrap();
        let feature = store
            .create(NewLabel {
                name: "Feature".to_string(),
                color: "#16a34a".to_string(),
                repository_id: Some(repository_id),
                group_id: None,
            })
            .await
            .unwrap();

        store
            .set_labels_for_issue(issue_id, &[bug.id, feature.id])
            .await
            .unwrap();
        assert_eq!(store.list_for_issue(issue_id).await.unwrap().len(), 2);

        store
            .set_labels_for_issue(issue_id, &[feature.id])
            .await
            .unwrap();
        let labels = store.list_for_issue(issue_id).await.unwrap();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].id, feature.id);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn deleting_a_label_cascades_out_of_the_issue_join_table(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let issue_id = seed_issue(&pool, repository_id).await;
        let store = PostgresLabelStore::new(pool);
        let bug = store
            .create(NewLabel {
                name: "Bug".to_string(),
                color: "#dc2626".to_string(),
                repository_id: Some(repository_id),
                group_id: None,
            })
            .await
            .unwrap();
        store
            .set_labels_for_issue(issue_id, &[bug.id])
            .await
            .unwrap();

        store.delete(bug.id).await.unwrap();

        assert!(store.list_for_issue(issue_id).await.unwrap().is_empty());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn deleting_a_label_cascades_out_of_the_merge_request_join_table(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let merge_request_id = seed_merge_request(&pool, repository_id).await;
        let store = PostgresLabelStore::new(pool);
        let bug = store
            .create(NewLabel {
                name: "Bug".to_string(),
                color: "#dc2626".to_string(),
                repository_id: Some(repository_id),
                group_id: None,
            })
            .await
            .unwrap();
        store
            .set_labels_for_merge_request(merge_request_id, &[bug.id])
            .await
            .unwrap();
        assert_eq!(
            store
                .list_for_merge_request(merge_request_id)
                .await
                .unwrap()
                .len(),
            1,
            "precondition: the label must actually be attached before it is deleted"
        );

        store.delete(bug.id).await.unwrap();

        assert!(
            store
                .list_for_merge_request(merge_request_id)
                .await
                .unwrap()
                .is_empty()
        );
    }

    /// Regression: repeating a label id in one request raised a primary-key violation (500).
    #[sqlx::test(migrations = "../../migrations")]
    async fn setting_duplicate_label_ids_is_accepted_and_attaches_the_label_once(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let issue_id = seed_issue(&pool, repository_id).await;
        let merge_request_id = seed_merge_request(&pool, repository_id).await;
        let store = PostgresLabelStore::new(pool);
        let bug = store
            .create(NewLabel {
                name: "Bug".to_string(),
                color: "#dc2626".to_string(),
                repository_id: Some(repository_id),
                group_id: None,
            })
            .await
            .unwrap();
        let feature = store
            .create(NewLabel {
                name: "Feature".to_string(),
                color: "#16a34a".to_string(),
                repository_id: Some(repository_id),
                group_id: None,
            })
            .await
            .unwrap();

        store
            .set_labels_for_issue(issue_id, &[bug.id, feature.id, bug.id])
            .await
            .unwrap();
        let issue_labels = store.list_for_issue(issue_id).await.unwrap();
        assert_eq!(
            issue_labels.len(),
            2,
            "the repeated id must collapse, not duplicate the row: {issue_labels:?}"
        );

        store
            .set_labels_for_merge_request(merge_request_id, &[bug.id, bug.id, feature.id])
            .await
            .unwrap();
        let mr_labels = store
            .list_for_merge_request(merge_request_id)
            .await
            .unwrap();
        assert_eq!(
            mr_labels.len(),
            2,
            "the repeated id must collapse, not duplicate the row: {mr_labels:?}"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_label_with_both_repository_and_group_scope_is_rejected_by_the_db(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let group_id = Uuid::new_v4();
        // No group row is seeded. The CHECK fires before the FK would, so this still tests the XOR constraint.
        let result = sqlx::query!(
            "INSERT INTO labels (name, color, repository_id, group_id) VALUES ($1, $2, $3, $4)",
            "Bug",
            "#dc2626",
            repository_id,
            group_id
        )
        .execute(&pool)
        .await;
        assert!(
            result.is_err(),
            "a label naming both a repository and a group must violate labels_scope_xor"
        );
    }
}
