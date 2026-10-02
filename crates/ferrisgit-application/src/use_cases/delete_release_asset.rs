use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::release::ReleaseStorePort;
use ferrisgit_domain::release_asset_storage::ReleaseAssetStoragePort;
use uuid::Uuid;

pub struct DeleteReleaseAssetUseCase {
    releases: Arc<dyn ReleaseStorePort>,
    asset_storage: Arc<dyn ReleaseAssetStoragePort>,
}

impl DeleteReleaseAssetUseCase {
    pub fn new(
        releases: Arc<dyn ReleaseStorePort>,
        asset_storage: Arc<dyn ReleaseAssetStoragePort>,
    ) -> Self {
        Self {
            releases,
            asset_storage,
        }
    }

    /// Deletes the row, then the file. As for a whole release, a file that fails to delete is only logged.
    pub async fn execute(&self, asset_id: Uuid, release_id: Uuid) -> Result<(), DomainError> {
        let asset = self
            .releases
            .find_asset(asset_id, release_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("release asset".to_string()))?;
        self.releases.delete_asset(asset_id, release_id).await?;
        if let Err(err) = self.asset_storage.delete(&asset.disk_path).await {
            tracing::error!(error = %err, asset_id = %asset.id, disk_path = %asset.disk_path, "failed to delete a release asset's file from disk after its row was deleted");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeReleases, FakeStorage};
    use crate::use_cases::fixtures::release_asset;

    #[tokio::test]
    async fn deleting_an_asset_deletes_the_row_then_the_file() {
        let release_id = Uuid::new_v4();
        let asset = release_asset(release_id, "release-assets/x/y/z");
        let asset_id = asset.id;
        let releases = Arc::new(FakeReleases::empty().with_assets(vec![asset]));
        let storage = Arc::new(FakeStorage::new());
        let use_case = DeleteReleaseAssetUseCase::new(releases.clone(), storage.clone());

        use_case.execute(asset_id, release_id).await.unwrap();

        assert_eq!(releases.deleted_asset_ids(), vec![(asset_id, release_id)]);
        assert_eq!(storage.deleted(), vec!["release-assets/x/y/z".to_string()]);
    }

    #[tokio::test]
    async fn deleting_an_unknown_asset_is_not_found_and_never_touches_the_filesystem() {
        let releases = Arc::new(FakeReleases::empty());
        let storage = Arc::new(FakeStorage::new());
        let use_case = DeleteReleaseAssetUseCase::new(releases.clone(), storage.clone());

        let result = use_case.execute(Uuid::new_v4(), Uuid::new_v4()).await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
        assert!(releases.deleted_asset_ids().is_empty());
        assert!(storage.deleted().is_empty());
    }
}
