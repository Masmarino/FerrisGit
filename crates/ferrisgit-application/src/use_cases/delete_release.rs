use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::release::ReleaseStorePort;
use ferrisgit_domain::release_asset_storage::ReleaseAssetStoragePort;
use uuid::Uuid;

pub struct DeleteReleaseUseCase {
    releases: Arc<dyn ReleaseStorePort>,
    asset_storage: Arc<dyn ReleaseAssetStoragePort>,
}

impl DeleteReleaseUseCase {
    pub fn new(
        releases: Arc<dyn ReleaseStorePort>,
        asset_storage: Arc<dyn ReleaseAssetStoragePort>,
    ) -> Self {
        Self {
            releases,
            asset_storage,
        }
    }

    /// The assets are listed first because the cascade deletes their rows with the release, and we need their paths to
    /// remove the files. A file that fails to delete is only logged: the release is gone either way.
    pub async fn execute(&self, release_id: Uuid, repository_id: Uuid) -> Result<(), DomainError> {
        let assets = self.releases.list_assets(release_id).await?;
        self.releases.delete(release_id, repository_id).await?;
        for asset in assets {
            if let Err(err) = self.asset_storage.delete(&asset.disk_path).await {
                tracing::error!(error = %err, asset_id = %asset.id, disk_path = %asset.disk_path, "failed to delete a release asset's file from disk after its release row was deleted");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeReleases, FakeStorage};
    use crate::use_cases::fixtures::{release, release_asset};

    #[tokio::test]
    async fn deleting_a_release_deletes_the_row_then_every_asset_file() {
        let release_id = Uuid::new_v4();
        let repository_id = Uuid::new_v4();
        let releases = Arc::new(
            FakeReleases::new(vec![release(release_id, repository_id)]).with_assets(vec![
                release_asset(release_id, "path/a"),
                release_asset(release_id, "path/b"),
            ]),
        );
        let storage = Arc::new(FakeStorage::new());
        let use_case = DeleteReleaseUseCase::new(releases.clone(), storage.clone());

        use_case.execute(release_id, repository_id).await.unwrap();

        assert_eq!(
            releases.deleted_release_ids(),
            vec![(release_id, repository_id)]
        );
        let mut deleted = storage.deleted();
        deleted.sort();
        assert_eq!(deleted, vec!["path/a".to_string(), "path/b".to_string()]);
    }

    #[tokio::test]
    async fn a_file_deletion_failure_does_not_fail_the_use_case_once_the_row_is_gone() {
        let release_id = Uuid::new_v4();
        let repository_id = Uuid::new_v4();
        let releases = Arc::new(
            FakeReleases::new(vec![release(release_id, repository_id)])
                .with_assets(vec![release_asset(release_id, "path/a")]),
        );
        let storage = Arc::new(FakeStorage::failing());
        let use_case = DeleteReleaseUseCase::new(releases.clone(), storage.clone());

        let result = use_case.execute(release_id, repository_id).await;

        assert!(
            result.is_ok(),
            "a failed file cleanup must not undo an already-successful database deletion"
        );
        assert_eq!(releases.deleted_release_ids().len(), 1);
    }

    #[tokio::test]
    async fn deleting_a_release_with_no_assets_is_a_no_op_beyond_the_row_itself() {
        let release_id = Uuid::new_v4();
        let repository_id = Uuid::new_v4();
        let releases = Arc::new(FakeReleases::new(vec![release(release_id, repository_id)]));
        let storage = Arc::new(FakeStorage::new());
        let use_case = DeleteReleaseUseCase::new(releases.clone(), storage.clone());

        use_case.execute(release_id, repository_id).await.unwrap();

        assert!(storage.deleted().is_empty());
    }
}
