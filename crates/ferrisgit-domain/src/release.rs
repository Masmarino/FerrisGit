use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::error::DomainError;

#[derive(Debug, Clone, Serialize)]
pub struct Release {
    pub id: Uuid,
    pub repository_id: Uuid,
    pub tag_name: String,
    pub title: String,
    pub notes: String,
    pub draft: bool,
    pub prerelease: bool,
    /// `None` once the author's account is deleted: the release outlives them.
    pub author_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
}

pub struct NewRelease {
    pub repository_id: Uuid,
    pub tag_name: String,
    pub title: String,
    pub notes: String,
    pub draft: bool,
    pub prerelease: bool,
    pub author_id: Uuid,
}

/// Only title, notes and prerelease. Publishing a draft is a separate one-way action (`ReleaseStorePort::publish`).
#[derive(Debug, Default)]
pub struct ReleaseUpdate {
    pub title: Option<String>,
    pub notes: Option<String>,
    pub prerelease: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseAsset {
    pub id: Uuid,
    pub release_id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub disk_path: String,
    /// `None` once the uploader's account is deleted: the asset outlives them.
    pub uploaded_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

pub struct NewReleaseAsset {
    /// Picked by the caller because the id is part of the on-disk file name, which is built before the file is written.
    pub id: Uuid,
    pub release_id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub disk_path: String,
    pub uploaded_by: Uuid,
}

#[async_trait]
pub trait ReleaseStorePort: Send + Sync {
    async fn create(&self, new_release: NewRelease) -> Result<Release, DomainError>;
    /// Most recently published first, drafts by creation date. `include_drafts` is the caller's authorization decision.
    async fn list_for_repository(
        &self,
        repository_id: Uuid,
        include_drafts: bool,
    ) -> Result<Vec<Release>, DomainError>;
    async fn find_by_tag_name(
        &self,
        repository_id: Uuid,
        tag_name: &str,
    ) -> Result<Option<Release>, DomainError>;
    async fn update(
        &self,
        id: Uuid,
        repository_id: Uuid,
        update: ReleaseUpdate,
    ) -> Result<Release, DomainError>;
    /// Clears `draft` and stamps `published_at`, only the first time: an already published release keeps its date.
    async fn publish(&self, id: Uuid, repository_id: Uuid) -> Result<Release, DomainError>;
    /// Scoped by `repository_id` so a release of another repo can't be deleted by guessing its id. `NotFound` if no match.
    async fn delete(&self, id: Uuid, repository_id: Uuid) -> Result<(), DomainError>;
    async fn create_asset(&self, new_asset: NewReleaseAsset) -> Result<ReleaseAsset, DomainError>;
    async fn list_assets(&self, release_id: Uuid) -> Result<Vec<ReleaseAsset>, DomainError>;
    async fn find_asset(
        &self,
        id: Uuid,
        release_id: Uuid,
    ) -> Result<Option<ReleaseAsset>, DomainError>;
    /// Scoped by `release_id` for the same reason as `delete`. `NotFound` if no match.
    async fn delete_asset(&self, id: Uuid, release_id: Uuid) -> Result<(), DomainError>;
    /// Asset counts for several releases in one query, releases without assets missing. The default returns nothing, the
    /// real store overrides it.
    async fn asset_counts(&self, _release_ids: &[Uuid]) -> Result<HashMap<Uuid, i64>, DomainError> {
        Ok(HashMap::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_update_defaults_to_changing_nothing() {
        let update = ReleaseUpdate::default();
        assert!(update.title.is_none());
        assert!(update.notes.is_none());
        assert!(update.prerelease.is_none());
    }
}
