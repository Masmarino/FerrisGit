use crate::error::{conflict_on_duplicate, infra};
use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::repository::{Repository, RepositoryVisibility};
use ferrisgit_domain::repository_collaborator::{
    CollaboratedRepository, CollaboratorRole, RepositoryCollaborator,
    RepositoryCollaboratorStorePort,
};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresRepositoryCollaboratorStore {
    pool: PgPool,
}

impl PostgresRepositoryCollaboratorStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct Row {
    repository_id: Uuid,
    user_id: Uuid,
    username: String,
    role: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<Row> for RepositoryCollaborator {
    type Error = DomainError;
    fn try_from(row: Row) -> Result<Self, DomainError> {
        Ok(RepositoryCollaborator {
            repository_id: row.repository_id,
            user_id: row.user_id,
            username: row.username,
            role: CollaboratorRole::parse(&row.role)?,
            created_at: row.created_at,
        })
    }
}

#[async_trait]
impl RepositoryCollaboratorStorePort for PostgresRepositoryCollaboratorStore {
    async fn add(
        &self,
        repository_id: Uuid,
        user_id: Uuid,
        role: CollaboratorRole,
    ) -> Result<(), DomainError> {
        sqlx::query!("INSERT INTO repository_collaborators (repository_id, user_id, role) VALUES ($1, $2, $3)", repository_id, user_id, role.as_str())
            .execute(&self.pool)
            .await
            .map_err(conflict_on_duplicate(|| {
                "already a collaborator".to_string()
            }))?;
        Ok(())
    }

    async fn set_role(
        &self,
        repository_id: Uuid,
        user_id: Uuid,
        role: CollaboratorRole,
    ) -> Result<(), DomainError> {
        let result = sqlx::query!("UPDATE repository_collaborators SET role = $1 WHERE repository_id = $2 AND user_id = $3", role.as_str(), repository_id, user_id)
            .execute(&self.pool)
            .await
            .map_err(infra)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound("collaborator".to_string()));
        }
        Ok(())
    }

    async fn remove(&self, repository_id: Uuid, user_id: Uuid) -> Result<(), DomainError> {
        sqlx::query!(
            "DELETE FROM repository_collaborators WHERE repository_id = $1 AND user_id = $2",
            repository_id,
            user_id
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }

    async fn list_for_repository(
        &self,
        repository_id: Uuid,
    ) -> Result<Vec<RepositoryCollaborator>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT rc.repository_id, rc.user_id, u.username, rc.role, rc.created_at \
             FROM repository_collaborators rc JOIN users u ON u.id = rc.user_id \
             WHERE rc.repository_id = $1 ORDER BY rc.created_at ASC, u.username ASC",
            repository_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        rows.into_iter().map(TryFrom::try_from).collect()
    }

    async fn get_role(
        &self,
        repository_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<CollaboratorRole>, DomainError> {
        let row = sqlx::query!(
            "SELECT role FROM repository_collaborators WHERE repository_id = $1 AND user_id = $2",
            repository_id,
            user_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;
        row.map(|r| CollaboratorRole::parse(&r.role)).transpose()
    }

    async fn list_repositories_for_collaborator(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<Uuid>, DomainError> {
        let rows = sqlx::query!(
            "SELECT repository_id FROM repository_collaborators WHERE user_id = $1",
            user_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows.into_iter().map(|r| r.repository_id).collect())
    }

    async fn list_collaborations_for_user(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<CollaboratedRepository>, DomainError> {
        let rows = sqlx::query!(
            "SELECT r.id, r.owner_id, r.name, r.group_id, r.description, r.disk_path, r.visibility, r.created_at, \
                    owner.username AS owner_username, rc.role \
             FROM repository_collaborators rc \
             JOIN repositories r ON r.id = rc.repository_id \
             JOIN users owner ON owner.id = r.owner_id \
             WHERE rc.user_id = $1",
            user_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        rows.into_iter()
            .map(|row| {
                Ok(CollaboratedRepository {
                    repository: Repository {
                        id: row.id,
                        owner_id: row.owner_id,
                        name: row.name,
                        group_id: row.group_id,
                        description: row.description,
                        disk_path: row.disk_path,
                        visibility: RepositoryVisibility::parse(&row.visibility)?,
                        created_at: row.created_at,
                    },
                    owner_username: row.owner_username,
                    role: CollaboratorRole::parse(&row.role)?,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::test_support::{seed_repository, seed_user};

    #[sqlx::test(migrations = "../../migrations")]
    async fn adding_then_listing_a_collaborator_returns_their_username_and_role(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let collaborator_id = seed_user(&pool, "collaborator").await;
        let repo_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresRepositoryCollaboratorStore::new(pool);

        store
            .add(repo_id, collaborator_id, CollaboratorRole::Reader)
            .await
            .unwrap();

        let listed = store.list_for_repository(repo_id).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].username, "collaborator");
        assert_eq!(listed[0].role, CollaboratorRole::Reader);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn get_role_is_none_until_added_then_some_the_added_role(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let collaborator_id = seed_user(&pool, "collaborator").await;
        let repo_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresRepositoryCollaboratorStore::new(pool);

        assert_eq!(
            store.get_role(repo_id, collaborator_id).await.unwrap(),
            None
        );
        store
            .add(repo_id, collaborator_id, CollaboratorRole::Contributor)
            .await
            .unwrap();
        assert_eq!(
            store.get_role(repo_id, collaborator_id).await.unwrap(),
            Some(CollaboratorRole::Contributor)
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn set_role_updates_an_existing_collaborators_role(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let collaborator_id = seed_user(&pool, "collaborator").await;
        let repo_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresRepositoryCollaboratorStore::new(pool);
        store
            .add(repo_id, collaborator_id, CollaboratorRole::Reader)
            .await
            .unwrap();

        store
            .set_role(repo_id, collaborator_id, CollaboratorRole::Maintainer)
            .await
            .unwrap();

        assert_eq!(
            store.get_role(repo_id, collaborator_id).await.unwrap(),
            Some(CollaboratorRole::Maintainer)
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn set_role_on_a_missing_pair_returns_not_found(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let repo_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresRepositoryCollaboratorStore::new(pool);

        let result = store
            .set_role(repo_id, Uuid::new_v4(), CollaboratorRole::Maintainer)
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn removing_a_pair_that_does_not_exist_is_not_an_error(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let repo_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresRepositoryCollaboratorStore::new(pool);

        store.remove(repo_id, Uuid::new_v4()).await.unwrap();
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn adding_the_same_pair_twice_returns_a_conflict(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let collaborator_id = seed_user(&pool, "collaborator").await;
        let repo_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresRepositoryCollaboratorStore::new(pool);

        store
            .add(repo_id, collaborator_id, CollaboratorRole::Contributor)
            .await
            .unwrap();
        let result = store
            .add(repo_id, collaborator_id, CollaboratorRole::Contributor)
            .await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_repositories_for_collaborator_returns_only_repos_that_user_collaborates_on(
        pool: PgPool,
    ) {
        let owner_id = seed_user(&pool, "owner").await;
        let collaborator_id = seed_user(&pool, "collaborator").await;
        let owned_by_collaborator = seed_repository(&pool, collaborator_id, "own-repo").await;
        let collaborated_repo = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresRepositoryCollaboratorStore::new(pool);
        store
            .add(
                collaborated_repo,
                collaborator_id,
                CollaboratorRole::Contributor,
            )
            .await
            .unwrap();

        let result = store
            .list_repositories_for_collaborator(collaborator_id)
            .await
            .unwrap();

        assert_eq!(result, vec![collaborated_repo]);
        assert!(!result.contains(&owned_by_collaborator));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_collaborations_for_user_joins_the_owner_username_and_role_in_one_query(
        pool: PgPool,
    ) {
        let owner_id = seed_user(&pool, "owner").await;
        let collaborator_id = seed_user(&pool, "collaborator").await;
        let owned_by_collaborator = seed_repository(&pool, collaborator_id, "own-repo").await;
        let collaborated_repo = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresRepositoryCollaboratorStore::new(pool);
        store
            .add(
                collaborated_repo,
                collaborator_id,
                CollaboratorRole::Maintainer,
            )
            .await
            .unwrap();

        let result = store
            .list_collaborations_for_user(collaborator_id)
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].repository.id, collaborated_repo);
        assert_eq!(result[0].owner_username, "owner");
        assert_eq!(result[0].role, CollaboratorRole::Maintainer);
        assert!(
            !result
                .iter()
                .any(|c| c.repository.id == owned_by_collaborator),
            "a repo the user owns outright must not appear as a collaboration"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_row_inserted_without_a_role_backfills_to_contributor(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let collaborator_id = seed_user(&pool, "collaborator").await;
        let repo_id = seed_repository(&pool, owner_id, "hello").await;
        // goes around the port to check that the column DEFAULT itself does the backfill
        sqlx::query!(
            "INSERT INTO repository_collaborators (repository_id, user_id) VALUES ($1, $2)",
            repo_id,
            collaborator_id
        )
        .execute(&pool)
        .await
        .unwrap();
        let store = PostgresRepositoryCollaboratorStore::new(pool);

        assert_eq!(
            store.get_role(repo_id, collaborator_id).await.unwrap(),
            Some(CollaboratorRole::Contributor)
        );
    }
}
