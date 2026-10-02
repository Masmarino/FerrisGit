use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::issue::{Issue, IssueStorePort};
use ferrisgit_domain::merge_request::{MergeRequest, MergeRequestStorePort};
use ferrisgit_domain::repository::{Repository, RepositoryStorePort};
use ferrisgit_domain::repository_collaborator::RepositoryCollaboratorStorePort;
use ferrisgit_domain::user::{User, UserRepositoryPort};
use uuid::Uuid;

use crate::access::visible_repository_ids;

const RESULTS_PER_TYPE: i64 = 8;

pub struct SearchResults {
    pub repositories: Vec<Repository>,
    pub issues: Vec<Issue>,
    pub merge_requests: Vec<MergeRequest>,
    pub users: Vec<User>,
}

impl SearchResults {
    fn empty() -> Self {
        Self {
            repositories: vec![],
            issues: vec![],
            merge_requests: vec![],
            users: vec![],
        }
    }
}

pub struct SearchUseCase {
    repositories: Arc<dyn RepositoryStorePort>,
    repository_collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
    groups: Arc<dyn GroupStorePort>,
    issues: Arc<dyn IssueStorePort>,
    merge_requests: Arc<dyn MergeRequestStorePort>,
    users: Arc<dyn UserRepositoryPort>,
}

impl SearchUseCase {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repositories: Arc<dyn RepositoryStorePort>,
        repository_collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
        groups: Arc<dyn GroupStorePort>,
        issues: Arc<dyn IssueStorePort>,
        merge_requests: Arc<dyn MergeRequestStorePort>,
        users: Arc<dyn UserRepositoryPort>,
    ) -> Self {
        Self {
            repositories,
            repository_collaborators,
            groups,
            issues,
            merge_requests,
            users,
        }
    }

    /// A blank query returns nothing without touching any store.
    pub async fn execute(&self, user_id: Uuid, query: &str) -> Result<SearchResults, DomainError> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(SearchResults::empty());
        }

        let visible_ids = visible_repository_ids(
            &self.repositories,
            &self.repository_collaborators,
            &self.groups,
            user_id,
        )
        .await?;

        // Users aren't scoped to visible repos, any signed-in user can find any user.
        if visible_ids.is_empty() {
            let users = self.users.search(trimmed, RESULTS_PER_TYPE).await?;
            return Ok(SearchResults {
                users,
                ..SearchResults::empty()
            });
        }

        let (repositories, issues, merge_requests, users) = tokio::try_join!(
            self.repositories
                .search(&visible_ids, trimmed, RESULTS_PER_TYPE),
            self.issues.search(&visible_ids, trimmed, RESULTS_PER_TYPE),
            self.merge_requests
                .search(&visible_ids, trimmed, RESULTS_PER_TYPE),
            self.users.search(trimmed, RESULTS_PER_TYPE),
        )?;

        Ok(SearchResults {
            repositories,
            issues,
            merge_requests,
            users,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeCollaborators, FakeGroups, FakeIssues, FakeMergeRequests, FakeRepositories, FakeUsers,
    };
    use crate::use_cases::fixtures;

    fn repo(id: Uuid, owner_id: Uuid, name: &str) -> Repository {
        Repository {
            name: name.to_string(),
            ..fixtures::repository_with_id(id, owner_id)
        }
    }

    fn some_user() -> User {
        fixtures::user("florian")
    }

    fn use_case(
        repos: Vec<Repository>,
        user_search_result: Vec<User>,
    ) -> (SearchUseCase, Arc<FakeRepositories>) {
        let repositories = Arc::new(FakeRepositories::new(repos));
        let use_case = SearchUseCase::new(
            repositories.clone(),
            Arc::new(FakeCollaborators::empty()),
            Arc::new(FakeGroups::empty()),
            Arc::new(FakeIssues::empty()),
            Arc::new(FakeMergeRequests::empty()),
            Arc::new(FakeUsers::new(user_search_result)),
        );
        (use_case, repositories)
    }

    #[tokio::test]
    async fn an_empty_query_returns_empty_results_without_calling_any_store() {
        let (use_case, _repositories) = use_case(
            vec![repo(Uuid::new_v4(), Uuid::new_v4(), "widget")],
            vec![some_user()],
        );

        let results = use_case.execute(Uuid::new_v4(), "   ").await.unwrap();

        assert!(results.repositories.is_empty());
        assert!(results.issues.is_empty());
        assert!(results.merge_requests.is_empty());
        assert!(results.users.is_empty());
    }

    #[tokio::test]
    async fn repository_search_is_scoped_to_the_visible_ids() {
        let user_id = Uuid::new_v4();
        let owned_id = Uuid::new_v4();
        let matching_repo = repo(owned_id, user_id, "widget parser");
        // Matches the query but belongs to someone else, so it mustn't show up for this user.
        let other_owners_repo = repo(Uuid::new_v4(), Uuid::new_v4(), "widget other");
        let (use_case, _repositories) =
            use_case(vec![matching_repo.clone(), other_owners_repo], vec![]);

        let results = use_case.execute(user_id, "widget").await.unwrap();

        assert_eq!(results.repositories.len(), 1);
        assert_eq!(results.repositories[0].id, owned_id);
    }

    #[tokio::test]
    async fn a_user_with_no_accessible_repositories_still_gets_user_results() {
        let user = some_user();
        let (use_case, _repositories) = use_case(vec![], vec![user.clone()]);

        let results = use_case.execute(Uuid::new_v4(), "florian").await.unwrap();

        assert!(results.repositories.is_empty());
        assert_eq!(results.users.len(), 1);
        assert_eq!(results.users[0].id, user.id);
    }
}
