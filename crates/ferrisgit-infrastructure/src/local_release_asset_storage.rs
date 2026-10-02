use std::path::PathBuf;

use crate::error::infra;
use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::release_asset_storage::ReleaseAssetStoragePort;
use uuid::Uuid;

pub struct LocalReleaseAssetStorage {
    storage_root: PathBuf,
}

impl LocalReleaseAssetStorage {
    pub fn new(storage_root: PathBuf) -> Self {
        Self { storage_root }
    }

    /// Keeps a plain filename only: no separators, no leading dots that could form `..`.
    /// Belt and braces, the path already carries a generated asset id.
    fn sanitize_filename(filename: &str) -> String {
        let cleaned: String = filename
            .chars()
            .filter(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_'))
            .collect();
        let cleaned = cleaned.trim_start_matches('.').to_string();
        if cleaned.is_empty() {
            "file".to_string()
        } else {
            cleaned
        }
    }
}

fn ignore_not_found(result: std::io::Result<()>) -> Result<(), DomainError> {
    match result {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(infra(e)),
        _ => Ok(()),
    }
}

#[async_trait]
impl ReleaseAssetStoragePort for LocalReleaseAssetStorage {
    async fn write(
        &self,
        repository_id: Uuid,
        release_id: Uuid,
        asset_id: Uuid,
        sanitized_filename: &str,
        data: bytes::Bytes,
    ) -> Result<String, DomainError> {
        let safe_name = Self::sanitize_filename(sanitized_filename);
        let relative_path =
            format!("release-assets/{repository_id}/{release_id}/{asset_id}-{safe_name}");
        let absolute = self.storage_root.join(&relative_path);
        if let Some(parent) = absolute.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(infra)?;
        }
        tokio::fs::write(&absolute, &data).await.map_err(infra)?;
        Ok(relative_path)
    }

    fn absolute_path(&self, disk_path: &str) -> PathBuf {
        self.storage_root.join(disk_path)
    }

    async fn delete(&self, disk_path: &str) -> Result<(), DomainError> {
        let absolute = self.storage_root.join(disk_path);
        ignore_not_found(tokio::fs::remove_file(&absolute).await)
    }

    async fn delete_all_for_repository(&self, repository_id: Uuid) -> Result<(), DomainError> {
        let dir = self
            .storage_root
            .join("release-assets")
            .join(repository_id.to_string());
        ignore_not_found(tokio::fs::remove_dir_all(&dir).await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn writing_then_reading_the_absolute_path_round_trips_the_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = LocalReleaseAssetStorage::new(tmp.path().to_path_buf());
        let repository_id = Uuid::new_v4();
        let release_id = Uuid::new_v4();
        let asset_id = Uuid::new_v4();

        let disk_path = storage
            .write(
                repository_id,
                release_id,
                asset_id,
                "binary.tar.gz",
                bytes::Bytes::from_static(b"hello world"),
            )
            .await
            .unwrap();

        let contents = tokio::fs::read(storage.absolute_path(&disk_path))
            .await
            .unwrap();
        assert_eq!(contents, b"hello world");
    }

    #[tokio::test]
    async fn a_malicious_filename_can_never_escape_the_storage_root() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = LocalReleaseAssetStorage::new(tmp.path().to_path_buf());
        let repository_id = Uuid::new_v4();
        let release_id = Uuid::new_v4();
        let asset_id = Uuid::new_v4();

        let disk_path = storage
            .write(
                repository_id,
                release_id,
                asset_id,
                "../../../../etc/passwd",
                bytes::Bytes::from_static(b"pwned"),
            )
            .await
            .unwrap();
        let absolute = storage.absolute_path(&disk_path);

        assert!(
            absolute.starts_with(tmp.path()),
            "the resolved path must stay inside the storage root, got {absolute:?}"
        );
        assert!(
            !disk_path.contains(".."),
            "the sanitized disk_path itself must never contain a traversal segment, got {disk_path}"
        );
    }

    #[tokio::test]
    async fn deleting_a_file_that_does_not_exist_is_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = LocalReleaseAssetStorage::new(tmp.path().to_path_buf());

        storage
            .delete("release-assets/nonexistent/path")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn deleting_an_existing_file_removes_it_from_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = LocalReleaseAssetStorage::new(tmp.path().to_path_buf());
        let disk_path = storage
            .write(
                Uuid::new_v4(),
                Uuid::new_v4(),
                Uuid::new_v4(),
                "a.txt",
                bytes::Bytes::from_static(b"x"),
            )
            .await
            .unwrap();
        let absolute = storage.absolute_path(&disk_path);
        assert!(absolute.exists());

        storage.delete(&disk_path).await.unwrap();

        assert!(!absolute.exists());
    }

    #[tokio::test]
    async fn delete_all_for_repository_removes_every_asset_under_that_repository() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = LocalReleaseAssetStorage::new(tmp.path().to_path_buf());
        let repository_id = Uuid::new_v4();
        let other_repository_id = Uuid::new_v4();
        let release_id = Uuid::new_v4();
        let asset_id = Uuid::new_v4();
        let path = storage
            .write(
                repository_id,
                release_id,
                asset_id,
                "notes.txt",
                bytes::Bytes::from_static(b"hello"),
            )
            .await
            .unwrap();
        let other_path = storage
            .write(
                other_repository_id,
                release_id,
                asset_id,
                "notes.txt",
                bytes::Bytes::from_static(b"hello"),
            )
            .await
            .unwrap();

        storage
            .delete_all_for_repository(repository_id)
            .await
            .unwrap();

        assert!(!storage.absolute_path(&path).exists());
        assert!(
            storage.absolute_path(&other_path).exists(),
            "a different repository's assets must be untouched"
        );
    }

    #[tokio::test]
    async fn delete_all_for_repository_is_not_an_error_when_it_has_no_assets() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = LocalReleaseAssetStorage::new(tmp.path().to_path_buf());

        storage
            .delete_all_for_repository(Uuid::new_v4())
            .await
            .unwrap();
    }
}
