use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::pipeline::PipelineStorePort;
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::user::UserRepositoryPort;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AdminStats {
    pub total_users: i64,
    pub total_repositories: i64,
    pub pipelines_last_7_days: i64,
}

pub struct GetAdminStatsUseCase {
    repositories: Arc<dyn RepositoryStorePort>,
    users: Arc<dyn UserRepositoryPort>,
    pipelines: Arc<dyn PipelineStorePort>,
}

impl GetAdminStatsUseCase {
    pub fn new(
        repositories: Arc<dyn RepositoryStorePort>,
        users: Arc<dyn UserRepositoryPort>,
        pipelines: Arc<dyn PipelineStorePort>,
    ) -> Self {
        Self {
            repositories,
            users,
            pipelines,
        }
    }

    pub async fn execute(&self) -> Result<AdminStats, DomainError> {
        let repositories = self.repositories.list_all().await?;
        let total_users = self.users.count().await?;
        let since = chrono::Utc::now() - chrono::Duration::days(7);
        let pipelines_last_7_days = self.pipelines.count_created_since(since).await?;
        Ok(AdminStats {
            total_users,
            total_repositories: repositories.len() as i64,
            pipelines_last_7_days,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakePipelines, FakeRepositories, FakeUsers};
    use chrono::Utc;
    use ferrisgit_domain::pipeline::{Pipeline, PipelineStatus};
    use ferrisgit_domain::repository::{Repository, RepositoryVisibility};
    use ferrisgit_domain::settings::ExecutionEngine;
    use ferrisgit_domain::user::User;
    use uuid::Uuid;

    fn a_user() -> User {
        User {
            id: Uuid::new_v4(),
            username: "alice".to_string(),
            email: "a@example.com".to_string(),
            password_hash: "h".to_string(),
            is_admin: false,
            created_at: Utc::now(),
        }
    }

    fn a_repo(owner_id: Uuid) -> Repository {
        Repository {
            id: Uuid::new_v4(),
            owner_id,
            name: "hello".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "p".to_string(),
            visibility: RepositoryVisibility::Private,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn reports_total_users_and_total_repositories() {
        let owner = a_user();
        let repositories = Arc::new(FakeRepositories::new(vec![
            a_repo(owner.id),
            a_repo(owner.id),
        ]));
        let users = Arc::new(FakeUsers::new(vec![owner]));
        let pipelines = Arc::new(FakePipelines::empty());
        let use_case = GetAdminStatsUseCase::new(repositories, users, pipelines);

        let stats = use_case.execute().await.unwrap();

        assert_eq!(stats.total_users, 1);
        assert_eq!(stats.total_repositories, 2);
    }

    #[tokio::test]
    async fn counts_only_pipelines_created_in_the_last_7_days() {
        let owner = a_user();
        let repo = a_repo(owner.id);
        let repository_id = repo.id;
        let recent = Pipeline {
            id: Uuid::new_v4(),
            repository_id,
            commit_sha: "a".to_string(),
            execution_engine: ExecutionEngine::DockerRunners,
            status: PipelineStatus::Success,
            triggered_by: owner.id,
            created_at: Utc::now(),
            finished_at: None,
        };
        let old = Pipeline {
            id: Uuid::new_v4(),
            repository_id,
            commit_sha: "b".to_string(),
            execution_engine: ExecutionEngine::DockerRunners,
            status: PipelineStatus::Success,
            triggered_by: owner.id,
            created_at: Utc::now() - chrono::Duration::days(10),
            finished_at: None,
        };
        let repositories = Arc::new(FakeRepositories::new(vec![repo]));
        let users = Arc::new(FakeUsers::new(vec![owner]));
        let pipelines = Arc::new(FakePipelines::new(vec![recent, old]));
        let use_case = GetAdminStatsUseCase::new(repositories, users, pipelines);

        let stats = use_case.execute().await.unwrap();

        assert_eq!(stats.pipelines_last_7_days, 1);
    }
}
