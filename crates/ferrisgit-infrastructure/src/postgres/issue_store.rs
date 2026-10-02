use std::collections::HashMap;

use crate::error::infra;
use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::issue::{
    Issue, IssueComment, IssueKind, IssueStatus, IssueStorePort, NewIssue, NewIssueComment,
};
use ferrisgit_domain::issue_comment::IssueCommentPort;
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresIssueStore {
    pool: PgPool,
}

impl PostgresIssueStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct Row {
    id: Uuid,
    repository_id: Uuid,
    number: i32,
    author_id: Uuid,
    assignee_id: Option<Uuid>,
    milestone_id: Option<Uuid>,
    title: String,
    description: String,
    status: String,
    kind: String,
    parent_issue_id: Option<Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
    closed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl TryFrom<Row> for Issue {
    type Error = DomainError;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        Ok(Issue {
            id: row.id,
            repository_id: row.repository_id,
            number: row.number,
            author_id: row.author_id,
            assignee_id: row.assignee_id,
            milestone_id: row.milestone_id,
            title: row.title,
            description: row.description,
            status: IssueStatus::parse(&row.status)?,
            kind: IssueKind::parse(&row.kind)?,
            parent_issue_id: row.parent_issue_id,
            created_at: row.created_at,
            closed_at: row.closed_at,
        })
    }
}

#[async_trait]
impl IssueStorePort for PostgresIssueStore {
    async fn create(&self, new_issue: NewIssue) -> Result<Issue, DomainError> {
        let mut tx = self.pool.begin().await.map_err(infra)?;
        let number: i32 = sqlx::query_scalar!(
            "UPDATE repositories SET next_issue_number = next_issue_number + 1 WHERE id = $1 RETURNING next_issue_number - 1 AS \"number!\"",
            new_issue.repository_id,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(infra)?;

        let row = sqlx::query_as!(
            Row,
            "INSERT INTO issues (repository_id, number, author_id, title, description, status, kind, parent_issue_id) \
             VALUES ($1, $2, $3, $4, $5, 'todo', $6, $7) \
             RETURNING id, repository_id, number, author_id, assignee_id, milestone_id, title, description, status, kind, parent_issue_id, created_at, closed_at",
            new_issue.repository_id,
            number,
            new_issue.author_id,
            new_issue.title,
            new_issue.description,
            new_issue.kind.as_str(),
            new_issue.parent_issue_id,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(infra)?;

        tx.commit().await.map_err(infra)?;
        row.try_into()
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Issue>, DomainError> {
        let row = sqlx::query_as!(
            Row,
            "SELECT id, repository_id, number, author_id, assignee_id, milestone_id, title, description, status, kind, parent_issue_id, created_at, closed_at \
             FROM issues WHERE id = $1",
            id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;
        row.map(TryInto::try_into).transpose()
    }

    async fn find_by_number(
        &self,
        repository_id: Uuid,
        number: i32,
    ) -> Result<Option<Issue>, DomainError> {
        let row = sqlx::query_as!(
            Row,
            "SELECT id, repository_id, number, author_id, assignee_id, milestone_id, title, description, status, kind, parent_issue_id, created_at, closed_at \
             FROM issues WHERE repository_id = $1 AND number = $2",
            repository_id,
            number,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;
        row.map(TryInto::try_into).transpose()
    }

    async fn list_for_repository(&self, repository_id: Uuid) -> Result<Vec<Issue>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT id, repository_id, number, author_id, assignee_id, milestone_id, title, description, status, kind, parent_issue_id, created_at, closed_at \
             FROM issues WHERE repository_id = $1 ORDER BY number ASC",
            repository_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn list_for_repository_filtered(
        &self,
        repository_id: Uuid,
        label_ids: Option<Vec<Uuid>>,
        milestone_id: Option<Uuid>,
    ) -> Result<Vec<Issue>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT DISTINCT i.id, i.repository_id, i.number, i.author_id, i.assignee_id, i.milestone_id, i.title, i.description, i.status, i.kind, i.parent_issue_id, i.created_at, i.closed_at \
             FROM issues i \
             LEFT JOIN issue_labels il ON il.issue_id = i.id \
             WHERE i.repository_id = $1 \
               AND ($2::uuid[] IS NULL OR il.label_id = ANY($2::uuid[])) \
               AND ($3::uuid IS NULL OR i.milestone_id = $3) \
             ORDER BY i.number ASC",
            repository_id,
            label_ids.as_deref(),
            milestone_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn update_fields(
        &self,
        id: Uuid,
        title: String,
        description: String,
        kind: IssueKind,
    ) -> Result<(), DomainError> {
        sqlx::query!(
            "UPDATE issues SET title = $1, description = $2, kind = $3 WHERE id = $4",
            title,
            description,
            kind.as_str(),
            id
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }

    async fn update_status(&self, id: Uuid, status: IssueStatus) -> Result<(), DomainError> {
        sqlx::query!(
            "UPDATE issues SET status = $1, closed_at = CASE WHEN $1 = 'done' THEN COALESCE(closed_at, now()) ELSE NULL END WHERE id = $2",
            status.as_str(),
            id
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }

    async fn assign(&self, id: Uuid, assignee_id: Option<Uuid>) -> Result<(), DomainError> {
        sqlx::query!(
            "UPDATE issues SET assignee_id = $1 WHERE id = $2",
            assignee_id,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }

    async fn set_milestone(&self, id: Uuid, milestone_id: Option<Uuid>) -> Result<(), DomainError> {
        sqlx::query!(
            "UPDATE issues SET milestone_id = $1 WHERE id = $2",
            milestone_id,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }

    async fn close(&self, id: Uuid) -> Result<(), DomainError> {
        sqlx::query!(
            "UPDATE issues SET status = 'done', closed_at = now() WHERE id = $1",
            id
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }

    async fn reopen(&self, id: Uuid) -> Result<(), DomainError> {
        sqlx::query!(
            "UPDATE issues SET status = 'todo', closed_at = NULL WHERE id = $1",
            id
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }

    /// Same search semantics as `PostgresRepositoryStore::search`.
    async fn search(
        &self,
        repository_ids: &[Uuid],
        query: &str,
        limit: i64,
    ) -> Result<Vec<Issue>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT id, repository_id, number, author_id, assignee_id, milestone_id, title, description, status, kind, parent_issue_id, created_at, closed_at FROM issues \
             WHERE repository_id = ANY($1::uuid[]) AND search_vector @@ websearch_to_tsquery('simple', $2) \
             ORDER BY ts_rank_cd(search_vector, websearch_to_tsquery('simple', $2)) DESC, id \
             LIMIT $3",
            repository_ids,
            query,
            limit,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        rows.into_iter().map(Issue::try_from).collect()
    }

    async fn list_assigned_to(
        &self,
        user_id: Uuid,
        repository_ids: &[Uuid],
        limit: i64,
    ) -> Result<Vec<Issue>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT id, repository_id, number, author_id, assignee_id, milestone_id, title, description, status, kind, parent_issue_id, created_at, closed_at FROM issues \
             WHERE repository_id = ANY($1::uuid[]) AND assignee_id = $2 AND status != 'done' \
             ORDER BY created_at DESC, id LIMIT $3",
            repository_ids,
            user_id,
            limit,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        rows.into_iter().map(Issue::try_from).collect()
    }

    async fn list_authored_by(
        &self,
        user_id: Uuid,
        repository_ids: &[Uuid],
        limit: i64,
    ) -> Result<Vec<Issue>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT id, repository_id, number, author_id, assignee_id, milestone_id, title, description, status, kind, parent_issue_id, created_at, closed_at FROM issues \
             WHERE repository_id = ANY($1::uuid[]) AND author_id = $2 AND status != 'done' \
             ORDER BY created_at DESC, id LIMIT $3",
            repository_ids,
            user_id,
            limit,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        rows.into_iter().map(Issue::try_from).collect()
    }
}

#[async_trait]
impl IssueCommentPort for PostgresIssueStore {
    async fn add_comment(&self, new_comment: NewIssueComment) -> Result<IssueComment, DomainError> {
        let row = sqlx::query_as!(
            IssueComment,
            "INSERT INTO issue_comments (issue_id, author_id, body) VALUES ($1, $2, $3) RETURNING id, issue_id, author_id, body, created_at",
            new_comment.issue_id,
            new_comment.author_id,
            new_comment.body,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(infra)?;
        Ok(row)
    }

    async fn list_comments(&self, issue_id: Uuid) -> Result<Vec<IssueComment>, DomainError> {
        let rows = sqlx::query_as!(
            IssueComment,
            "SELECT id, issue_id, author_id, body, created_at FROM issue_comments WHERE issue_id = $1 ORDER BY created_at ASC",
            issue_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows)
    }

    async fn comment_counts(&self, issue_ids: &[Uuid]) -> Result<HashMap<Uuid, i64>, DomainError> {
        if issue_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query_as::<_, (Uuid, i64)>("SELECT issue_id, COUNT(*)::bigint FROM issue_comments WHERE issue_id = ANY($1) GROUP BY issue_id")
            .bind(issue_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(infra)?;
        Ok(rows.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::test_support::{seed_repository, seed_user};
    use sqlx::PgPool;

    #[sqlx::test(migrations = "../../migrations")]
    async fn issues_in_the_same_repository_get_sequential_numbers_starting_at_1(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresIssueStore::new(pool);

        let first = store
            .create(NewIssue {
                repository_id,
                author_id: owner_id,
                title: "first".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();
        let second = store
            .create(NewIssue {
                repository_id,
                author_id: owner_id,
                title: "second".to_string(),
                description: String::new(),
                kind: IssueKind::Task,
                parent_issue_id: None,
            })
            .await
            .unwrap();

        assert_eq!(first.number, 1);
        assert_eq!(second.number, 2);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn issue_numbers_are_scoped_per_repository_not_global(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let repo_a = seed_repository(&pool, owner_id, "repo-a").await;
        let repo_b = seed_repository(&pool, owner_id, "repo-b").await;
        let store = PostgresIssueStore::new(pool);

        let in_a = store
            .create(NewIssue {
                repository_id: repo_a,
                author_id: owner_id,
                title: "a".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();
        let in_b = store
            .create(NewIssue {
                repository_id: repo_b,
                author_id: owner_id,
                title: "b".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();

        assert_eq!(
            in_a.number, 1,
            "repo-a's first issue starts at 1 regardless of repo-b's own counter"
        );
        assert_eq!(
            in_b.number, 1,
            "repo-b has its own independent counter, not a shared global sequence"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn closing_and_reopening_round_trips_status_and_closed_at(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresIssueStore::new(pool);
        let issue = store
            .create(NewIssue {
                repository_id,
                author_id: owner_id,
                title: "t".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();

        store.close(issue.id).await.unwrap();
        let closed = store.find_by_id(issue.id).await.unwrap().unwrap();
        assert_eq!(closed.status, IssueStatus::Done);
        assert!(closed.closed_at.is_some());

        store.reopen(issue.id).await.unwrap();
        let reopened = store.find_by_id(issue.id).await.unwrap().unwrap();
        assert_eq!(reopened.status, IssueStatus::Todo);
        assert!(reopened.closed_at.is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn moving_status_to_done_sets_closed_at_and_moving_away_clears_it(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresIssueStore::new(pool);
        let issue = store
            .create(NewIssue {
                repository_id,
                author_id: owner_id,
                title: "t".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();

        store
            .update_status(issue.id, IssueStatus::Done)
            .await
            .unwrap();
        let done = store.find_by_id(issue.id).await.unwrap().unwrap();
        assert_eq!(done.status, IssueStatus::Done);
        assert!(done.closed_at.is_some());

        store
            .update_status(issue.id, IssueStatus::InProgress)
            .await
            .unwrap();
        let reopened = store.find_by_id(issue.id).await.unwrap().unwrap();
        assert_eq!(reopened.status, IssueStatus::InProgress);
        assert!(reopened.closed_at.is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn comments_are_listed_oldest_first(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresIssueStore::new(pool);
        let issue = store
            .create(NewIssue {
                repository_id,
                author_id: owner_id,
                title: "t".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();

        store
            .add_comment(NewIssueComment {
                issue_id: issue.id,
                author_id: owner_id,
                body: "first".to_string(),
            })
            .await
            .unwrap();
        store
            .add_comment(NewIssueComment {
                issue_id: issue.id,
                author_id: owner_id,
                body: "second".to_string(),
            })
            .await
            .unwrap();

        let comments = store.list_comments(issue.id).await.unwrap();
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].body, "first");
        assert_eq!(comments[1].body, "second");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn search_ranks_a_title_match_above_a_description_only_match(pool: PgPool) {
        let owner_id = seed_user(&pool, "florian").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresIssueStore::new(pool);
        let title_match = store
            .create(NewIssue {
                repository_id,
                author_id: owner_id,
                title: "crash on startup".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();
        let description_match = store
            .create(NewIssue {
                repository_id,
                author_id: owner_id,
                title: "unrelated".to_string(),
                description: "seen a crash on startup once".to_string(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();

        let results = store
            .search(&[repository_id], "crash on startup", 8)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, title_match.id);
        assert_eq!(results[1].id, description_match.id);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn search_excludes_a_negated_term_while_still_matching_the_positive_one(pool: PgPool) {
        let owner_id = seed_user(&pool, "florian").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresIssueStore::new(pool);
        let wanted = store
            .create(NewIssue {
                repository_id,
                author_id: owner_id,
                title: "widget for external use".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();
        let excluded = store
            .create(NewIssue {
                repository_id,
                author_id: owner_id,
                title: "widget for internal use".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();

        let results = store
            .search(&[repository_id], "widget -internal", 8)
            .await
            .unwrap();

        assert_eq!(
            results.iter().map(|i| i.id).collect::<Vec<_>>(),
            vec![wanted.id]
        );
        assert!(!results.iter().any(|i| i.id == excluded.id));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn search_never_returns_an_issue_outside_the_given_repository_ids(pool: PgPool) {
        let owner_id = seed_user(&pool, "florian").await;
        let in_scope_repo = seed_repository(&pool, owner_id, "a").await;
        let out_of_scope_repo = seed_repository(&pool, owner_id, "b").await;
        let store = PostgresIssueStore::new(pool);
        let in_scope = store
            .create(NewIssue {
                repository_id: in_scope_repo,
                author_id: owner_id,
                title: "widget bug".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();
        store
            .create(NewIssue {
                repository_id: out_of_scope_repo,
                author_id: owner_id,
                title: "widget bug too".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();

        let results = store
            .search(&[in_scope_repo], "widget bug", 8)
            .await
            .unwrap();

        assert_eq!(
            results.iter().map(|i| i.id).collect::<Vec<_>>(),
            vec![in_scope.id]
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_assigned_to_returns_only_open_issues_assigned_to_that_user(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let assignee_id = seed_user(&pool, "assignee").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresIssueStore::new(pool);

        let assigned = store
            .create(NewIssue {
                repository_id,
                author_id: owner_id,
                title: "assigned to me".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();
        store.assign(assigned.id, Some(assignee_id)).await.unwrap();

        let not_assigned = store
            .create(NewIssue {
                repository_id,
                author_id: owner_id,
                title: "not assigned".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();
        store.assign(not_assigned.id, Some(owner_id)).await.unwrap();

        let closed_but_assigned = store
            .create(NewIssue {
                repository_id,
                author_id: owner_id,
                title: "closed".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();
        store
            .assign(closed_but_assigned.id, Some(assignee_id))
            .await
            .unwrap();
        store.close(closed_but_assigned.id).await.unwrap();

        let results = store
            .list_assigned_to(assignee_id, &[repository_id], 20)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, assigned.id);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_assigned_to_is_scoped_to_the_given_repository_ids(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let assignee_id = seed_user(&pool, "assignee").await;
        let visible_repo = seed_repository(&pool, owner_id, "visible").await;
        let hidden_repo = seed_repository(&pool, owner_id, "hidden").await;
        let store = PostgresIssueStore::new(pool);

        let in_visible = store
            .create(NewIssue {
                repository_id: visible_repo,
                author_id: owner_id,
                title: "in visible repo".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();
        store
            .assign(in_visible.id, Some(assignee_id))
            .await
            .unwrap();

        let in_hidden = store
            .create(NewIssue {
                repository_id: hidden_repo,
                author_id: owner_id,
                title: "in hidden repo".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();
        store.assign(in_hidden.id, Some(assignee_id)).await.unwrap();

        let results = store
            .list_assigned_to(assignee_id, &[visible_repo], 20)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, in_visible.id);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_authored_by_returns_only_open_issues_authored_by_that_user(pool: PgPool) {
        let author_id = seed_user(&pool, "author").await;
        let other_id = seed_user(&pool, "other").await;
        let repository_id = seed_repository(&pool, author_id, "hello").await;
        let store = PostgresIssueStore::new(pool);

        let mine = store
            .create(NewIssue {
                repository_id,
                author_id,
                title: "mine".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();
        store
            .create(NewIssue {
                repository_id,
                author_id: other_id,
                title: "not mine".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();
        let mine_closed = store
            .create(NewIssue {
                repository_id,
                author_id,
                title: "mine but closed".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();
        store.close(mine_closed.id).await.unwrap();

        let results = store
            .list_authored_by(author_id, &[repository_id], 20)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, mine.id);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_for_repository_filtered_narrows_by_label_and_milestone(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresIssueStore::new(pool.clone());

        let matching = store
            .create(NewIssue {
                repository_id,
                author_id: owner_id,
                title: "matching".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();
        let other = store
            .create(NewIssue {
                repository_id,
                author_id: owner_id,
                title: "other".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();

        let label_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO labels (id, name, color, repository_id) VALUES ($1, 'Bug', '#dc2626', $2)",
            label_id,
            repository_id
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO issue_labels (issue_id, label_id) VALUES ($1, $2)",
            matching.id,
            label_id
        )
        .execute(&pool)
        .await
        .unwrap();

        let results = store
            .list_for_repository_filtered(repository_id, Some(vec![label_id]), None)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, matching.id);
        assert!(!results.iter().any(|i| i.id == other.id));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_for_repository_filtered_deduplicates_an_issue_matching_multiple_requested_labels(
        pool: PgPool,
    ) {
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresIssueStore::new(pool.clone());

        let doubly_tagged = store
            .create(NewIssue {
                repository_id,
                author_id: owner_id,
                title: "doubly tagged".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();

        let bug_label_id = Uuid::new_v4();
        let urgent_label_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO labels (id, name, color, repository_id) VALUES ($1, 'Bug', '#dc2626', $2)",
            bug_label_id,
            repository_id
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query!("INSERT INTO labels (id, name, color, repository_id) VALUES ($1, 'Urgent', '#f97316', $2)", urgent_label_id, repository_id).execute(&pool).await.unwrap();
        sqlx::query!(
            "INSERT INTO issue_labels (issue_id, label_id) VALUES ($1, $2)",
            doubly_tagged.id,
            bug_label_id
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO issue_labels (issue_id, label_id) VALUES ($1, $2)",
            doubly_tagged.id,
            urgent_label_id
        )
        .execute(&pool)
        .await
        .unwrap();

        // The LEFT JOIN gives one row per matching label; DISTINCT must collapse them to one per issue.
        let results = store
            .list_for_repository_filtered(
                repository_id,
                Some(vec![bug_label_id, urgent_label_id]),
                None,
            )
            .await
            .unwrap();

        assert_eq!(
            results.len(),
            1,
            "an issue matching 2 of the requested labels must appear once, not once per matching label"
        );
        assert_eq!(results[0].id, doubly_tagged.id);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn comment_counts_returns_the_number_of_comments_per_issue(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresIssueStore::new(pool);
        let new_issue = |title: &str| NewIssue {
            repository_id,
            author_id: owner_id,
            title: title.to_string(),
            description: String::new(),
            kind: IssueKind::Bug,
            parent_issue_id: None,
        };
        let busy = store.create(new_issue("busy")).await.unwrap();
        let quiet = store.create(new_issue("quiet")).await.unwrap();
        let silent = store.create(new_issue("silent")).await.unwrap();
        for body in ["one", "two", "three"] {
            store
                .add_comment(NewIssueComment {
                    issue_id: busy.id,
                    author_id: owner_id,
                    body: body.to_string(),
                })
                .await
                .unwrap();
        }
        store
            .add_comment(NewIssueComment {
                issue_id: quiet.id,
                author_id: owner_id,
                body: "only".to_string(),
            })
            .await
            .unwrap();

        let counts = store
            .comment_counts(&[busy.id, quiet.id, silent.id])
            .await
            .unwrap();

        assert_eq!(counts.get(&busy.id).copied(), Some(3));
        assert_eq!(counts.get(&quiet.id).copied(), Some(1));
        assert_eq!(
            counts.get(&silent.id).copied().unwrap_or(0),
            0,
            "an issue without comments is absent or zero"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn comment_counts_ignores_issues_that_were_not_asked_for_and_an_empty_slice_returns_an_empty_map(
        pool: PgPool,
    ) {
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresIssueStore::new(pool);
        let asked = store
            .create(NewIssue {
                repository_id,
                author_id: owner_id,
                title: "asked".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();
        let other = store
            .create(NewIssue {
                repository_id,
                author_id: owner_id,
                title: "other".to_string(),
                description: String::new(),
                kind: IssueKind::Bug,
                parent_issue_id: None,
            })
            .await
            .unwrap();
        store
            .add_comment(NewIssueComment {
                issue_id: other.id,
                author_id: owner_id,
                body: "noise".to_string(),
            })
            .await
            .unwrap();

        let counts = store.comment_counts(&[asked.id]).await.unwrap();
        assert!(!counts.contains_key(&other.id));
        assert_eq!(counts.get(&asked.id).copied().unwrap_or(0), 0);

        assert!(store.comment_counts(&[]).await.unwrap().is_empty());
    }
}
