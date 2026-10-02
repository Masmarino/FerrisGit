use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::release::{NewReleaseAsset, ReleaseAsset, ReleaseStorePort};
use ferrisgit_domain::release_asset_storage::ReleaseAssetStoragePort;
use uuid::Uuid;

pub struct UploadReleaseAssetUseCase {
    releases: Arc<dyn ReleaseStorePort>,
    asset_storage: Arc<dyn ReleaseAssetStoragePort>,
}

impl UploadReleaseAssetUseCase {
    pub fn new(
        releases: Arc<dyn ReleaseStorePort>,
        asset_storage: Arc<dyn ReleaseAssetStoragePort>,
    ) -> Self {
        Self {
            releases,
            asset_storage,
        }
    }

    /// One generated id is used for both the file on disk and the row.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        repository_id: Uuid,
        release_id: Uuid,
        filename: String,
        content_type: String,
        data: bytes::Bytes,
        uploaded_by: Uuid,
    ) -> Result<ReleaseAsset, DomainError> {
        let asset_id = Uuid::new_v4();
        let size_bytes = data.len() as i64;
        let disk_path = self
            .asset_storage
            .write(repository_id, release_id, asset_id, &filename, data)
            .await?;
        self.releases
            .create_asset(NewReleaseAsset {
                id: asset_id,
                release_id,
                filename,
                content_type,
                size_bytes,
                disk_path,
                uploaded_by,
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::{FakeReleases, FakeStorage};

    #[tokio::test]
    async fn uploading_an_asset_writes_the_file_then_the_row_with_the_same_id() {
        let releases = Arc::new(FakeReleases::empty());
        let storage = Arc::new(FakeStorage::new());
        let use_case = UploadReleaseAssetUseCase::new(releases.clone(), storage.clone());
        let repository_id = Uuid::new_v4();
        let release_id = Uuid::new_v4();

        let asset = use_case
            .execute(
                repository_id,
                release_id,
                "binary.tar.gz".to_string(),
                "application/gzip".to_string(),
                bytes::Bytes::from_static(b"hello"),
                Uuid::new_v4(),
            )
            .await
            .unwrap();

        let writes = storage.writes();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].0, repository_id);
        assert_eq!(writes[0].1, release_id);
        assert_eq!(
            writes[0].2, asset.id,
            "the id used to build the disk file name must be the same id the returned asset carries"
        );

        let rows = releases.assets_snapshot();
        assert_eq!(rows[0].id, asset.id);
        assert_eq!(asset.size_bytes, 5);
    }
}
