use std::collections::HashMap;

use async_trait::async_trait;
use ferrisgit_domain::diff::DiffSide;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::merge_request::{
    MergeRequest, MergeRequestReview, MergeRequestReviewPort, MergeRequestStatus,
    MergeRequestStorePort, NewMergeRequest, ReviewDecision,
};
use ferrisgit_domain::merge_request_comment::{
    MergeRequestComment, MergeRequestCommentPort, NewMergeRequestComment,
};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresMergeRequestStore {
    pool: PgPool,
}

impl PostgresMergeRequestStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct Row {
    id: Uuid,
    repository_id: Uuid,
    author_id: Option<Uuid>,
    source_branch: String,
    target_branch: String,
    title: String,
    description: String,
    status: String,
    merge_commit_sha: Option<String>,
    milestone_id: Option<Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
    closed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Row {
    fn into_domain(self) -> Result<MergeRequest, DomainError> {
        Ok(MergeRequest {
            id: self.id,
            repository_id: self.repository_id,
            author_id: self.author_id,
            source_branch: self.source_branch,
            target_branch: self.target_branch,
            title: self.title,
            description: self.description,
            status: MergeRequestStatus::parse(&self.status)?,
            merge_commit_sha: self.merge_commit_sha,
            milestone_id: self.milestone_id,
            created_at: self.created_at,
            closed_at: self.closed_at,
        })
    }
}

struct CommentRow {
    id: Uuid,
    merge_request_id: Uuid,
    author_id: Option<Uuid>,
    body: String,
    created_at: chrono::DateTime<chrono::Utc>,
    reply_to_id: Option<Uuid>,
    file_path: Option<String>,
    line_number: Option<i32>,
    side: Option<String>,
    anchor_content: Option<String>,
    resolved: bool,
    end_line: Option<i32>,
    suggested_content: Option<String>,
    applied_at: Option<chrono::DateTime<chrono::Utc>>,
    applied_commit_sha: Option<String>,
}

impl CommentRow {
    fn into_domain(self) -> Result<MergeRequestComment, DomainError> {
        Ok(MergeRequestComment {
            id: self.id,
            merge_request_id: self.merge_request_id,
            author_id: self.author_id,
            body: self.body,
            created_at: self.created_at,
            reply_to_id: self.reply_to_id,
            file_path: self.file_path,
            line_number: self.line_number,
            side: self.side.map(|s| DiffSide::parse(&s)).transpose()?,
            anchor_content: self.anchor_content,
            resolved: self.resolved,
            end_line: self.end_line,
            suggested_content: self.suggested_content,
            applied_at: self.applied_at,
            applied_commit_sha: self.applied_commit_sha,
        })
    }
}

struct UpsertReviewRow {
    merge_request_id: Uuid,
    user_id: Uuid,
    decision: String,
    source_sha: String,
    created_at: chrono::DateTime<chrono::Utc>,
    username: String,
}

struct ReviewRow {
    merge_request_id: Uuid,
    user_id: Uuid,
    username: String,
    decision: String,
    source_sha: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl ReviewRow {
    fn into_domain(self) -> Result<MergeRequestReview, DomainError> {
        Ok(MergeRequestReview {
            merge_request_id: self.merge_request_id,
            user_id: self.user_id,
            username: self.username,
            decision: ReviewDecision::parse(&self.decision)?,
            source_sha: self.source_sha,
            created_at: self.created_at,
        })
    }
}

#[async_trait]
impl MergeRequestStorePort for PostgresMergeRequestStore {
    async fn create(&self, new_mr: NewMergeRequest) -> Result<MergeRequest, DomainError> {
        let row = sqlx::query_as!(
            Row,
            "INSERT INTO merge_requests (repository_id, author_id, source_branch, target_branch, title, description) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id, repository_id, author_id, source_branch, target_branch, title, description, status, merge_commit_sha, milestone_id, created_at, closed_at",
            new_mr.repository_id,
            new_mr.author_id,
            new_mr.source_branch,
            new_mr.target_branch,
            new_mr.title,
            new_mr.description,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        row.into_domain()
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<MergeRequest>, DomainError> {
        let row = sqlx::query_as!(Row, "SELECT id, repository_id, author_id, source_branch, target_branch, title, description, status, merge_commit_sha, milestone_id, created_at, closed_at FROM merge_requests WHERE id = $1", id).fetch_optional(&self.pool).await.map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        row.map(Row::into_domain).transpose()
    }

    async fn list_for_repository_filtered(
        &self,
        repository_id: Uuid,
        label_ids: Option<Vec<Uuid>>,
        milestone_id: Option<Uuid>,
    ) -> Result<Vec<MergeRequest>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT DISTINCT mr.id, mr.repository_id, mr.author_id, mr.source_branch, mr.target_branch, mr.title, mr.description, mr.status, mr.merge_commit_sha, mr.milestone_id, mr.created_at, mr.closed_at \
             FROM merge_requests mr \
             LEFT JOIN merge_request_labels mrl ON mrl.merge_request_id = mr.id \
             WHERE mr.repository_id = $1 \
               AND ($2::uuid[] IS NULL OR mrl.label_id = ANY($2::uuid[])) \
               AND ($3::uuid IS NULL OR mr.milestone_id = $3) \
             ORDER BY mr.created_at DESC",
            repository_id,
            label_ids.as_deref(),
            milestone_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        rows.into_iter().map(Row::into_domain).collect()
    }

    async fn update_fields(
        &self,
        id: Uuid,
        title: String,
        description: String,
    ) -> Result<(), DomainError> {
        sqlx::query!(
            "UPDATE merge_requests SET title = $1, description = $2 WHERE id = $3",
            title,
            description,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(())
    }

    async fn set_milestone(&self, id: Uuid, milestone_id: Option<Uuid>) -> Result<(), DomainError> {
        sqlx::query!(
            "UPDATE merge_requests SET milestone_id = $1 WHERE id = $2",
            milestone_id,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(())
    }

    async fn mark_merged(&self, id: Uuid, merge_commit_sha: &str) -> Result<(), DomainError> {
        let result = sqlx::query!(
            "UPDATE merge_requests SET status = 'merged', merge_commit_sha = $1, closed_at = now() WHERE id = $2 AND status = 'open'",
            merge_commit_sha,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(DomainError::Conflict(
                "merge request is no longer open".to_string(),
            ));
        }
        Ok(())
    }

    async fn mark_closed(&self, id: Uuid) -> Result<(), DomainError> {
        let result = sqlx::query!(
            "UPDATE merge_requests SET status = 'closed', closed_at = now() WHERE id = $1 AND status = 'open'",
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(DomainError::Conflict(
                "merge request is no longer open".to_string(),
            ));
        }
        Ok(())
    }

    /// Same search semantics as `PostgresRepositoryStore::search`.
    async fn search(
        &self,
        repository_ids: &[Uuid],
        query: &str,
        limit: i64,
    ) -> Result<Vec<MergeRequest>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT id, repository_id, author_id, source_branch, target_branch, title, description, status, merge_commit_sha, milestone_id, created_at, closed_at FROM merge_requests \
             WHERE repository_id = ANY($1::uuid[]) AND search_vector @@ websearch_to_tsquery('simple', $2) \
             ORDER BY ts_rank_cd(search_vector, websearch_to_tsquery('simple', $2)) DESC, id \
             LIMIT $3",
            repository_ids,
            query,
            limit,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        rows.into_iter().map(Row::into_domain).collect()
    }

    async fn list_authored_by(
        &self,
        user_id: Uuid,
        repository_ids: &[Uuid],
        limit: i64,
    ) -> Result<Vec<MergeRequest>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT id, repository_id, author_id, source_branch, target_branch, title, description, status, merge_commit_sha, milestone_id, created_at, closed_at FROM merge_requests \
             WHERE repository_id = ANY($1::uuid[]) AND author_id = $2 AND status = 'open' \
             ORDER BY created_at DESC, id LIMIT $3",
            repository_ids,
            user_id,
            limit,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        rows.into_iter().map(Row::into_domain).collect()
    }

    /// `IS DISTINCT FROM`, not `!=`: a merge request whose author's account was deleted (`author_id` NULL) still
    /// awaits everyone's review, where `NULL != $2` would silently drop it.
    async fn list_awaiting_review_by(
        &self,
        user_id: Uuid,
        repository_ids: &[Uuid],
        limit: i64,
    ) -> Result<Vec<MergeRequest>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT id, repository_id, author_id, source_branch, target_branch, title, description, status, merge_commit_sha, milestone_id, created_at, closed_at FROM merge_requests \
             WHERE repository_id = ANY($1::uuid[]) AND status = 'open' AND author_id IS DISTINCT FROM $2 \
             AND NOT EXISTS (SELECT 1 FROM merge_request_reviews r WHERE r.merge_request_id = merge_requests.id AND r.user_id = $2) \
             ORDER BY created_at DESC, id LIMIT $3",
            repository_ids,
            user_id,
            limit,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        rows.into_iter().map(Row::into_domain).collect()
    }
}

#[async_trait]
impl MergeRequestCommentPort for PostgresMergeRequestStore {
    async fn add_comment(
        &self,
        new_comment: NewMergeRequestComment,
    ) -> Result<MergeRequestComment, DomainError> {
        let (file_path, line_number, end_line, side, anchor_content) = match new_comment.anchor {
            Some(a) => (
                Some(a.file_path),
                Some(a.line_number),
                a.end_line,
                Some(a.side.as_str().to_string()),
                Some(a.anchor_content),
            ),
            None => (None, None, None, None, None),
        };
        let row = sqlx::query_as!(
            CommentRow,
            "INSERT INTO merge_request_comments (merge_request_id, author_id, body, reply_to_id, file_path, line_number, end_line, side, anchor_content, suggested_content) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING *",
            new_comment.merge_request_id,
            new_comment.author_id,
            new_comment.body,
            new_comment.reply_to_id,
            file_path,
            line_number,
            end_line,
            side,
            anchor_content,
            new_comment.suggested_content,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        row.into_domain()
    }

    async fn list_comments(
        &self,
        merge_request_id: Uuid,
    ) -> Result<Vec<MergeRequestComment>, DomainError> {
        let rows = sqlx::query_as!(CommentRow, "SELECT * FROM merge_request_comments WHERE merge_request_id = $1 ORDER BY created_at ASC", merge_request_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        rows.into_iter().map(CommentRow::into_domain).collect()
    }

    async fn set_comment_resolved(
        &self,
        comment_id: Uuid,
        resolved: bool,
    ) -> Result<(), DomainError> {
        let result = sqlx::query!(
            "UPDATE merge_request_comments SET resolved = $1 WHERE id = $2",
            resolved,
            comment_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound("comment".to_string()));
        }
        Ok(())
    }

    async fn mark_comment_applied(
        &self,
        comment_id: Uuid,
        commit_sha: &str,
    ) -> Result<MergeRequestComment, DomainError> {
        let row = sqlx::query_as!(
            CommentRow,
            "UPDATE merge_request_comments SET applied_at = now(), applied_commit_sha = $1 WHERE id = $2 RETURNING *",
            commit_sha,
            comment_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        row.ok_or_else(|| DomainError::NotFound("comment".to_string()))?
            .into_domain()
    }

    async fn comment_counts(
        &self,
        merge_request_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, i64>, DomainError> {
        if merge_request_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query_as::<_, (Uuid, i64)>("SELECT merge_request_id, COUNT(*)::bigint FROM merge_request_comments WHERE merge_request_id = ANY($1) GROUP BY merge_request_id")
            .bind(merge_request_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(rows.into_iter().collect())
    }
}

#[async_trait]
impl MergeRequestReviewPort for PostgresMergeRequestStore {
    async fn upsert_review(
        &self,
        merge_request_id: Uuid,
        user_id: Uuid,
        decision: ReviewDecision,
        source_sha: &str,
    ) -> Result<MergeRequestReview, DomainError> {
        let decision_str = decision.as_str().to_string();
        let row = sqlx::query_as!(
            UpsertReviewRow,
            "INSERT INTO merge_request_reviews (merge_request_id, user_id, decision, source_sha) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (merge_request_id, user_id) DO UPDATE SET decision = $3, source_sha = $4, created_at = now() \
             RETURNING merge_request_id, user_id, decision, source_sha, created_at, \
             (SELECT username FROM users WHERE id = $2) AS \"username!\"",
            merge_request_id,
            user_id,
            decision_str,
            source_sha,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;

        Ok(MergeRequestReview {
            merge_request_id: row.merge_request_id,
            user_id: row.user_id,
            username: row.username,
            decision: ReviewDecision::parse(&row.decision)?,
            source_sha: row.source_sha,
            created_at: row.created_at,
        })
    }

    async fn list_reviews(
        &self,
        merge_request_id: Uuid,
    ) -> Result<Vec<MergeRequestReview>, DomainError> {
        let rows = sqlx::query_as!(
            ReviewRow,
            "SELECT mrr.merge_request_id, mrr.user_id, u.username, mrr.decision, mrr.source_sha, mrr.created_at \
             FROM merge_request_reviews mrr JOIN users u ON u.id = mrr.user_id \
             WHERE mrr.merge_request_id = $1 ORDER BY mrr.created_at ASC",
            merge_request_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        rows.into_iter().map(ReviewRow::into_domain).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrisgit_domain::repository::{NewRepository, RepositoryStorePort, RepositoryVisibility};
    use ferrisgit_domain::user::{NewUser, UserRepositoryPort};

    async fn seed_repository(pool: &PgPool) -> Uuid {
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let owner_id = users
            .create(NewUser {
                username: "florian".to_string(),
                email: "f@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
            })
            .await
            .unwrap()
            .id;
        let repos = crate::postgres::repository_store::PostgresRepositoryStore::new(pool.clone());
        repos
            .create(
                NewRepository {
                    owner_id,
                    name: "hello".to_string(),
                    group_id: None,
                    description: String::new(),
                    visibility: RepositoryVisibility::Private,
                },
                "path".to_string(),
            )
            .await
            .unwrap()
            .id
    }

    async fn seed_second_user(pool: &PgPool, username: &str) -> Uuid {
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        users
            .create(NewUser {
                username: username.to_string(),
                email: format!("{username}@example.com"),
                password_hash: "h".to_string(),
                is_admin: false,
            })
            .await
            .unwrap()
            .id
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

    #[sqlx::test(migrations = "../../migrations")]
    async fn creating_then_finding_a_merge_request_returns_it(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let store = PostgresMergeRequestStore::new(pool);

        let created = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();
        assert_eq!(created.status, MergeRequestStatus::Open);
        assert_eq!(created.merge_commit_sha, None);

        let found = store.find_by_id(created.id).await.unwrap().unwrap();
        assert_eq!(found.title, "Add feature");
        assert_eq!(found.source_branch, "feature");
        assert_eq!(found.target_branch, "main");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn mark_merged_sets_status_and_the_merge_commit_sha(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let store = PostgresMergeRequestStore::new(pool);
        let created = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();

        store.mark_merged(created.id, "deadbeef").await.unwrap();

        let found = store.find_by_id(created.id).await.unwrap().unwrap();
        assert_eq!(found.status, MergeRequestStatus::Merged);
        assert_eq!(found.merge_commit_sha, Some("deadbeef".to_string()));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn mark_closed_sets_status_and_closed_at(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let store = PostgresMergeRequestStore::new(pool);
        let created = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();

        store.mark_closed(created.id).await.unwrap();

        let found = store.find_by_id(created.id).await.unwrap().unwrap();
        assert_eq!(found.status, MergeRequestStatus::Closed);
        assert!(found.closed_at.is_some());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn mark_merged_on_an_already_closed_merge_request_returns_a_conflict(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let store = PostgresMergeRequestStore::new(pool);
        let created = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();

        store.mark_merged(created.id, "deadbeef").await.unwrap();

        let second_attempt = store.mark_closed(created.id).await;

        assert!(matches!(second_attempt, Err(DomainError::Conflict(_))));
        let found = store.find_by_id(created.id).await.unwrap().unwrap();
        assert_eq!(found.status, MergeRequestStatus::Merged);
        assert_eq!(found.merge_commit_sha, Some("deadbeef".to_string()));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn adding_then_listing_comments_returns_them_oldest_first(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let store = PostgresMergeRequestStore::new(pool);
        let mr = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();

        store
            .add_comment(NewMergeRequestComment {
                merge_request_id: mr.id,
                author_id,
                body: "first".to_string(),
                reply_to_id: None,
                anchor: None,
                suggested_content: None,
            })
            .await
            .unwrap();
        store
            .add_comment(NewMergeRequestComment {
                merge_request_id: mr.id,
                author_id,
                body: "second".to_string(),
                reply_to_id: None,
                anchor: None,
                suggested_content: None,
            })
            .await
            .unwrap();

        let comments = store.list_comments(mr.id).await.unwrap();

        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].body, "first");
        assert_eq!(comments[1].body, "second");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn add_comment_round_trips_an_inline_anchor_and_a_reply(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let store = PostgresMergeRequestStore::new(pool);
        let mr = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();

        let root = store
            .add_comment(NewMergeRequestComment {
                merge_request_id: mr.id,
                author_id,
                body: "why this line?".to_string(),
                reply_to_id: None,
                anchor: Some(ferrisgit_domain::merge_request_comment::CommentAnchor {
                    file_path: "README.md".to_string(),
                    line_number: 2,
                    end_line: None,
                    side: DiffSide::New,
                    anchor_content: "line 2\n".to_string(),
                }),
                suggested_content: None,
            })
            .await
            .unwrap();
        assert_eq!(root.file_path.as_deref(), Some("README.md"));
        assert_eq!(root.line_number, Some(2));
        assert_eq!(root.side, Some(DiffSide::New));
        assert!(root.reply_to_id.is_none());

        let reply = store
            .add_comment(NewMergeRequestComment {
                merge_request_id: mr.id,
                author_id,
                body: "good question".to_string(),
                reply_to_id: Some(root.id),
                anchor: None,
                suggested_content: None,
            })
            .await
            .unwrap();
        assert_eq!(reply.reply_to_id, Some(root.id));
        assert!(
            reply.file_path.is_none(),
            "a reply's own row has no anchor of its own — the use case layer is what copies the root's anchor onto a reply before calling add_comment, not the store"
        );

        let comments = store.list_comments(mr.id).await.unwrap();
        assert_eq!(comments.len(), 2);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn add_comment_round_trips_a_multi_line_suggestion_and_mark_comment_applied_sets_it_applied(
        pool: PgPool,
    ) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let store = PostgresMergeRequestStore::new(pool);
        let mr = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();

        let suggestion = store
            .add_comment(NewMergeRequestComment {
                merge_request_id: mr.id,
                author_id,
                body: "swap this block".to_string(),
                reply_to_id: None,
                anchor: Some(ferrisgit_domain::merge_request_comment::CommentAnchor {
                    file_path: "README.md".to_string(),
                    line_number: 2,
                    end_line: Some(3),
                    side: DiffSide::New,
                    anchor_content: "line 2\nline 3\n".to_string(),
                }),
                suggested_content: Some("replacement 2\nreplacement 3\n".to_string()),
            })
            .await
            .unwrap();
        assert_eq!(suggestion.end_line, Some(3));
        assert_eq!(
            suggestion.suggested_content.as_deref(),
            Some("replacement 2\nreplacement 3\n")
        );
        assert!(suggestion.applied_at.is_none());

        let applied = store
            .mark_comment_applied(suggestion.id, "deadbeef")
            .await
            .unwrap();
        assert!(applied.applied_at.is_some());
        assert_eq!(applied.applied_commit_sha.as_deref(), Some("deadbeef"));

        let comments = store.list_comments(mr.id).await.unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].applied_commit_sha.as_deref(), Some("deadbeef"));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn mark_comment_applied_on_a_non_existent_comment_is_not_found(pool: PgPool) {
        let store = PostgresMergeRequestStore::new(pool);

        let result = store.mark_comment_applied(Uuid::new_v4(), "deadbeef").await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_new_comment_starts_unresolved_and_set_comment_resolved_toggles_it(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let store = PostgresMergeRequestStore::new(pool);
        let mr = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();
        let root = store
            .add_comment(NewMergeRequestComment {
                merge_request_id: mr.id,
                author_id,
                body: "why?".to_string(),
                reply_to_id: None,
                anchor: None,
                suggested_content: None,
            })
            .await
            .unwrap();
        assert!(!root.resolved);

        store.set_comment_resolved(root.id, true).await.unwrap();
        let resolved = store.list_comments(mr.id).await.unwrap();
        assert!(resolved[0].resolved);

        store.set_comment_resolved(root.id, false).await.unwrap();
        let reopened = store.list_comments(mr.id).await.unwrap();
        assert!(!reopened[0].resolved);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn set_comment_resolved_on_a_non_existent_comment_is_not_found(pool: PgPool) {
        let store = PostgresMergeRequestStore::new(pool);

        let result = store.set_comment_resolved(Uuid::new_v4(), true).await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn upserting_a_review_twice_overwrites_rather_than_duplicates(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let store = PostgresMergeRequestStore::new(pool);
        let mr = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();

        store
            .upsert_review(mr.id, author_id, ReviewDecision::Approved, "sha1")
            .await
            .unwrap();
        let updated = store
            .upsert_review(mr.id, author_id, ReviewDecision::ChangesRequested, "sha2")
            .await
            .unwrap();

        assert_eq!(updated.decision, ReviewDecision::ChangesRequested);
        assert_eq!(updated.source_sha, "sha2");
        let listed = store.list_reviews(mr.id).await.unwrap();
        assert_eq!(
            listed.len(),
            1,
            "a second review from the same user must overwrite, not add a row"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn two_different_users_reviews_both_appear(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let other_id = users
            .create(ferrisgit_domain::user::NewUser {
                username: "alice".to_string(),
                email: "alice@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
            })
            .await
            .unwrap()
            .id;
        let store = PostgresMergeRequestStore::new(pool);
        let mr = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();

        store
            .upsert_review(mr.id, author_id, ReviewDecision::Approved, "sha1")
            .await
            .unwrap();
        store
            .upsert_review(mr.id, other_id, ReviewDecision::ChangesRequested, "sha1")
            .await
            .unwrap();

        let listed = store.list_reviews(mr.id).await.unwrap();
        assert_eq!(listed.len(), 2);
        assert!(
            listed
                .iter()
                .any(|r| r.user_id == author_id && r.decision == ReviewDecision::Approved)
        );
        assert!(
            listed
                .iter()
                .any(|r| r.user_id == other_id && r.decision == ReviewDecision::ChangesRequested)
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_reviews_resolves_the_reviewers_username(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let store = PostgresMergeRequestStore::new(pool);
        let mr = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();

        store
            .upsert_review(mr.id, author_id, ReviewDecision::Approved, "sha1")
            .await
            .unwrap();

        let listed = store.list_reviews(mr.id).await.unwrap();
        assert_eq!(listed[0].username, "florian");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn search_ranks_a_title_match_above_a_description_only_match(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let store = PostgresMergeRequestStore::new(pool);
        let title_match = store
            .create(NewMergeRequest {
                repository_id,
                author_id,
                source_branch: "feature".to_string(),
                target_branch: "main".to_string(),
                title: "fix the parser".to_string(),
                description: String::new(),
            })
            .await
            .unwrap();
        let description_match = store
            .create(NewMergeRequest {
                repository_id,
                author_id,
                source_branch: "other".to_string(),
                target_branch: "main".to_string(),
                title: "unrelated".to_string(),
                description: "notes about how to fix the parser correctly".to_string(),
            })
            .await
            .unwrap();

        let results = store
            .search(&[repository_id], "fix the parser", 8)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, title_match.id);
        assert_eq!(results[1].id, description_match.id);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn search_excludes_a_negated_term_while_still_matching_the_positive_one(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let store = PostgresMergeRequestStore::new(pool);
        let wanted = store
            .create(NewMergeRequest {
                repository_id,
                author_id,
                source_branch: "feature-a".to_string(),
                target_branch: "main".to_string(),
                title: "widget for external use".to_string(),
                description: String::new(),
            })
            .await
            .unwrap();
        let excluded = store
            .create(NewMergeRequest {
                repository_id,
                author_id,
                source_branch: "feature-b".to_string(),
                target_branch: "main".to_string(),
                title: "widget for internal use".to_string(),
                description: String::new(),
            })
            .await
            .unwrap();

        let results = store
            .search(&[repository_id], "widget -internal", 8)
            .await
            .unwrap();

        assert_eq!(
            results.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![wanted.id]
        );
        assert!(!results.iter().any(|m| m.id == excluded.id));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn search_never_returns_a_merge_request_outside_the_given_repository_ids(pool: PgPool) {
        let in_scope_repo = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let out_of_scope_repo = {
            let repos =
                crate::postgres::repository_store::PostgresRepositoryStore::new(pool.clone());
            repos
                .create(
                    ferrisgit_domain::repository::NewRepository {
                        owner_id: author_id,
                        name: "other".to_string(),
                        group_id: None,
                        description: String::new(),
                        visibility: RepositoryVisibility::Private,
                    },
                    "other-path".to_string(),
                )
                .await
                .unwrap()
                .id
        };
        let store = PostgresMergeRequestStore::new(pool);
        let in_scope = store
            .create(NewMergeRequest {
                repository_id: in_scope_repo,
                author_id,
                source_branch: "f".to_string(),
                target_branch: "main".to_string(),
                title: "gizmo change".to_string(),
                description: String::new(),
            })
            .await
            .unwrap();
        store
            .create(NewMergeRequest {
                repository_id: out_of_scope_repo,
                author_id,
                source_branch: "f".to_string(),
                target_branch: "main".to_string(),
                title: "gizmo change too".to_string(),
                description: String::new(),
            })
            .await
            .unwrap();

        let results = store
            .search(&[in_scope_repo], "gizmo change", 8)
            .await
            .unwrap();

        assert_eq!(
            results.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![in_scope.id]
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_authored_by_returns_only_open_mrs_authored_by_that_user(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let other_id = seed_second_user(&pool, "other").await;
        let store = PostgresMergeRequestStore::new(pool);

        let mine = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();
        store.create(new_mr(repository_id, other_id)).await.unwrap();
        let mine_merged = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();
        store.mark_merged(mine_merged.id, "abc123").await.unwrap();

        let results = store
            .list_authored_by(author_id, &[repository_id], 20)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, mine.id);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_awaiting_review_by_excludes_own_mrs_and_already_reviewed_ones(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let reviewer_id = seed_second_user(&pool, "reviewer").await;
        let store = PostgresMergeRequestStore::new(pool);

        let own_mr = store
            .create(new_mr(repository_id, reviewer_id))
            .await
            .unwrap();
        let already_reviewed = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();
        store
            .upsert_review(
                already_reviewed.id,
                reviewer_id,
                ReviewDecision::Approved,
                "sha1",
            )
            .await
            .unwrap();
        let needs_review = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();

        let results = store
            .list_awaiting_review_by(reviewer_id, &[repository_id], 20)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, needs_review.id);
        assert!(
            results.iter().all(|mr| mr.id != own_mr.id),
            "the reviewer's own MR must never appear in their own review queue"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn an_open_merge_request_whose_author_was_deleted_still_awaits_review(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let reviewer_id = seed_second_user(&pool, "reviewer").await;
        let gone_id = seed_second_user(&pool, "gone").await;
        let store = PostgresMergeRequestStore::new(pool.clone());
        let orphaned = store.create(new_mr(repository_id, gone_id)).await.unwrap();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(gone_id)
            .execute(&pool)
            .await
            .unwrap();

        let results = store
            .list_awaiting_review_by(reviewer_id, &[repository_id], 20)
            .await
            .unwrap();

        assert_eq!(
            results
                .iter()
                .map(|mr| (mr.id, mr.author_id))
                .collect::<Vec<_>>(),
            vec![(orphaned.id, None)]
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn update_fields_changes_title_and_description_but_not_other_fields(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let store = PostgresMergeRequestStore::new(pool);
        let created = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();

        store
            .update_fields(
                created.id,
                "new title".to_string(),
                "new description".to_string(),
            )
            .await
            .unwrap();

        let found = store.find_by_id(created.id).await.unwrap().unwrap();
        assert_eq!(found.title, "new title");
        assert_eq!(found.description, "new description");
        assert_eq!(found.status, MergeRequestStatus::Open);
        assert_eq!(found.source_branch, "feature");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn set_milestone_assigns_then_clears_it(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let store = PostgresMergeRequestStore::new(pool.clone());
        let created = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();
        assert_eq!(created.milestone_id, None);

        let milestone_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO milestones (id, title, repository_id) VALUES ($1, 'v1', $2)",
            milestone_id,
            repository_id
        )
        .execute(&pool)
        .await
        .unwrap();

        store
            .set_milestone(created.id, Some(milestone_id))
            .await
            .unwrap();
        let with_milestone = store.find_by_id(created.id).await.unwrap().unwrap();
        assert_eq!(with_milestone.milestone_id, Some(milestone_id));

        store.set_milestone(created.id, None).await.unwrap();
        let cleared = store.find_by_id(created.id).await.unwrap().unwrap();
        assert_eq!(cleared.milestone_id, None);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_for_repository_filtered_narrows_by_label_and_milestone(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let store = PostgresMergeRequestStore::new(pool.clone());

        let matching = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();
        let other = store
            .create(new_mr(repository_id, author_id))
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
            "INSERT INTO merge_request_labels (merge_request_id, label_id) VALUES ($1, $2)",
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
        assert!(!results.iter().any(|mr| mr.id == other.id));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_for_repository_filtered_deduplicates_a_merge_request_matching_multiple_requested_labels(
        pool: PgPool,
    ) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let store = PostgresMergeRequestStore::new(pool.clone());

        let doubly_tagged = store
            .create(new_mr(repository_id, author_id))
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
            "INSERT INTO merge_request_labels (merge_request_id, label_id) VALUES ($1, $2)",
            doubly_tagged.id,
            bug_label_id
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO merge_request_labels (merge_request_id, label_id) VALUES ($1, $2)",
            doubly_tagged.id,
            urgent_label_id
        )
        .execute(&pool)
        .await
        .unwrap();

        // The LEFT JOIN yields one row per matching label; DISTINCT must collapse them to one per MR.
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
            "an MR matching 2 of the requested labels must appear once, not once per matching label"
        );
        assert_eq!(results[0].id, doubly_tagged.id);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_for_repository_filtered_narrows_by_milestone_only(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let store = PostgresMergeRequestStore::new(pool.clone());

        let milestone_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO milestones (id, title, repository_id) VALUES ($1, 'v1', $2)",
            milestone_id,
            repository_id
        )
        .execute(&pool)
        .await
        .unwrap();

        let matching = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();
        store
            .set_milestone(matching.id, Some(milestone_id))
            .await
            .unwrap();
        let other = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();

        let results = store
            .list_for_repository_filtered(repository_id, None, Some(milestone_id))
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, matching.id);
        assert!(!results.iter().any(|mr| mr.id == other.id));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn comment_counts_counts_general_inline_and_reply_comments_per_merge_request(
        pool: PgPool,
    ) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let author_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let store = PostgresMergeRequestStore::new(pool);
        let busy = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();
        let quiet = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();
        let silent = store
            .create(new_mr(repository_id, author_id))
            .await
            .unwrap();

        store
            .add_comment(NewMergeRequestComment {
                merge_request_id: busy.id,
                author_id,
                body: "general".to_string(),
                reply_to_id: None,
                anchor: None,
                suggested_content: None,
            })
            .await
            .unwrap();
        let root = store
            .add_comment(NewMergeRequestComment {
                merge_request_id: busy.id,
                author_id,
                body: "inline".to_string(),
                reply_to_id: None,
                anchor: Some(ferrisgit_domain::merge_request_comment::CommentAnchor {
                    file_path: "README.md".to_string(),
                    line_number: 2,
                    end_line: None,
                    side: DiffSide::New,
                    anchor_content: "line 2\n".to_string(),
                }),
                suggested_content: None,
            })
            .await
            .unwrap();
        store
            .add_comment(NewMergeRequestComment {
                merge_request_id: busy.id,
                author_id,
                body: "reply".to_string(),
                reply_to_id: Some(root.id),
                anchor: None,
                suggested_content: None,
            })
            .await
            .unwrap();
        store
            .add_comment(NewMergeRequestComment {
                merge_request_id: quiet.id,
                author_id,
                body: "only".to_string(),
                reply_to_id: None,
                anchor: None,
                suggested_content: None,
            })
            .await
            .unwrap();

        let counts = store
            .comment_counts(&[busy.id, quiet.id, silent.id])
            .await
            .unwrap();

        assert_eq!(
            counts.get(&busy.id).copied(),
            Some(3),
            "general + inline + reply"
        );
        assert_eq!(counts.get(&quiet.id).copied(), Some(1));
        assert_eq!(
            counts.get(&silent.id).copied().unwrap_or(0),
            0,
            "a merge request without comments is absent or zero"
        );

        let only_quiet = store.comment_counts(&[quiet.id]).await.unwrap();
        assert!(!only_quiet.contains_key(&busy.id));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn comment_counts_of_an_empty_slice_is_an_empty_map(pool: PgPool) {
        let store = PostgresMergeRequestStore::new(pool);
        assert!(store.comment_counts(&[]).await.unwrap().is_empty());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_comment_whose_author_was_deleted_still_lists_without_an_author(pool: PgPool) {
        let repository_id = seed_repository(&pool).await;
        let users = crate::postgres::user_repository::PostgresUserRepository::new(pool.clone());
        let owner_id = users.find_by_username("florian").await.unwrap().unwrap().id;
        let store = PostgresMergeRequestStore::new(pool.clone());
        let merge_request = store.create(new_mr(repository_id, owner_id)).await.unwrap();
        sqlx::query("INSERT INTO merge_request_comments (merge_request_id, author_id, body) VALUES ($1, NULL, 'orphaned')")
            .bind(merge_request.id)
            .execute(&pool)
            .await
            .unwrap();

        let comments = store.list_comments(merge_request.id).await.unwrap();

        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].author_id, None);
    }
}
