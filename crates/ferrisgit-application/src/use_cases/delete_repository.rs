use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::release_asset_storage::ReleaseAssetStoragePort;
use ferrisgit_domain::repository::{Repository, RepositoryStorePort};
use uuid::Uuid;

pub struct DeleteRepositoryUseCase {
    repositories: Arc<dyn RepositoryStorePort>,
    release_asset_storage: Arc<dyn ReleaseAssetStoragePort>,
}

impl DeleteRepositoryUseCase {
    pub fn new(
        repositories: Arc<dyn RepositoryStorePort>,
        release_asset_storage: Arc<dyn ReleaseAssetStoragePort>,
    ) -> Self {
        Self {
            repositories,
            release_asset_storage,
        }
    }

    /// `remove_git_storage` runs once the row is gone and gets the repository as it was. It returns nothing: the caller
    /// logs a failure, since a leftover directory is harmless and not worth a half-deleted repository.
    pub async fn execute(
        &self,
        repository_id: Uuid,
        remove_git_storage: impl FnOnce(&Repository),
    ) -> Result<(), DomainError> {
        let repo = self
            .repositories
            .find_by_id(repository_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("repository".to_string()))?;

        self.repositories.delete(repository_id).await?;
        remove_git_storage(&repo);
        self.release_asset_storage
            .delete_all_for_repository(repository_id)
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeRepositories, FakeStorage};
    use crate::use_cases::fixtures;

    fn repository() -> Repository {
        Repository {
            disk_path: "alice/hello.git".to_string(),
            ..fixtures::repository(Uuid::new_v4())
        }
    }

    #[tokio::test]
    async fn deletes_the_database_row_removes_git_storage_and_release_assets() {
        let repo = repository();
        let repositories = Arc::new(FakeRepositories::new(vec![repo.clone()]));
        let release_assets = Arc::new(FakeStorage::new());
        let use_case = DeleteRepositoryUseCase::new(repositories.clone(), release_assets.clone());
        let mut removed_storage_for: Option<String> = None;

        use_case
            .execute(repo.id, |r| removed_storage_for = Some(r.disk_path.clone()))
            .await
            .unwrap();

        assert_eq!(repositories.deleted_ids().as_slice(), &[repo.id]);
        assert_eq!(removed_storage_for, Some("alice/hello.git".to_string()));
        assert_eq!(
            release_assets.deleted_for_repository().as_slice(),
            &[repo.id]
        );
    }

    #[tokio::test]
    async fn an_unknown_repository_is_not_found_and_nothing_is_deleted() {
        let repo = repository();
        let repositories = Arc::new(FakeRepositories::new(vec![repo.clone()]));
        let release_assets = Arc::new(FakeStorage::new());
        let use_case = DeleteRepositoryUseCase::new(repositories.clone(), release_assets.clone());

        let result = use_case
            .execute(Uuid::new_v4(), |_| {
                panic!("must not run for an unknown repository")
            })
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
        assert!(repositories.deleted_ids().is_empty());
        assert!(release_assets.deleted_for_repository().is_empty());
    }
}
