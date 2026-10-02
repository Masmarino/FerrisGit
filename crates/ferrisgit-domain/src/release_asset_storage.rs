use async_trait::async_trait;
use uuid::Uuid;

use crate::error::DomainError;

/// Where the bytes of release attachments live, separate from `ReleaseStorePort` which keeps the metadata rows pointing
/// at them. Callers use both.
#[async_trait]
pub trait ReleaseAssetStoragePort: Send + Sync {
    /// Writes `data` and returns the `disk_path`, relative to `STORAGE_ROOT`, to store in the metadata row. Takes
    /// `bytes::Bytes` so the multipart buffer isn't copied again (uploads go up to 100MB).
    async fn write(
        &self,
        repository_id: Uuid,
        release_id: Uuid,
        asset_id: Uuid,
        sanitized_filename: &str,
        data: bytes::Bytes,
    ) -> Result<String, DomainError>;
    /// Maps a stored `disk_path` to a filesystem path. No I/O, can't fail.
    fn absolute_path(&self, disk_path: &str) -> std::path::PathBuf;
    async fn delete(&self, disk_path: &str) -> Result<(), DomainError>;
    /// Best-effort cleanup for a deleted repository. Everything is namespaced by `repository_id`, so there's no need to list
    /// `release_assets`.
    async fn delete_all_for_repository(&self, repository_id: Uuid) -> Result<(), DomainError>;
}
