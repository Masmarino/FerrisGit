use async_trait::async_trait;
use uuid::Uuid;

use crate::error::DomainError;

/// Binary attachment storage: a filesystem concern kept separate from `ReleaseStorePort`, which owns the metadata
/// rows recording where the bytes live. Callers use both together.
#[async_trait]
pub trait ReleaseAssetStoragePort: Send + Sync {
    /// Writes `data` and returns the `disk_path` (relative to `STORAGE_ROOT`) to persist in the metadata row.
    ///
    /// Takes `bytes::Bytes` so the multipart-decoded buffer passes through without a second copy (uploads go up to
    /// 100MB).
    async fn write(
        &self,
        repository_id: Uuid,
        release_id: Uuid,
        asset_id: Uuid,
        sanitized_filename: &str,
        data: bytes::Bytes,
    ) -> Result<String, DomainError>;
    /// Resolves a stored `disk_path` to a filesystem path. Pure path arithmetic: no I/O, cannot fail.
    fn absolute_path(&self, disk_path: &str) -> std::path::PathBuf;
    async fn delete(&self, disk_path: &str) -> Result<(), DomainError>;
    /// Best-effort cleanup for a deleted repository: removes every asset file under it without enumerating
    /// `release_assets` rows (the layout is namespaced by `repository_id`).
    async fn delete_all_for_repository(&self, repository_id: Uuid) -> Result<(), DomainError>;
}
