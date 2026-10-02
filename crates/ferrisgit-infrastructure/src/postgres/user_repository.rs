use super::repository_row::RepositoryRow;
use crate::error::infra;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::repository::Repository;
use ferrisgit_domain::user::{NewUser, User, UserRepositoryPort};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresUserRepository {
    pool: PgPool,
}

impl PostgresUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// WHERE clause over `users u` for an admin who can sign in: no pending invitation or password reset (either
/// leaves the password unusable). A fixed string, never built from input.
const ACTIVE_ADMIN: &str = "u.is_admin \
     AND NOT EXISTS (SELECT 1 FROM user_invitations i WHERE i.user_id = u.id) \
     AND NOT EXISTS (SELECT 1 FROM password_reset_tokens r WHERE r.user_id = u.id)";

/// Prefix of the Maintainer-rule queries in `delete`: every group where `$1` is a direct Maintainer, walked
/// up to its root. A fixed string, never built from input.
const MAINTAINED_CHAINS: &str = "WITH RECURSIVE chain(root, id, parent_group_id, name, depth) AS ( \
       SELECT g.id, g.id, g.parent_group_id, g.name, 0 FROM groups g \
       WHERE g.id IN (SELECT m.group_id FROM group_members m WHERE m.user_id = $1 AND m.role = 'maintainer') \
     UNION ALL \
       SELECT c.root, g.id, g.parent_group_id, g.name, c.depth + 1 FROM groups g JOIN chain c ON g.id = c.parent_group_id)";

#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    username: String,
    email: String,
    password_hash: String,
    is_admin: bool,
    created_at: DateTime<Utc>,
}

impl From<UserRow> for User {
    fn from(row: UserRow) -> Self {
        User {
            id: row.id,
            username: row.username,
            email: row.email,
            password_hash: row.password_hash,
            is_admin: row.is_admin,
            created_at: row.created_at,
        }
    }
}

#[async_trait]
impl UserRepositoryPort for PostgresUserRepository {
    async fn find_by_username(&self, username: &str) -> Result<Option<User>, DomainError> {
        sqlx::query_as!(User, "SELECT id, username, email, password_hash, is_admin, created_at FROM users WHERE username = $1", username)
            .fetch_optional(&self.pool)
            .await
            .map_err(infra)
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, DomainError> {
        sqlx::query_as!(User, "SELECT id, username, email, password_hash, is_admin, created_at FROM users WHERE id = $1", id)
            .fetch_optional(&self.pool)
            .await
            .map_err(infra)
    }

    async fn create(&self, new_user: NewUser) -> Result<User, DomainError> {
        sqlx::query_as!(
            User,
            "INSERT INTO users (username, email, password_hash, is_admin) VALUES ($1, $2, $3, $4) RETURNING id, username, email, password_hash, is_admin, created_at",
            new_user.username,
            new_user.email,
            new_user.password_hash,
            new_user.is_admin,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(infra)
    }

    async fn count(&self) -> Result<i64, DomainError> {
        sqlx::query_scalar!("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await
            .map(|count| count.unwrap_or(0))
            .map_err(infra)
    }

    async fn update_email(&self, user_id: Uuid, email: String) -> Result<User, DomainError> {
        sqlx::query_as!(User, "UPDATE users SET email = $1 WHERE id = $2 RETURNING id, username, email, password_hash, is_admin, created_at", email, user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(infra)?
            .ok_or_else(|| DomainError::NotFound("user".to_string()))
    }

    async fn update_password_hash(
        &self,
        user_id: Uuid,
        password_hash: String,
    ) -> Result<(), DomainError> {
        let result = sqlx::query!(
            "UPDATE users SET password_hash = $1 WHERE id = $2",
            password_hash,
            user_id
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound("user".to_string()));
        }
        Ok(())
    }

    /// Counts only admins who can sign in, so one with a pending invitation or reset can't let the real last admin step down.
    async fn count_admins(&self) -> Result<i64, DomainError> {
        sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM users u WHERE {ACTIVE_ADMIN}"
        )))
        .fetch_one(&self.pool)
        .await
        .map_err(infra)
    }

    /// A demotion locks every active admin row in id order, so concurrent demotions run one after the other and
    /// the second re-reads the admin set. The fixed order avoids a deadlock (Postgres would abort one with a 500).
    /// Promotions take no lock, and demoting an admin who can't sign in yet is always allowed.
    async fn set_admin(&self, user_id: Uuid, is_admin: bool) -> Result<(), DomainError> {
        let mut tx = self.pool.begin().await.map_err(infra)?;
        if !is_admin {
            let admins: Vec<Uuid> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "SELECT u.id FROM users u WHERE {ACTIVE_ADMIN} ORDER BY u.id FOR UPDATE OF u"
            )))
            .fetch_all(&mut *tx)
            .await
            .map_err(infra)?;
            if admins == [user_id] {
                return Err(DomainError::Conflict(
                    "cannot remove the last administrator".to_string(),
                ));
            }
        }
        let result = sqlx::query("UPDATE users SET is_admin = $1 WHERE id = $2")
            .bind(is_admin)
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(infra)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound("user".to_string()));
        }
        tx.commit().await.map_err(infra)
    }

    /// One transaction, every refusal before any write. Locks the target and all active admins in id order (same
    /// as `set_admin`, so no deadlock with a demotion), then the Maintainer grants of the affected chains FOR UPDATE:
    /// with FOR SHARE, two Maintainers deleted at once would each share-lock the other's grant and deadlock on the
    /// cascade. Then personal repos are deleted (rows only, the caller cleans up the disk), group repos go to the heir
    /// and the user row cascades away. Runtime queries, since `ACTIVE_ADMIN` is spliced in.
    async fn delete(&self, user_id: Uuid, heir_id: Uuid) -> Result<Vec<Repository>, DomainError> {
        if heir_id == user_id {
            return Err(DomainError::Validation(
                "a deleted user's group repositories cannot go to that same user".to_string(),
            ));
        }
        let mut tx = self.pool.begin().await.map_err(infra)?;
        let locked: Vec<(Uuid, bool)> = sqlx::query_as(sqlx::AssertSqlSafe(format!("SELECT u.id, ({ACTIVE_ADMIN}) FROM users u WHERE u.id = $1 OR ({ACTIVE_ADMIN}) ORDER BY u.id FOR UPDATE OF u")))
            .bind(user_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(infra)?;
        let Some(&(_, target_is_active_admin)) = locked.iter().find(|(id, _)| *id == user_id)
        else {
            return Err(DomainError::NotFound("user".to_string()));
        };
        if target_is_active_admin
            && locked
                .iter()
                .filter(|(_, is_active_admin)| *is_active_admin)
                .count()
                == 1
        {
            return Err(DomainError::Conflict(
                "cannot remove the last administrator".to_string(),
            ));
        }

        // Lock first, then check: the check's snapshot then comes after any change the lock waited for.
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "{MAINTAINED_CHAINS} SELECT m.group_id FROM group_members m WHERE m.group_id IN (SELECT id FROM chain) AND m.role = 'maintainer' \
             ORDER BY m.group_id, m.user_id FOR UPDATE OF m"
        )))
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(infra)?;
        let orphaned_group: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "{MAINTAINED_CHAINS}, flagged AS ( \
                SELECT c.root, c.name, c.depth, EXISTS (SELECT 1 FROM group_members m WHERE m.group_id = c.id AND m.role = 'maintainer' AND m.user_id <> $1) AS has_other \
                FROM chain c) \
             SELECT string_agg(name, '/' ORDER BY depth DESC) FROM flagged GROUP BY root HAVING NOT bool_or(has_other) ORDER BY 1 LIMIT 1"
        )))
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(infra)?;
        if let Some(path) = orphaned_group {
            return Err(DomainError::Conflict(format!(
                "the user is the last maintainer of the group {path}; promote another member first"
            )));
        }

        let deleted: Vec<RepositoryRow> = sqlx::query_as(
            "DELETE FROM repositories WHERE owner_id = $1 AND group_id IS NULL RETURNING id, owner_id, name, group_id, description, disk_path, visibility, created_at",
        )
        .bind(user_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(infra)?;
        let deleted = deleted
            .into_iter()
            .map(Repository::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        sqlx::query(
            "UPDATE repositories SET owner_id = $2 WHERE owner_id = $1 AND group_id IS NOT NULL",
        )
        .bind(user_id)
        .bind(heir_id)
        .execute(&mut *tx)
        .await
        .map_err(infra)?;
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(infra)?;
        tx.commit().await.map_err(infra)?;
        Ok(deleted)
    }

    /// Same search semantics as `PostgresRepositoryStore::search`.
    async fn search(&self, query: &str, limit: i64) -> Result<Vec<User>, DomainError> {
        sqlx::query_as!(
            User,
            "SELECT id, username, email, password_hash, is_admin, created_at FROM users \
             WHERE search_vector @@ websearch_to_tsquery('simple', $1) \
             ORDER BY ts_rank_cd(search_vector, websearch_to_tsquery('simple', $1)) DESC, id \
             LIMIT $2",
            query,
            limit,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)
    }

    async fn find_by_username_ignore_case(
        &self,
        username: &str,
    ) -> Result<Option<User>, DomainError> {
        sqlx::query_as::<_, UserRow>("SELECT id, username, email, password_hash, is_admin, created_at FROM users WHERE lower(username) = lower($1)")
            .bind(username)
            .fetch_optional(&self.pool)
            .await
            .map(|row| row.map(User::from))
            .map_err(infra)
    }

    async fn find_by_email_ignore_case(&self, email: &str) -> Result<Option<User>, DomainError> {
        sqlx::query_as::<_, UserRow>("SELECT id, username, email, password_hash, is_admin, created_at FROM users WHERE lower(email) = lower($1)")
            .bind(email)
            .fetch_optional(&self.pool)
            .await
            .map(|row| row.map(User::from))
            .map_err(infra)
    }

    async fn list(&self, limit: i64) -> Result<Vec<User>, DomainError> {
        sqlx::query_as::<_, UserRow>("SELECT id, username, email, password_hash, is_admin, created_at FROM users ORDER BY created_at, id LIMIT $1")
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map(|rows| rows.into_iter().map(User::from).collect())
            .map_err(infra)
    }

    async fn get_token_epoch(&self, user_id: Uuid) -> Result<i32, DomainError> {
        sqlx::query_scalar!("SELECT token_epoch FROM users WHERE id = $1", user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(infra)?
            .ok_or_else(|| DomainError::NotFound("user".to_string()))
    }

    async fn bump_token_epoch(&self, user_id: Uuid) -> Result<(), DomainError> {
        let result = sqlx::query!(
            "UPDATE users SET token_epoch = token_epoch + 1 WHERE id = $1",
            user_id
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound("user".to_string()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "../../migrations")]
    async fn creating_then_finding_a_user_by_username_returns_it(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool);
        let created = repo
            .create(NewUser {
                username: "florian".to_string(),
                email: "florian@example.com".to_string(),
                password_hash: "hash".to_string(),
                is_admin: true,
            })
            .await
            .unwrap();

        let found = repo.find_by_username("florian").await.unwrap().unwrap();
        assert_eq!(found.id, created.id);
        assert!(found.is_admin);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn finding_an_unknown_username_returns_none(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool);
        assert!(repo.find_by_username("nobody").await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn count_reflects_the_number_of_users(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool);
        assert_eq!(repo.count().await.unwrap(), 0);
        repo.create(NewUser {
            username: "a".to_string(),
            email: "a@example.com".to_string(),
            password_hash: "h".to_string(),
            is_admin: false,
        })
        .await
        .unwrap();
        assert_eq!(repo.count().await.unwrap(), 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn update_email_changes_it_and_returns_the_updated_user(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool);
        let created = repo
            .create(NewUser {
                username: "florian".to_string(),
                email: "old@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
            })
            .await
            .unwrap();

        let updated = repo
            .update_email(created.id, "new@example.com".to_string())
            .await
            .unwrap();

        assert_eq!(updated.email, "new@example.com");
        let refetched = repo.find_by_id(created.id).await.unwrap().unwrap();
        assert_eq!(refetched.email, "new@example.com");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn update_email_for_an_unknown_user_is_not_found(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool);
        let result = repo
            .update_email(Uuid::new_v4(), "new@example.com".to_string())
            .await;
        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn update_password_hash_changes_it(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool);
        let created = repo
            .create(NewUser {
                username: "florian".to_string(),
                email: "f@example.com".to_string(),
                password_hash: "old-hash".to_string(),
                is_admin: false,
            })
            .await
            .unwrap();

        repo.update_password_hash(created.id, "new-hash".to_string())
            .await
            .unwrap();

        let refetched = repo.find_by_id(created.id).await.unwrap().unwrap();
        assert_eq!(refetched.password_hash, "new-hash");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn update_password_hash_for_an_unknown_user_is_not_found(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool);
        let result = repo
            .update_password_hash(Uuid::new_v4(), "new-hash".to_string())
            .await;
        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn search_matches_on_username_and_is_never_scoped(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool);
        let match_user = repo
            .create(NewUser {
                username: "florian-widgets".to_string(),
                email: "f@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
            })
            .await
            .unwrap();
        repo.create(NewUser {
            username: "someone-else".to_string(),
            email: "s@example.com".to_string(),
            password_hash: "h".to_string(),
            is_admin: false,
        })
        .await
        .unwrap();

        let results = repo.search("widgets", 8).await.unwrap();

        assert_eq!(
            results.iter().map(|u| u.id).collect::<Vec<_>>(),
            vec![match_user.id]
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn search_excludes_a_negated_term_while_still_matching_the_positive_one(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool);
        let wanted = repo
            .create(NewUser {
                username: "widget external".to_string(),
                email: "a@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
            })
            .await
            .unwrap();
        let excluded = repo
            .create(NewUser {
                username: "widget internal".to_string(),
                email: "b@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
            })
            .await
            .unwrap();

        let results = repo.search("widget -internal", 8).await.unwrap();

        assert_eq!(
            results.iter().map(|u| u.id).collect::<Vec<_>>(),
            vec![wanted.id]
        );
        assert!(!results.iter().any(|u| u.id == excluded.id));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_freshly_created_user_starts_at_token_epoch_zero(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool);
        let created = repo
            .create(NewUser {
                username: "florian".to_string(),
                email: "f@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
            })
            .await
            .unwrap();

        assert_eq!(repo.get_token_epoch(created.id).await.unwrap(), 0);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn bump_token_epoch_increments_it_and_is_cumulative(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool);
        let created = repo
            .create(NewUser {
                username: "florian".to_string(),
                email: "f@example.com".to_string(),
                password_hash: "h".to_string(),
                is_admin: false,
            })
            .await
            .unwrap();

        repo.bump_token_epoch(created.id).await.unwrap();
        assert_eq!(repo.get_token_epoch(created.id).await.unwrap(), 1);

        repo.bump_token_epoch(created.id).await.unwrap();
        assert_eq!(repo.get_token_epoch(created.id).await.unwrap(), 2);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn get_token_epoch_for_an_unknown_user_is_not_found(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool);
        assert!(matches!(
            repo.get_token_epoch(Uuid::new_v4()).await,
            Err(DomainError::NotFound(_))
        ));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn bump_token_epoch_for_an_unknown_user_is_not_found(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool);
        assert!(matches!(
            repo.bump_token_epoch(Uuid::new_v4()).await,
            Err(DomainError::NotFound(_))
        ));
    }

    async fn create_user(repo: &PostgresUserRepository, username: &str, is_admin: bool) -> User {
        repo.create(NewUser {
            username: username.to_string(),
            email: format!("{username}@example.com"),
            password_hash: "h".to_string(),
            is_admin,
        })
        .await
        .unwrap()
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn count_admins_counts_only_the_admins(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool);
        assert_eq!(repo.count_admins().await.unwrap(), 0);
        create_user(&repo, "root", true).await;
        create_user(&repo, "alice", false).await;
        create_user(&repo, "carol", true).await;

        assert_eq!(repo.count_admins().await.unwrap(), 2);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn set_admin_promotes_and_demotes(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool);
        create_user(&repo, "root", true).await;
        let alice = create_user(&repo, "alice", false).await;

        repo.set_admin(alice.id, true).await.unwrap();
        assert!(repo.find_by_id(alice.id).await.unwrap().unwrap().is_admin);
        assert_eq!(repo.count_admins().await.unwrap(), 2);

        repo.set_admin(alice.id, false).await.unwrap();
        assert!(!repo.find_by_id(alice.id).await.unwrap().unwrap().is_admin);
        assert_eq!(repo.count_admins().await.unwrap(), 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn set_admin_is_idempotent(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool);
        let root = create_user(&repo, "root", true).await;
        let alice = create_user(&repo, "alice", false).await;

        repo.set_admin(root.id, true).await.unwrap();
        repo.set_admin(alice.id, false).await.unwrap();

        assert!(repo.find_by_id(root.id).await.unwrap().unwrap().is_admin);
        assert!(!repo.find_by_id(alice.id).await.unwrap().unwrap().is_admin);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn set_admin_refuses_to_demote_the_last_admin(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool);
        let root = create_user(&repo, "root", true).await;
        create_user(&repo, "alice", false).await;

        let result = repo.set_admin(root.id, false).await;

        assert!(
            matches!(&result, Err(DomainError::Conflict(m)) if m == "cannot remove the last administrator"),
            "{result:?}"
        );
        assert!(
            repo.find_by_id(root.id).await.unwrap().unwrap().is_admin,
            "the refused demotion changed nothing"
        );
    }

    async fn invite_pending(pool: &PgPool, user_id: Uuid) {
        sqlx::query("INSERT INTO user_invitations (user_id, token_hash, expires_at) VALUES ($1, $2, now() + interval '1 day')").bind(user_id).bind(user_id.to_string()).execute(pool).await.unwrap();
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn an_admin_still_pending_activation_does_not_count(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool.clone());
        create_user(&repo, "root", true).await;
        let carol = create_user(&repo, "carol", true).await;
        invite_pending(&pool, carol.id).await;

        assert_eq!(repo.count_admins().await.unwrap(), 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn set_admin_refuses_to_demote_the_last_active_admin_even_with_an_invited_admin_around(
        pool: PgPool,
    ) {
        let repo = PostgresUserRepository::new(pool.clone());
        let root = create_user(&repo, "root", true).await;
        let carol = create_user(&repo, "carol", true).await;
        invite_pending(&pool, carol.id).await;

        let result = repo.set_admin(root.id, false).await;

        assert!(
            matches!(&result, Err(DomainError::Conflict(m)) if m == "cannot remove the last administrator"),
            "{result:?}"
        );
        assert!(repo.find_by_id(root.id).await.unwrap().unwrap().is_admin);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn an_admin_still_pending_activation_can_always_be_demoted(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool.clone());
        create_user(&repo, "root", true).await;
        let carol = create_user(&repo, "carol", true).await;
        invite_pending(&pool, carol.id).await;

        repo.set_admin(carol.id, false).await.unwrap();

        assert!(!repo.find_by_id(carol.id).await.unwrap().unwrap().is_admin);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn once_the_invited_admin_activates_the_other_admin_may_step_down(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool.clone());
        let root = create_user(&repo, "root", true).await;
        let carol = create_user(&repo, "carol", true).await;
        invite_pending(&pool, carol.id).await;
        sqlx::query("DELETE FROM user_invitations WHERE user_id = $1")
            .bind(carol.id)
            .execute(&pool)
            .await
            .unwrap(); // what activation does

        repo.set_admin(root.id, false).await.unwrap();

        assert_eq!(repo.count_admins().await.unwrap(), 1);
    }

    async fn reset_pending(pool: &PgPool, user_id: Uuid) {
        sqlx::query("INSERT INTO password_reset_tokens (user_id, token_hash, expires_at) VALUES ($1, $2, now() + interval '1 hour')").bind(user_id).bind(user_id.to_string()).execute(pool).await.unwrap();
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn an_admin_with_a_pending_password_reset_does_not_count(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool.clone());
        create_user(&repo, "root", true).await;
        let carol = create_user(&repo, "carol", true).await;
        reset_pending(&pool, carol.id).await;

        assert_eq!(repo.count_admins().await.unwrap(), 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn set_admin_refuses_to_demote_the_last_active_admin_while_the_other_admin_has_a_pending_reset(
        pool: PgPool,
    ) {
        let repo = PostgresUserRepository::new(pool.clone());
        let root = create_user(&repo, "root", true).await;
        let carol = create_user(&repo, "carol", true).await;
        reset_pending(&pool, carol.id).await;

        let result = repo.set_admin(root.id, false).await;

        assert!(
            matches!(&result, Err(DomainError::Conflict(m)) if m == "cannot remove the last administrator"),
            "{result:?}"
        );
        assert!(repo.find_by_id(root.id).await.unwrap().unwrap().is_admin);

        sqlx::query("DELETE FROM password_reset_tokens WHERE user_id = $1")
            .bind(carol.id)
            .execute(&pool)
            .await
            .unwrap(); // what using the link does
        repo.set_admin(root.id, false).await.unwrap();
        assert_eq!(repo.count_admins().await.unwrap(), 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn an_admin_with_a_pending_password_reset_can_always_be_demoted(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool.clone());
        create_user(&repo, "root", true).await;
        let carol = create_user(&repo, "carol", true).await;
        reset_pending(&pool, carol.id).await;

        repo.set_admin(carol.id, false).await.unwrap();

        assert!(!repo.find_by_id(carol.id).await.unwrap().unwrap().is_admin);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn set_admin_for_an_unknown_user_is_not_found(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool);
        create_user(&repo, "root", true).await;

        assert!(matches!(
            repo.set_admin(Uuid::new_v4(), true).await,
            Err(DomainError::NotFound(_))
        ));
        assert!(matches!(
            repo.set_admin(Uuid::new_v4(), false).await,
            Err(DomainError::NotFound(_))
        ));
    }

    /// Without the row locks both UPDATEs would succeed and leave zero admins.
    #[sqlx::test(migrations = "../../migrations")]
    async fn concurrent_demotions_of_the_last_two_admins_leave_exactly_one(pool: PgPool) {
        let repo = std::sync::Arc::new(PostgresUserRepository::new(pool));
        let first = create_user(&repo, "root", true).await;
        let second = create_user(&repo, "carol", true).await;

        for _ in 0..20 {
            let (a, b) = tokio::join!(
                repo.set_admin(first.id, false),
                repo.set_admin(second.id, false)
            );

            assert_eq!(
                [&a, &b].iter().filter(|r| r.is_ok()).count(),
                1,
                "{a:?} / {b:?}"
            );
            assert!(
                [&a, &b]
                    .iter()
                    .any(|r| matches!(r, Err(DomainError::Conflict(_)))),
                "{a:?} / {b:?}"
            );
            assert_eq!(repo.count_admins().await.unwrap(), 1);
            repo.set_admin(first.id, true).await.unwrap();
            repo.set_admin(second.id, true).await.unwrap();
        }
    }

    fn is_last_admin_conflict<T: std::fmt::Debug>(result: &Result<T, DomainError>) -> bool {
        matches!(result, Err(DomainError::Conflict(m)) if m == "cannot remove the last administrator")
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn delete_removes_the_user(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool);
        let root = create_user(&repo, "root", true).await;
        let alice = create_user(&repo, "alice", false).await;

        repo.delete(alice.id, root.id).await.unwrap();

        assert!(repo.find_by_id(alice.id).await.unwrap().is_none());
        assert!(repo.find_by_id(root.id).await.unwrap().is_some());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn delete_of_an_unknown_user_is_not_found(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool);
        let root = create_user(&repo, "root", true).await;

        assert!(matches!(
            repo.delete(Uuid::new_v4(), root.id).await,
            Err(DomainError::NotFound(_))
        ));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn delete_refuses_the_last_active_admin_and_changes_nothing(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool.clone());
        let root = create_user(&repo, "root", true).await;
        let alice = create_user(&repo, "alice", false).await;

        let result = repo.delete(root.id, alice.id).await;

        assert!(is_last_admin_conflict(&result), "{result:?}");
        assert!(repo.find_by_id(root.id).await.unwrap().is_some());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn delete_counts_only_active_admins_toward_the_floor(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool.clone());
        let root = create_user(&repo, "root", true).await;
        let carol = create_user(&repo, "carol", true).await;
        let dave = create_user(&repo, "dave", true).await;
        invite_pending(&pool, carol.id).await;
        reset_pending(&pool, dave.id).await;

        let result = repo.delete(root.id, carol.id).await;
        assert!(is_last_admin_conflict(&result), "{result:?}");

        repo.delete(carol.id, root.id).await.unwrap();
        sqlx::query("DELETE FROM password_reset_tokens WHERE user_id = $1")
            .bind(dave.id)
            .execute(&pool)
            .await
            .unwrap(); // what using the link does
        repo.delete(root.id, dave.id).await.unwrap();
        assert_eq!(repo.count_admins().await.unwrap(), 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn concurrent_deletions_of_the_last_two_admins_leave_exactly_one(pool: PgPool) {
        let repo = std::sync::Arc::new(PostgresUserRepository::new(pool));
        for round in 0..20 {
            let first = create_user(&repo, &format!("root-{round}"), true).await;
            let second = create_user(&repo, &format!("carol-{round}"), true).await;

            let (a, b) = tokio::join!(
                repo.delete(first.id, second.id),
                repo.delete(second.id, first.id)
            );

            assert_eq!(
                [&a, &b].iter().filter(|r| r.is_ok()).count(),
                1,
                "{a:?} / {b:?}"
            );
            assert!(
                [&a, &b].iter().any(|r| is_last_admin_conflict(r)),
                "{a:?} / {b:?}"
            );
            assert_eq!(repo.count_admins().await.unwrap(), 1);
            let survivor = if a.is_ok() { second.id } else { first.id };
            sqlx::query("UPDATE users SET is_admin = false WHERE id = $1")
                .bind(survivor)
                .execute(&repo.pool)
                .await
                .unwrap();
        }
    }

    async fn seed_repository(
        pool: &PgPool,
        owner_id: Uuid,
        name: &str,
        group_id: Option<Uuid>,
    ) -> Uuid {
        sqlx::query_scalar("INSERT INTO repositories (owner_id, name, disk_path, group_id) VALUES ($1, $2, $3, $4) RETURNING id")
            .bind(owner_id)
            .bind(name)
            .bind(format!("{owner_id}/{name}.git"))
            .bind(group_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn seed_group(pool: &PgPool, name: &str, created_by: Uuid) -> Uuid {
        sqlx::query_scalar("INSERT INTO groups (name, created_by) VALUES ($1, $2) RETURNING id")
            .bind(name)
            .bind(created_by)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn repository_ids(pool: &PgPool) -> Vec<Uuid> {
        sqlx::query_scalar("SELECT id FROM repositories ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn delete_removes_the_personal_repositories_in_the_same_transaction_and_returns_them(
        pool: PgPool,
    ) {
        let repo = PostgresUserRepository::new(pool.clone());
        let root = create_user(&repo, "root", true).await;
        let alice = create_user(&repo, "alice", false).await;
        let first = seed_repository(&pool, alice.id, "first", None).await;
        let second = seed_repository(&pool, alice.id, "second", None).await;
        let roots = seed_repository(&pool, root.id, "infra", None).await;

        let deleted = repo.delete(alice.id, root.id).await.unwrap();

        let mut returned: Vec<(Uuid, String, String)> = deleted
            .iter()
            .map(|r| (r.id, r.name.clone(), r.disk_path.clone()))
            .collect();
        returned.sort();
        let mut expected = vec![
            (
                first,
                "first".to_string(),
                format!("{}/first.git", alice.id),
            ),
            (
                second,
                "second".to_string(),
                format!("{}/second.git", alice.id),
            ),
        ];
        expected.sort();
        assert_eq!(returned, expected);
        assert!(
            deleted
                .iter()
                .all(|r| r.owner_id == alice.id && r.group_id.is_none())
        );
        assert_eq!(repository_ids(&pool).await, vec![roots]);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_refused_deletion_keeps_every_personal_repository(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool.clone());
        let root = create_user(&repo, "root", true).await;
        let alice = create_user(&repo, "alice", false).await;
        let roots = seed_repository(&pool, root.id, "infra", None).await;

        let result = repo.delete(root.id, alice.id).await;

        assert!(is_last_admin_conflict(&result), "{result:?}");
        assert_eq!(repository_ids(&pool).await, vec![roots]);
    }

    /// With the user as their own heir, group repos would cascade-delete with the account.
    #[sqlx::test(migrations = "../../migrations")]
    async fn delete_refuses_the_user_as_their_own_heir_and_changes_nothing(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool.clone());
        create_user(&repo, "root", true).await;
        let alice = create_user(&repo, "alice", false).await;
        let group_id = seed_group(&pool, "acme", alice.id).await;
        seed_repository(&pool, alice.id, "api", Some(group_id)).await;
        seed_repository(&pool, alice.id, "hello", None).await;

        let result = repo.delete(alice.id, alice.id).await;

        assert!(
            matches!(&result, Err(DomainError::Validation(_))),
            "{result:?}"
        );
        assert!(repo.find_by_id(alice.id).await.unwrap().is_some());
        assert_eq!(repository_ids(&pool).await.len(), 2);
    }

    async fn add_member(pool: &PgPool, group_id: Uuid, user_id: Uuid, role: &str) {
        sqlx::query("INSERT INTO group_members (group_id, user_id, role) VALUES ($1, $2, $3)")
            .bind(group_id)
            .bind(user_id)
            .bind(role)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn seed_subgroup(pool: &PgPool, name: &str, parent: Uuid, created_by: Uuid) -> Uuid {
        sqlx::query_scalar("INSERT INTO groups (parent_group_id, name, created_by) VALUES ($1, $2, $3) RETURNING id").bind(parent).bind(name).bind(created_by).fetch_one(pool).await.unwrap()
    }

    fn is_last_maintainer_conflict<T: std::fmt::Debug>(
        result: &Result<T, DomainError>,
        path: &str,
    ) -> bool {
        matches!(result, Err(DomainError::Conflict(m)) if *m == format!("the user is the last maintainer of the group {path}; promote another member first"))
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn delete_refuses_the_last_maintainer_of_a_group_chain_before_any_write(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool.clone());
        let root = create_user(&repo, "root", true).await;
        let alice = create_user(&repo, "alice", false).await;
        let bob = create_user(&repo, "bob", false).await;
        let acme = seed_group(&pool, "acme", bob.id).await;
        add_member(&pool, acme, bob.id, "reader").await;
        let backend = seed_subgroup(&pool, "backend", acme, alice.id).await;
        add_member(&pool, backend, alice.id, "maintainer").await;
        let alices = seed_repository(&pool, alice.id, "hello", None).await;

        let result = repo.delete(alice.id, root.id).await;

        assert!(
            is_last_maintainer_conflict(&result, "acme/backend"),
            "{result:?}"
        );
        assert!(repo.find_by_id(alice.id).await.unwrap().is_some());
        assert_eq!(repository_ids(&pool).await, vec![alices]);

        sqlx::query(
            "UPDATE group_members SET role = 'maintainer' WHERE group_id = $1 AND user_id = $2",
        )
        .bind(acme)
        .bind(bob.id)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(repo.delete(alice.id, root.id).await.unwrap().len(), 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn delete_does_not_let_the_users_own_grants_cover_each_other(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool.clone());
        let root = create_user(&repo, "root", true).await;
        let alice = create_user(&repo, "alice", false).await;
        let acme = seed_group(&pool, "acme", alice.id).await;
        add_member(&pool, acme, alice.id, "maintainer").await;
        let backend = seed_subgroup(&pool, "backend", acme, alice.id).await;
        add_member(&pool, backend, alice.id, "maintainer").await;

        let result = repo.delete(alice.id, root.id).await;

        assert!(
            matches!(&result, Err(DomainError::Conflict(m)) if m.starts_with("the user is the last maintainer of the group ")),
            "{result:?}"
        );
        assert!(repo.find_by_id(alice.id).await.unwrap().is_some());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn concurrent_deletions_of_a_groups_last_two_maintainers_leave_exactly_one(pool: PgPool) {
        let repo = std::sync::Arc::new(PostgresUserRepository::new(pool.clone()));
        let root = create_user(&repo, "root", true).await;
        for round in 0..20 {
            let alice = create_user(&repo, &format!("alice-{round}"), false).await;
            let bob = create_user(&repo, &format!("bob-{round}"), false).await;
            let group = seed_group(&pool, &format!("acme-{round}"), alice.id).await;
            add_member(&pool, group, alice.id, "maintainer").await;
            add_member(&pool, group, bob.id, "maintainer").await;

            let (a, b) = tokio::join!(repo.delete(alice.id, root.id), repo.delete(bob.id, root.id));

            assert_eq!(
                [&a, &b].iter().filter(|r| r.is_ok()).count(),
                1,
                "{a:?} / {b:?}"
            );
            assert!(
                [&a, &b]
                    .iter()
                    .any(|r| is_last_maintainer_conflict(r, &format!("acme-{round}"))),
                "{a:?} / {b:?}"
            );
            let maintainers: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM group_members WHERE group_id = $1 AND role = 'maintainer'",
            )
            .bind(group)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(maintainers, 1);
        }
    }

    /// A group repo's `owner_id` cascades on user deletion, so the repo must go to the heir instead.
    #[sqlx::test(migrations = "../../migrations")]
    async fn delete_hands_the_group_repositories_the_user_created_to_the_heir(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool.clone());
        let root = create_user(&repo, "root", true).await;
        let alice = create_user(&repo, "alice", false).await;
        let bob = create_user(&repo, "bob", false).await;
        let group_id = seed_group(&pool, "acme", bob.id).await;
        let alices = seed_repository(&pool, alice.id, "api", Some(group_id)).await;
        let bobs = seed_repository(&pool, bob.id, "web", Some(group_id)).await;

        repo.delete(alice.id, root.id).await.unwrap();

        let owners: Vec<(Uuid, Uuid, Option<Uuid>)> =
            sqlx::query_as("SELECT id, owner_id, group_id FROM repositories ORDER BY name")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            owners,
            vec![
                (alices, root.id, Some(group_id)),
                (bobs, bob.id, Some(group_id))
            ]
        );
    }

    /// Content the user wrote but doesn't own survives with no author; what they owned cascades.
    #[sqlx::test(migrations = "../../migrations")]
    async fn delete_keeps_what_the_user_wrote_elsewhere_with_its_author_set_to_null(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool.clone());
        let root = create_user(&repo, "root", true).await;
        let alice = create_user(&repo, "alice", false).await;
        let bob = create_user(&repo, "bob", false).await;
        let bobs_repository = seed_repository(&pool, bob.id, "hello", None).await;
        let group_id = seed_group(&pool, "acme", alice.id).await;
        let group_repository = seed_repository(&pool, bob.id, "in-acme", Some(group_id)).await;
        let merge_request_id: Uuid = sqlx::query_scalar("INSERT INTO merge_requests (repository_id, author_id, source_branch, target_branch, title) VALUES ($1, $2, 'feature', 'main', 't') RETURNING id")
            .bind(bobs_repository)
            .bind(alice.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let bobs_merge_request: Uuid = sqlx::query_scalar("INSERT INTO merge_requests (repository_id, author_id, source_branch, target_branch, title) VALUES ($1, $2, 'fix', 'main', 't') RETURNING id")
            .bind(bobs_repository)
            .bind(bob.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let comment_id: Uuid = sqlx::query_scalar("INSERT INTO merge_request_comments (merge_request_id, author_id, body) VALUES ($1, $2, 'lgtm') RETURNING id")
            .bind(bobs_merge_request)
            .bind(alice.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let release_id: Uuid = sqlx::query_scalar("INSERT INTO releases (repository_id, tag_name, title, author_id) VALUES ($1, 'v1', 'v1', $2) RETURNING id")
            .bind(bobs_repository)
            .bind(alice.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let asset_id: Uuid = sqlx::query_scalar("INSERT INTO release_assets (release_id, filename, content_type, size_bytes, disk_path, uploaded_by) VALUES ($1, 'a.txt', 'text/plain', 1, 'x', $2) RETURNING id")
            .bind(release_id)
            .bind(alice.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO issues (repository_id, number, author_id, title, description, status, kind) VALUES ($1, 1, $2, 'bug', '', 'todo', 'bug')").bind(bobs_repository).bind(alice.id).execute(&pool).await.unwrap();

        repo.delete(alice.id, root.id).await.unwrap();

        let group: Option<Uuid> = sqlx::query_scalar("SELECT created_by FROM groups WHERE id = $1")
            .bind(group_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let merge_request: Option<Uuid> =
            sqlx::query_scalar("SELECT author_id FROM merge_requests WHERE id = $1")
                .bind(merge_request_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let comment: Option<Uuid> =
            sqlx::query_scalar("SELECT author_id FROM merge_request_comments WHERE id = $1")
                .bind(comment_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let release: Option<Uuid> =
            sqlx::query_scalar("SELECT author_id FROM releases WHERE id = $1")
                .bind(release_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let asset: Option<Uuid> =
            sqlx::query_scalar("SELECT uploaded_by FROM release_assets WHERE id = $1")
                .bind(asset_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            (group, merge_request, comment, release, asset),
            (None, None, None, None, None)
        );
        let group_repository_owner: Uuid =
            sqlx::query_scalar("SELECT owner_id FROM repositories WHERE id = $1")
                .bind(group_repository)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(group_repository_owner, bob.id);
        let issues: i64 =
            sqlx::query_scalar("SELECT count(*) FROM issues WHERE repository_id = $1")
                .bind(bobs_repository)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            issues, 0,
            "the issues a user filed cascade with them (0001, `issues_author_id_fkey`)"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn search_never_matches_on_email(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool);
        repo.create(NewUser {
            username: "florian".to_string(),
            email: "gadget-lover@example.com".to_string(),
            password_hash: "h".to_string(),
            is_admin: false,
        })
        .await
        .unwrap();

        let results = repo.search("gadget-lover", 8).await.unwrap();

        assert!(results.is_empty());
    }

    async fn seed(pool: &PgPool, username: &str, email: &str, created_at: DateTime<Utc>) -> Uuid {
        sqlx::query_scalar("INSERT INTO users (username, email, password_hash, created_at) VALUES ($1, $2, 'h', $3) RETURNING id").bind(username).bind(email).bind(created_at).fetch_one(pool).await.unwrap()
    }

    // Postgres lower() follows the database locale for non-ASCII characters, so these tests stick to ASCII case pairs.
    #[sqlx::test(migrations = "../../migrations")]
    async fn finding_by_username_ignoring_case_matches_any_casing(pool: PgPool) {
        let alice = seed(&pool, "alice", "alice@example.com", Utc::now()).await;
        seed(&pool, "bob", "bob@example.com", Utc::now()).await;
        let repo = PostgresUserRepository::new(pool);

        for probe in ["alice", "Alice", "ALICE", "aLiCe"] {
            assert_eq!(
                repo.find_by_username_ignore_case(probe)
                    .await
                    .unwrap()
                    .map(|u| u.id),
                Some(alice),
                "{probe}"
            );
        }
        assert!(
            repo.find_by_username_ignore_case("alic")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            repo.find_by_username_ignore_case("alice ")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            repo.find_by_username_ignore_case("nobody")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn finding_by_username_ignoring_case_returns_the_stored_casing_and_every_field(
        pool: PgPool,
    ) {
        let stored = seed(&pool, "MixedCase", "mixed@example.com", Utc::now()).await;
        let repo = PostgresUserRepository::new(pool);

        let found = repo
            .find_by_username_ignore_case("mixedcase")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(found.id, stored);
        assert_eq!(found.username, "MixedCase");
        assert_eq!(found.email, "mixed@example.com");
        assert_eq!(found.password_hash, "h");
        assert!(!found.is_admin);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_like_wildcard_in_the_probe_is_not_a_pattern(pool: PgPool) {
        seed(&pool, "alice", "alice@example.com", Utc::now()).await;
        let repo = PostgresUserRepository::new(pool);

        assert!(
            repo.find_by_username_ignore_case("al%")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            repo.find_by_username_ignore_case("a_ice")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            repo.find_by_email_ignore_case("%@example.com")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn finding_by_email_ignoring_case_matches_any_casing(pool: PgPool) {
        let alice = seed(&pool, "alice", "Alice@Example.com", Utc::now()).await;
        seed(&pool, "bob", "bob@example.com", Utc::now()).await;
        let repo = PostgresUserRepository::new(pool);

        for probe in [
            "alice@example.com",
            "ALICE@EXAMPLE.COM",
            "Alice@Example.com",
        ] {
            assert_eq!(
                repo.find_by_email_ignore_case(probe)
                    .await
                    .unwrap()
                    .map(|u| u.id),
                Some(alice),
                "{probe}"
            );
        }
        assert!(
            repo.find_by_email_ignore_case("alice@example.org")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            repo.find_by_email_ignore_case("nobody@example.com")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn listing_orders_by_creation_time_then_id(pool: PgPool) {
        let base = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let newest = seed(
            &pool,
            "newest",
            "n@example.com",
            base + chrono::Duration::hours(2),
        )
        .await;
        let oldest = seed(&pool, "oldest", "o@example.com", base).await;
        let tie_a = seed(
            &pool,
            "tie-a",
            "ta@example.com",
            base + chrono::Duration::hours(1),
        )
        .await;
        let tie_b = seed(
            &pool,
            "tie-b",
            "tb@example.com",
            base + chrono::Duration::hours(1),
        )
        .await;
        let (tie_first, tie_second) = if tie_a < tie_b {
            (tie_a, tie_b)
        } else {
            (tie_b, tie_a)
        };
        let repo = PostgresUserRepository::new(pool);

        let listed: Vec<Uuid> = repo
            .list(100)
            .await
            .unwrap()
            .into_iter()
            .map(|u| u.id)
            .collect();

        assert_eq!(listed, vec![oldest, tie_first, tie_second, newest]);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn listing_honours_the_limit_and_keeps_the_oldest(pool: PgPool) {
        let base = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let first = seed(&pool, "first", "1@example.com", base).await;
        let second = seed(
            &pool,
            "second",
            "2@example.com",
            base + chrono::Duration::minutes(1),
        )
        .await;
        seed(
            &pool,
            "third",
            "3@example.com",
            base + chrono::Duration::minutes(2),
        )
        .await;
        let repo = PostgresUserRepository::new(pool);

        let listed: Vec<Uuid> = repo
            .list(2)
            .await
            .unwrap()
            .into_iter()
            .map(|u| u.id)
            .collect();

        assert_eq!(listed, vec![first, second]);
        assert!(repo.list(0).await.unwrap().is_empty());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn listing_an_empty_table_is_empty_and_maps_every_column(pool: PgPool) {
        let repo = PostgresUserRepository::new(pool.clone());
        assert!(repo.list(10).await.unwrap().is_empty());

        let created_at = DateTime::from_timestamp_micros(1_700_000_000_123_456).unwrap();
        let id: Uuid = sqlx::query_scalar("INSERT INTO users (username, email, password_hash, is_admin, created_at) VALUES ('root', 'root@example.com', 'hash', true, $1) RETURNING id")
            .bind(created_at)
            .fetch_one(&pool)
            .await
            .unwrap();

        let listed = repo.list(10).await.unwrap();

        assert_eq!(listed.len(), 1);
        assert_eq!(
            (
                listed[0].id,
                listed[0].username.as_str(),
                listed[0].email.as_str(),
                listed[0].password_hash.as_str(),
                listed[0].is_admin
            ),
            (id, "root", "root@example.com", "hash", true)
        );
        assert_eq!(listed[0].created_at, created_at);
    }
}
