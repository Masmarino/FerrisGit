use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::DomainError;
use crate::repository::Repository;

/// Declaration order is the ordering, so `role >= CollaboratorRole::X` is a valid tier check. No `Serialize`:
/// responses go through `as_str()` into a DTO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CollaboratorRole {
    Reader,
    Contributor,
    Maintainer,
}

impl CollaboratorRole {
    pub fn parse(s: &str) -> Result<Self, DomainError> {
        match s {
            "reader" => Ok(CollaboratorRole::Reader),
            "contributor" => Ok(CollaboratorRole::Contributor),
            "maintainer" => Ok(CollaboratorRole::Maintainer),
            other => Err(DomainError::Validation(format!("invalid role: {other}"))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            CollaboratorRole::Reader => "reader",
            CollaboratorRole::Contributor => "contributor",
            CollaboratorRole::Maintainer => "maintainer",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RepositoryCollaborator {
    pub repository_id: Uuid,
    pub user_id: Uuid,
    pub username: String,
    pub role: CollaboratorRole,
    pub created_at: DateTime<Utc>,
}

/// A repository the user collaborates on but does not own, with the owner's username and the user's role, so `GET
/// /repositories` needs no further query per row.
#[derive(Debug, Clone)]
pub struct CollaboratedRepository {
    pub repository: Repository,
    pub owner_username: String,
    pub role: CollaboratorRole,
}

/// Who besides the owner has standing access to a repository, and at what role. Checked on nearly every repo-scoped
/// request, hence a lean port separate from `RepositorySettingsStorePort`.
#[async_trait]
pub trait RepositoryCollaboratorStorePort: Send + Sync {
    /// `Conflict` if that pair already exists.
    async fn add(
        &self,
        repository_id: Uuid,
        user_id: Uuid,
        role: CollaboratorRole,
    ) -> Result<(), DomainError>;
    /// `NotFound("collaborator")` if the pair doesn't exist, unlike `remove`: the caller asked for a role to take
    /// effect.
    async fn set_role(
        &self,
        repository_id: Uuid,
        user_id: Uuid,
        role: CollaboratorRole,
    ) -> Result<(), DomainError>;
    /// A no-op (not an error) if the pair doesn't exist.
    async fn remove(&self, repository_id: Uuid, user_id: Uuid) -> Result<(), DomainError>;
    async fn list_for_repository(
        &self,
        repository_id: Uuid,
    ) -> Result<Vec<RepositoryCollaborator>, DomainError>;
    /// `None` if they aren't a collaborator. The hot path of every access check.
    async fn get_role(
        &self,
        repository_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<CollaboratorRole>, DomainError>;
    async fn list_repositories_for_collaborator(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<Uuid>, DomainError>;
    /// Same repositories as `list_repositories_for_collaborator` with owner username and role in one query (`GET
    /// /repositories`). Defaulted to `unimplemented!()` so test doubles that never use it need no stub.
    async fn list_collaborations_for_user(
        &self,
        _user_id: Uuid,
    ) -> Result<Vec<CollaboratedRepository>, DomainError> {
        unimplemented!("list_collaborations_for_user")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::Mutex;

    struct FakeCollaborators(Mutex<Vec<(Uuid, Uuid, CollaboratorRole)>>);

    #[async_trait]
    impl RepositoryCollaboratorStorePort for FakeCollaborators {
        async fn add(
            &self,
            repository_id: Uuid,
            user_id: Uuid,
            role: CollaboratorRole,
        ) -> Result<(), DomainError> {
            self.0.lock().unwrap().push((repository_id, user_id, role));
            Ok(())
        }
        async fn set_role(
            &self,
            repository_id: Uuid,
            user_id: Uuid,
            role: CollaboratorRole,
        ) -> Result<(), DomainError> {
            let mut rows = self.0.lock().unwrap();
            let Some(row) = rows
                .iter_mut()
                .find(|(r, u, _)| *r == repository_id && *u == user_id)
            else {
                return Err(DomainError::NotFound("collaborator".to_string()));
            };
            row.2 = role;
            Ok(())
        }
        async fn remove(&self, repository_id: Uuid, user_id: Uuid) -> Result<(), DomainError> {
            self.0
                .lock()
                .unwrap()
                .retain(|(r, u, _)| !(*r == repository_id && *u == user_id));
            Ok(())
        }
        async fn list_for_repository(
            &self,
            repository_id: Uuid,
        ) -> Result<Vec<RepositoryCollaborator>, DomainError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|(r, _, _)| *r == repository_id)
                .map(|(r, u, role)| RepositoryCollaborator {
                    repository_id: *r,
                    user_id: *u,
                    username: "someone".to_string(),
                    role: *role,
                    created_at: Utc::now(),
                })
                .collect())
        }
        async fn get_role(
            &self,
            repository_id: Uuid,
            user_id: Uuid,
        ) -> Result<Option<CollaboratorRole>, DomainError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .find(|(r, u, _)| *r == repository_id && *u == user_id)
                .map(|(_, _, role)| *role))
        }
        async fn list_repositories_for_collaborator(
            &self,
            user_id: Uuid,
        ) -> Result<Vec<Uuid>, DomainError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, u, _)| *u == user_id)
                .map(|(r, _, _)| *r)
                .collect())
        }
    }

    #[tokio::test]
    async fn a_port_implementation_can_be_exercised_through_the_trait_object() {
        let store: Arc<dyn RepositoryCollaboratorStorePort> =
            Arc::new(FakeCollaborators(Mutex::new(vec![])));
        let repo_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        assert_eq!(store.get_role(repo_id, user_id).await.unwrap(), None);
        store
            .add(repo_id, user_id, CollaboratorRole::Contributor)
            .await
            .unwrap();
        assert_eq!(
            store.get_role(repo_id, user_id).await.unwrap(),
            Some(CollaboratorRole::Contributor)
        );
        assert_eq!(
            store
                .list_repositories_for_collaborator(user_id)
                .await
                .unwrap(),
            vec![repo_id]
        );

        store
            .set_role(repo_id, user_id, CollaboratorRole::Maintainer)
            .await
            .unwrap();
        assert_eq!(
            store.get_role(repo_id, user_id).await.unwrap(),
            Some(CollaboratorRole::Maintainer)
        );

        store.remove(repo_id, user_id).await.unwrap();
        assert_eq!(store.get_role(repo_id, user_id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn set_role_on_a_missing_pair_is_not_found() {
        let store: Arc<dyn RepositoryCollaboratorStorePort> =
            Arc::new(FakeCollaborators(Mutex::new(vec![])));
        let result = store
            .set_role(Uuid::new_v4(), Uuid::new_v4(), CollaboratorRole::Maintainer)
            .await;
        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[test]
    fn collaborator_role_parse_and_as_str_round_trip() {
        assert_eq!(
            CollaboratorRole::parse("reader").unwrap(),
            CollaboratorRole::Reader
        );
        assert_eq!(
            CollaboratorRole::parse("contributor").unwrap(),
            CollaboratorRole::Contributor
        );
        assert_eq!(
            CollaboratorRole::parse("maintainer").unwrap(),
            CollaboratorRole::Maintainer
        );
        assert_eq!(CollaboratorRole::Reader.as_str(), "reader");
        assert_eq!(CollaboratorRole::Contributor.as_str(), "contributor");
        assert_eq!(CollaboratorRole::Maintainer.as_str(), "maintainer");
        assert!(matches!(
            CollaboratorRole::parse("bogus"),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn roles_are_ordered_reader_below_contributor_below_maintainer() {
        assert!(CollaboratorRole::Reader < CollaboratorRole::Contributor);
        assert!(CollaboratorRole::Contributor < CollaboratorRole::Maintainer);
        assert!(CollaboratorRole::Reader < CollaboratorRole::Maintainer);
    }
}
