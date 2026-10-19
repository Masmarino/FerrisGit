use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::repository::{
    Repository, RepositoryNamespace, RepositoryRelocation, RepositoryStorePort,
    repository_disk_path,
};

use super::name_rules::is_valid_path_name;

/// Renames a repository and/or transfers it to another namespace (the caller's own, or a group). Its old path stays
/// usable as a redirect, for Git and for the interface. The caller checks the caller's role on the repository and on the
/// destination before this runs.
pub struct RelocateRepositoryUseCase {
    repositories: Arc<dyn RepositoryStorePort>,
}

impl RelocateRepositoryUseCase {
    pub fn new(repositories: Arc<dyn RepositoryStorePort>) -> Self {
        Self { repositories }
    }

    /// `None` keeps the name, or the namespace. `move_disk(from, to)` moves the bare repository (and its wiki) on disk,
    /// both paths relative to the storage root; it runs before the database changes, and once more the other way round
    /// if they fail, so the two never disagree for long.
    pub async fn execute(
        &self,
        repo: Repository,
        new_name: Option<String>,
        new_namespace: Option<RepositoryNamespace>,
        mut move_disk: impl FnMut(&str, &str) -> std::io::Result<()>,
    ) -> Result<Repository, DomainError> {
        let name = new_name.unwrap_or_else(|| repo.name.clone());
        if !is_valid_path_name(&name) {
            return Err(DomainError::Validation(
                "repository name must be non-empty and alphanumeric/-/_ only".to_string(),
            ));
        }
        let namespace = new_namespace.unwrap_or_else(|| repo.namespace());
        if name == repo.name && namespace == repo.namespace() {
            return Ok(repo);
        }

        let taken = match namespace {
            RepositoryNamespace::User(owner_id) => {
                self.repositories
                    .find_by_owner_and_name(owner_id, &name)
                    .await?
            }
            RepositoryNamespace::Group(group_id) => {
                self.repositories
                    .find_by_group_and_name(group_id, &name)
                    .await?
            }
        };
        if taken.is_some_and(|other| other.id != repo.id) {
            return Err(DomainError::Conflict(format!(
                "a repository named '{name}' already exists there"
            )));
        }

        let (owner_id, group_id) = match namespace {
            RepositoryNamespace::User(owner_id) => (owner_id, None),
            // In a group the owner is only the creator, and the group's members get their roles from it.
            RepositoryNamespace::Group(group_id) => (repo.owner_id, Some(group_id)),
        };
        let disk_path = repository_disk_path(namespace, &name);
        move_disk(&repo.disk_path, &disk_path).map_err(|e| {
            DomainError::Infrastructure(format!("failed to move the repository: {e}"))
        })?;

        let relocated = self
            .repositories
            .relocate(
                repo.id,
                RepositoryRelocation {
                    name,
                    owner_id,
                    group_id,
                    disk_path: disk_path.clone(),
                },
            )
            .await;
        if relocated.is_err()
            && let Err(err) = move_disk(&disk_path, &repo.disk_path)
        {
            tracing::error!(error = %err, repository_id = %repo.id, from = disk_path, to = repo.disk_path, "failed to move a repository back after its relocation failed");
        }
        relocated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeRepositories;
    use crate::use_cases::fixtures::repository;
    use uuid::Uuid;

    struct World {
        use_case: RelocateRepositoryUseCase,
        repositories: Arc<FakeRepositories>,
        repo: Repository,
        moves: Vec<(String, String)>,
    }

    /// `hello`, owned by the first of `others`' owners if any, plus `others`.
    fn world(others: Vec<Repository>) -> World {
        let owner_id = others.first().map_or_else(Uuid::new_v4, |r| r.owner_id);
        let repo = Repository {
            disk_path: format!("{owner_id}/hello.git"),
            ..repository(owner_id)
        };
        let mut repos = vec![repo.clone()];
        repos.extend(others);
        let repositories = Arc::new(FakeRepositories::new(repos));
        World {
            use_case: RelocateRepositoryUseCase::new(repositories.clone()),
            repositories,
            repo,
            moves: vec![],
        }
    }

    impl World {
        async fn relocate(
            &mut self,
            name: Option<&str>,
            namespace: Option<RepositoryNamespace>,
        ) -> Result<Repository, DomainError> {
            let moves = &mut self.moves;
            self.use_case
                .execute(
                    self.repo.clone(),
                    name.map(str::to_string),
                    namespace,
                    |from, to| {
                        moves.push((from.to_string(), to.to_string()));
                        Ok(())
                    },
                )
                .await
        }
    }

    #[tokio::test]
    async fn a_rename_moves_the_disk_and_leaves_the_old_name_as_a_redirect() {
        let mut w = world(vec![]);
        let owner_id = w.repo.owner_id;

        let renamed = w.relocate(Some("world"), None).await.unwrap();

        assert_eq!(renamed.name, "world");
        assert_eq!(renamed.disk_path, format!("{owner_id}/world.git"));
        assert_eq!(
            w.moves,
            vec![(
                format!("{owner_id}/hello.git"),
                format!("{owner_id}/world.git")
            )]
        );
        let redirected = w
            .repositories
            .find_redirect_by_owner_and_name(owner_id, "hello")
            .await
            .unwrap();
        assert_eq!(redirected.map(|r| r.id), Some(w.repo.id));
    }

    #[tokio::test]
    async fn a_transfer_to_a_group_keeps_the_creator_as_owner() {
        let mut w = world(vec![]);
        let group_id = Uuid::new_v4();

        let moved = w
            .relocate(None, Some(RepositoryNamespace::Group(group_id)))
            .await
            .unwrap();

        assert_eq!(moved.group_id, Some(group_id));
        assert_eq!(moved.owner_id, w.repo.owner_id);
        assert_eq!(moved.disk_path, format!("groups/{group_id}/hello.git"));
    }

    #[tokio::test]
    async fn a_transfer_to_a_user_makes_them_the_owner() {
        let mut w = world(vec![]);
        let new_owner = Uuid::new_v4();

        let moved = w
            .relocate(Some("mine"), Some(RepositoryNamespace::User(new_owner)))
            .await
            .unwrap();

        assert_eq!((moved.owner_id, moved.group_id), (new_owner, None));
        assert_eq!(moved.name, "mine");
    }

    #[tokio::test]
    async fn nothing_moves_for_an_unchanged_path_a_bad_name_or_a_taken_one() {
        let taken = Repository {
            name: "taken".to_string(),
            ..repository(Uuid::new_v4())
        };
        let mut w = world(vec![taken]);

        let same = w.relocate(Some("hello"), None).await.unwrap();
        assert_eq!(same.id, w.repo.id);
        assert!(matches!(
            w.relocate(Some("no spaces"), None).await,
            Err(DomainError::Validation(_))
        ));
        assert!(matches!(
            w.relocate(Some("taken"), None).await,
            Err(DomainError::Conflict(_))
        ));
        assert!(w.moves.is_empty());
    }

    #[tokio::test]
    async fn a_failed_disk_move_leaves_the_repository_where_it_was() {
        let w = world(vec![]);

        let result = w
            .use_case
            .execute(w.repo.clone(), Some("world".to_string()), None, |_, _| {
                Err(std::io::Error::other("disk full"))
            })
            .await;

        assert!(matches!(result, Err(DomainError::Infrastructure(_))));
        assert_eq!(w.repositories.get(w.repo.id).unwrap().name, "hello");
    }
}
