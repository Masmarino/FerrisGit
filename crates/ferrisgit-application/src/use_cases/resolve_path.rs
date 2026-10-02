use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::{Group, GroupStorePort};
use ferrisgit_domain::repository::{Repository, RepositoryStorePort};
use ferrisgit_domain::user::UserRepositoryPort;
use uuid::Uuid;

pub enum ResolvedPath {
    PersonalRepository(Repository),
    Group {
        chain: Vec<Group>,
    },
    GroupRepository {
        chain: Vec<Group>,
        repository: Repository,
    },
}

pub struct ResolvePathUseCase {
    users: Arc<dyn UserRepositoryPort>,
    groups: Arc<dyn GroupStorePort>,
    repositories: Arc<dyn RepositoryStorePort>,
}

impl ResolvePathUseCase {
    pub fn new(
        users: Arc<dyn UserRepositoryPort>,
        groups: Arc<dyn GroupStorePort>,
        repositories: Arc<dyn RepositoryStorePort>,
    ) -> Self {
        Self {
            users,
            groups,
            repositories,
        }
    }

    pub async fn execute(&self, segments: &[String]) -> Result<ResolvedPath, DomainError> {
        let not_found = || DomainError::NotFound("path".to_string());
        let first = segments.first().ok_or_else(not_found)?;

        if let Some(user) = self.users.find_by_username(first).await? {
            let repo_name = segments.get(1).ok_or_else(not_found)?;
            if segments.len() != 2 {
                return Err(not_found());
            }
            let repo = self
                .repositories
                .find_by_owner_and_name(user.id, repo_name)
                .await?
                .ok_or_else(not_found)?;
            return Ok(ResolvedPath::PersonalRepository(repo));
        }

        let mut chain: Vec<Group> = Vec::new();
        let mut parent_id: Option<Uuid> = None;
        let mut remaining = segments;
        loop {
            let name = &remaining[0];
            match self.groups.find_child_by_name(parent_id, name).await? {
                Some(group) => {
                    parent_id = Some(group.id);
                    chain.push(group);
                    remaining = &remaining[1..];
                    if remaining.is_empty() {
                        return Ok(ResolvedPath::Group { chain });
                    }
                }
                None if remaining.len() == 1 => {
                    let group_id = chain.last().ok_or_else(not_found)?.id;
                    let repo = self
                        .repositories
                        .find_by_group_and_name(group_id, name)
                        .await?
                        .ok_or_else(not_found)?;
                    return Ok(ResolvedPath::GroupRepository {
                        chain,
                        repository: repo,
                    });
                }
                None => return Err(not_found()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeGroups, FakeRepositories, FakeUsers};
    use crate::use_cases::fixtures::{self, group, user};

    fn repo(owner_id: Uuid, group_id: Option<Uuid>, name: &str) -> Repository {
        Repository {
            group_id,
            name: name.to_string(),
            ..fixtures::repository(owner_id)
        }
    }

    #[tokio::test]
    async fn resolves_a_personal_repository() {
        let alice = user("alice");
        let mine = repo(alice.id, None, "mine");
        let use_case = ResolvePathUseCase::new(
            Arc::new(FakeUsers::new(vec![alice.clone()])),
            Arc::new(FakeGroups::new(vec![])),
            Arc::new(FakeRepositories::new(vec![mine.clone()])),
        );

        let result = use_case
            .execute(&["alice".to_string(), "mine".to_string()])
            .await
            .unwrap();

        assert!(matches!(result, ResolvedPath::PersonalRepository(r) if r.id == mine.id));
    }

    #[tokio::test]
    async fn resolves_a_root_group_with_no_repository_segment() {
        let acme = group(None, "acme");
        let use_case = ResolvePathUseCase::new(
            Arc::new(FakeUsers::new(vec![])),
            Arc::new(FakeGroups::new(vec![acme.clone()])),
            Arc::new(FakeRepositories::new(vec![])),
        );

        let result = use_case.execute(&["acme".to_string()]).await.unwrap();

        assert!(
            matches!(result, ResolvedPath::Group { chain } if chain.len() == 1 && chain[0].id == acme.id)
        );
    }

    #[tokio::test]
    async fn resolves_a_deeply_nested_group() {
        let acme = group(None, "acme");
        let backend = group(Some(acme.id), "backend");
        let infra = group(Some(backend.id), "infra");
        let use_case = ResolvePathUseCase::new(
            Arc::new(FakeUsers::new(vec![])),
            Arc::new(FakeGroups::new(vec![
                acme.clone(),
                backend.clone(),
                infra.clone(),
            ])),
            Arc::new(FakeRepositories::new(vec![])),
        );

        let result = use_case
            .execute(&[
                "acme".to_string(),
                "backend".to_string(),
                "infra".to_string(),
            ])
            .await
            .unwrap();

        let ResolvedPath::Group { chain } = result else {
            panic!("expected Group")
        };
        assert_eq!(
            chain.iter().map(|g| g.id).collect::<Vec<_>>(),
            vec![acme.id, backend.id, infra.id]
        );
    }

    #[tokio::test]
    async fn resolves_a_deeply_nested_group_repository() {
        let acme = group(None, "acme");
        let backend = group(Some(acme.id), "backend");
        let tf = repo(Uuid::new_v4(), Some(backend.id), "terraform-modules");
        let use_case = ResolvePathUseCase::new(
            Arc::new(FakeUsers::new(vec![])),
            Arc::new(FakeGroups::new(vec![acme.clone(), backend.clone()])),
            Arc::new(FakeRepositories::new(vec![tf.clone()])),
        );

        let result = use_case
            .execute(&[
                "acme".to_string(),
                "backend".to_string(),
                "terraform-modules".to_string(),
            ])
            .await
            .unwrap();

        let ResolvedPath::GroupRepository { chain, repository } = result else {
            panic!("expected GroupRepository")
        };
        assert_eq!(
            chain.iter().map(|g| g.id).collect::<Vec<_>>(),
            vec![acme.id, backend.id]
        );
        assert_eq!(repository.id, tf.id);
    }

    #[tokio::test]
    async fn a_path_that_matches_nothing_is_not_found() {
        let use_case = ResolvePathUseCase::new(
            Arc::new(FakeUsers::new(vec![])),
            Arc::new(FakeGroups::new(vec![])),
            Arc::new(FakeRepositories::new(vec![])),
        );

        let result = use_case
            .execute(&["nobody".to_string(), "nothing".to_string()])
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn a_username_with_the_wrong_number_of_segments_is_not_found() {
        let alice = user("alice");
        let use_case = ResolvePathUseCase::new(
            Arc::new(FakeUsers::new(vec![alice])),
            Arc::new(FakeGroups::new(vec![])),
            Arc::new(FakeRepositories::new(vec![])),
        );

        let result = use_case.execute(&["alice".to_string()]).await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }
}
