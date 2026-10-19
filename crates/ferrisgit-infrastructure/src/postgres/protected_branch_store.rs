use crate::error::{conflict_on_duplicate, infra};
use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::protected_branch::{
    NewProtectedBranch, ProtectedBranch, ProtectedBranchStorePort, ProtectedBranchUpdate,
};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresProtectedBranchStore {
    pool: PgPool,
}

impl PostgresProtectedBranchStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProtectedBranchStorePort for PostgresProtectedBranchStore {
    async fn list_for_repository(
        &self,
        repository_id: Uuid,
    ) -> Result<Vec<ProtectedBranch>, DomainError> {
        sqlx::query_as!(
            ProtectedBranch,
            "SELECT id, repository_id, pattern, required_approvals, required_checks, created_at FROM protected_branches WHERE repository_id = $1 ORDER BY created_at, pattern",
            repository_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)
    }

    async fn create(&self, new: NewProtectedBranch) -> Result<ProtectedBranch, DomainError> {
        let pattern = new.pattern.clone();
        sqlx::query_as!(
            ProtectedBranch,
            "INSERT INTO protected_branches (repository_id, pattern, required_approvals, required_checks) VALUES ($1, $2, $3, $4) RETURNING id, repository_id, pattern, required_approvals, required_checks, created_at",
            new.repository_id,
            new.pattern,
            new.required_approvals,
            &new.required_checks
        )
        .fetch_one(&self.pool)
        .await
        .map_err(conflict_on_duplicate(|| {
            format!("branch pattern {pattern} is already protected")
        }))
    }

    async fn update(
        &self,
        repository_id: Uuid,
        id: Uuid,
        update: ProtectedBranchUpdate,
    ) -> Result<ProtectedBranch, DomainError> {
        sqlx::query_as!(
            ProtectedBranch,
            "UPDATE protected_branches SET required_approvals = COALESCE($3, required_approvals), required_checks = COALESCE($4, required_checks) WHERE id = $1 AND repository_id = $2 RETURNING id, repository_id, pattern, required_approvals, required_checks, created_at",
            id,
            repository_id,
            update.required_approvals,
            update.required_checks.as_deref()
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?
        .ok_or_else(|| DomainError::NotFound("protected branch".to_string()))
    }

    async fn delete(&self, repository_id: Uuid, id: Uuid) -> Result<(), DomainError> {
        let result = sqlx::query!(
            "DELETE FROM protected_branches WHERE id = $1 AND repository_id = $2",
            id,
            repository_id
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound("protected branch".to_string()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::test_support::{seed_owned_repository, seed_repository};

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_protected_pattern_is_listed_until_it_is_deleted(pool: PgPool) {
        let (_, repository_id) = seed_owned_repository(&pool, "florian").await;
        let store = PostgresProtectedBranchStore::new(pool);

        let main = store
            .create(NewProtectedBranch {
                repository_id,
                pattern: "main".to_string(),
                required_approvals: 0,
                required_checks: vec![],
            })
            .await
            .unwrap();
        store
            .create(NewProtectedBranch {
                repository_id,
                pattern: "release/*".to_string(),
                required_approvals: 0,
                required_checks: vec![],
            })
            .await
            .unwrap();

        let patterns: Vec<String> = store
            .list_for_repository(repository_id)
            .await
            .unwrap()
            .into_iter()
            .map(|p| p.pattern)
            .collect();
        assert_eq!(patterns, vec!["main", "release/*"]);

        store.delete(repository_id, main.id).await.unwrap();
        let left = store.list_for_repository(repository_id).await.unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].pattern, "release/*");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn protecting_the_same_pattern_twice_is_a_conflict(pool: PgPool) {
        let (_, repository_id) = seed_owned_repository(&pool, "florian").await;
        let store = PostgresProtectedBranchStore::new(pool);
        let new = || NewProtectedBranch {
            repository_id,
            pattern: "main".to_string(),
            required_approvals: 0,
            required_checks: vec![],
        };

        store.create(new()).await.unwrap();
        assert!(matches!(
            store.create(new()).await,
            Err(DomainError::Conflict(_))
        ));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_rule_of_another_repository_cannot_be_deleted_through_this_one(pool: PgPool) {
        let (owner_id, repository_id) = seed_owned_repository(&pool, "florian").await;
        let other_repository_id = seed_repository(&pool, owner_id, "other").await;
        let store = PostgresProtectedBranchStore::new(pool);
        let rule = store
            .create(NewProtectedBranch {
                repository_id,
                pattern: "main".to_string(),
                required_approvals: 0,
                required_checks: vec![],
            })
            .await
            .unwrap();

        assert!(matches!(
            store.delete(other_repository_id, rule.id).await,
            Err(DomainError::NotFound(_))
        ));
        assert_eq!(
            store
                .list_for_repository(repository_id)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_rule_keeps_its_requirements_and_they_can_be_changed(pool: PgPool) {
        let (_, repository_id) = seed_owned_repository(&pool, "florian").await;
        let store = PostgresProtectedBranchStore::new(pool);
        let rule = store
            .create(NewProtectedBranch {
                repository_id,
                pattern: "main".to_string(),
                required_approvals: 2,
                required_checks: vec!["pipeline".to_string()],
            })
            .await
            .unwrap();
        assert_eq!(rule.required_approvals, 2);
        assert_eq!(rule.required_checks, vec!["pipeline"]);

        let updated = store
            .update(
                repository_id,
                rule.id,
                ProtectedBranchUpdate {
                    required_checks: Some(vec!["pipeline/lint".to_string()]),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.required_approvals, 2, "a field left out is kept");
        assert_eq!(updated.required_checks, vec!["pipeline/lint"]);
        assert!(matches!(
            store
                .update(Uuid::new_v4(), rule.id, ProtectedBranchUpdate::default())
                .await,
            Err(DomainError::NotFound(_))
        ));
    }
}
