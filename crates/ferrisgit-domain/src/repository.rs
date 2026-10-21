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

/// Where a repository lives: a user's personal namespace, or a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryNamespace {
    User(Uuid),
    Group(Uuid),
}

impl Repository {
    pub fn namespace(&self) -> RepositoryNamespace {
        match self.group_id {
            Some(group_id) => RepositoryNamespace::Group(group_id),
            None => RepositoryNamespace::User(self.owner_id),
        }
    }
}

/// Where the bare repository of `name` in a namespace lives under the storage root. Moving a repository moves it there.
pub fn repository_disk_path(namespace: RepositoryNamespace, name: &str) -> String {
    match namespace {
        RepositoryNamespace::User(owner_id) => format!("{owner_id}/{name}.git"),
        RepositoryNamespace::Group(group_id) => format!("groups/{group_id}/{name}.git"),
    }
}

/// A repository's new name and place. `owner_id` stays the creator's when the repository goes into a group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryRelocation {
    pub name: String,
    pub owner_id: Uuid,
    pub group_id: Option<Uuid>,
    pub disk_path: String,
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
    /// The repository that used to live at `owner_id`/`name` before a rename or a transfer, if that path has not been
    /// taken since. Defaults to none so test doubles need no stub.
    async fn find_redirect_by_owner_and_name(
        &self,
        _owner_id: Uuid,
        _name: &str,
    ) -> Result<Option<Repository>, DomainError> {
        Ok(None)
    }
    /// Same as `find_redirect_by_owner_and_name`, for a path in a group.
    async fn find_redirect_by_group_and_name(
        &self,
        _group_id: Uuid,
        _name: &str,
    ) -> Result<Option<Repository>, DomainError> {
        Ok(None)
    }
    /// Renames and/or moves the repository, its wiki's disk path along, and leaves its old path as a redirect to it.
    /// A redirect at the new path goes away. `Conflict` if a repository already has the new path, `NotFound` if missing.
    async fn relocate(
        &self,
        _id: Uuid,
        _relocation: RepositoryRelocation,
    ) -> Result<Repository, DomainError> {
        unimplemented!("relocate")
    }
    /// Cascades to every row pointing at the repository but leaves the disk alone: the caller removes the bare repo, the
    /// wiki and the release assets. Defaults to `unimplemented!` so test doubles need no stub.
    async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
        unimplemented!("delete")
    }
    async fn list_for_group(&self, group_id: Uuid) -> Result<Vec<Repository>, DomainError>;
    /// Changes the description and/or visibility (`None` keeps the field) and returns the updated repository. Name and owner
    /// never change here since they're part of the clone URL. `NotFound` if missing.
    async fn update_details(
        &self,
        _id: Uuid,
        _description: Option<String>,
        _visibility: Option<RepositoryVisibility>,
    ) -> Result<Repository, DomainError> {
        unimplemented!("update_details")
    }
    /// Every repository, whatever the owner or visibility, for admin metrics.
    async fn list_all(&self) -> Result<Vec<Repository>, DomainError> {
        unimplemented!("list_all")
    }
    /// Ids of all public repositories. Only `visible_repository_ids` calls it.
    async fn list_public(&self) -> Result<Vec<Uuid>, DomainError> {
        unimplemented!("list_public")
    }
    /// Full-text search within `ids`, the caller's accessible set. Never call it with an unfiltered scope.
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
    fn a_disk_path_is_made_of_the_namespace_and_the_name() {
        let id = Uuid::nil();
        assert_eq!(
            repository_disk_path(RepositoryNamespace::User(id), "hello"),
            format!("{id}/hello.git")
        );
        assert_eq!(
            repository_disk_path(RepositoryNamespace::Group(id), "hello"),
            format!("groups/{id}/hello.git")
        );
    }

    #[test]
    fn parsing_an_unknown_visibility_is_a_validation_error() {
        assert!(matches!(
            RepositoryVisibility::parse("secret"),
            Err(DomainError::Validation(_))
        ));
    }
}
