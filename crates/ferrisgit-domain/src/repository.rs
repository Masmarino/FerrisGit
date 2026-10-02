use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RepositoryVisibility {
    Private,
    Public,
}

impl RepositoryVisibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            RepositoryVisibility::Private => "private",
            RepositoryVisibility::Public => "public",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "private" => Ok(RepositoryVisibility::Private),
            "public" => Ok(RepositoryVisibility::Public),
            other => Err(DomainError::Validation(format!(
                "unknown visibility: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Repository {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub group_id: Option<Uuid>,
    pub description: String,
    pub disk_path: String,
    pub visibility: RepositoryVisibility,
    pub created_at: DateTime<Utc>,
}

pub struct NewRepository {
    pub owner_id: Uuid,
    pub name: String,
    pub group_id: Option<Uuid>,
    pub description: String,
    pub visibility: RepositoryVisibility,
}

#[async_trait]
pub trait RepositoryStorePort: Send + Sync {
    async fn create(
        &self,
        new_repo: NewRepository,
        disk_path: String,
    ) -> Result<Repository, DomainError>;
    async fn list_for_owner(&self, owner_id: Uuid) -> Result<Vec<Repository>, DomainError>;
    async fn find_by_owner_and_name(
        &self,
        owner_id: Uuid,
        name: &str,
    ) -> Result<Option<Repository>, DomainError>;
    async fn find_by_group_and_name(
        &self,
        group_id: Uuid,
        name: &str,
    ) -> Result<Option<Repository>, DomainError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Repository>, DomainError>;
    /// Cascades to every row referencing the repository (nothing is left pointing at it). Does not touch
    /// disk: the caller removes the bare git directory, the wiki's, and release asset files. Defaulted to
    /// `unimplemented!` like `list_public`/`search` so test doubles need no stub.
    async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
        unimplemented!("delete")
    }
    async fn list_for_group(&self, group_id: Uuid) -> Result<Vec<Repository>, DomainError>;
    /// Changes the description and/or the visibility (`None` leaves a field as is) and returns the updated
    /// repository, `NotFound` if it does not exist. The name and owner never change here: they are part of the clone
    /// URL. Defaulted to `unimplemented!` like `delete`.
    async fn update_details(
        &self,
        _id: Uuid,
        _description: Option<String>,
        _visibility: Option<RepositoryVisibility>,
    ) -> Result<Repository, DomainError> {
        unimplemented!("update_details")
    }
    /// Every repository regardless of owner or visibility, for admin-metrics aggregation. Defaulted to
    /// `unimplemented!` like `list_public`/`search`.
    async fn list_all(&self) -> Result<Vec<Repository>, DomainError> {
        unimplemented!("list_all")
    }
    /// Every id with `visibility == Public`; the only caller is `visible_repository_ids`. Defaulted to
    /// `unimplemented!` like `search`.
    async fn list_public(&self) -> Result<Vec<Uuid>, DomainError> {
        unimplemented!("list_public")
    }
    /// Full-text search restricted to `ids` (the caller's accessible set), never called with an unfiltered scope. The
    /// default body only saves test doubles a stub.
    async fn search(
        &self,
        _ids: &[Uuid],
        _query: &str,
        _limit: i64,
    ) -> Result<Vec<Repository>, DomainError> {
        unimplemented!("search")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_round_trips_through_its_string_form() {
        assert_eq!(
            RepositoryVisibility::parse("private").unwrap(),
            RepositoryVisibility::Private
        );
        assert_eq!(RepositoryVisibility::Public.as_str(), "public");
    }

    #[test]
    fn parsing_an_unknown_visibility_is_a_validation_error() {
        assert!(matches!(
            RepositoryVisibility::parse("secret"),
            Err(DomainError::Validation(_))
        ));
    }
}
