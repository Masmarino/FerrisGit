use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::release::ReleaseStorePort;
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::tag::TagCreatorPort;
use uuid::Uuid;

pub struct DeleteTagUseCase {
    repositories: Arc<dyn RepositoryStorePort>,
    releases: Arc<dyn ReleaseStorePort>,
    tag_creator: Arc<dyn TagCreatorPort>,
}

impl DeleteTagUseCase {
    pub fn new(
        repositories: Arc<dyn RepositoryStorePort>,
        releases: Arc<dyn ReleaseStorePort>,
        tag_creator: Arc<dyn TagCreatorPort>,
    ) -> Self {
        Self {
            repositories,
            releases,
            tag_creator,
        }
    }

    /// Refuses to delete a tag a release still references: a Maintainer should not silently sever an active release's
    /// link to its commit. `(repository_id, tag_name)` is unique per release, so a hit means the tag is in use.
    pub async fn execute(&self, repository_id: Uuid, tag_name: String) -> Result<(), DomainError> {
        let repo = self
            .repositories
            .find_by_id(repository_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("repository".to_string()))?;

        if self
            .releases
            .find_by_tag_name(repository_id, &tag_name)
            .await?
            .is_some()
        {
            return Err(DomainError::Conflict(format!(
                "tag '{tag_name}' is still used by a release"
            )));
        }

        self.tag_creator
            .delete_tag(&repo.disk_path, &tag_name)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeReleases, FakeRepositories, FakeTagCreator};
    use crate::use_cases::fixtures::{self, release};
    use ferrisgit_domain::repository::Repository;

    fn repository() -> Repository {
        Repository {
            disk_path: "alice/hello.git".to_string(),
            ..fixtures::repository(Uuid::new_v4())
        }
    }

    #[tokio::test]
    async fn deletes_a_tag_that_no_release_references() {
        let repo = repository();
        let tag_creator = Arc::new(FakeTagCreator::default());
        let use_case = DeleteTagUseCase::new(
            Arc::new(FakeRepositories::new(vec![repo.clone()])),
            Arc::new(FakeReleases::empty()),
            tag_creator.clone(),
        );

        use_case
            .execute(repo.id, "v1.0.0".to_string())
            .await
            .unwrap();

        assert_eq!(
            tag_creator.deleted(),
            vec![("alice/hello.git".to_string(), "v1.0.0".to_string())]
        );
    }

    #[tokio::test]
    async fn refuses_to_delete_a_tag_still_referenced_by_a_release() {
        let repo = repository();
        let release = release(Uuid::new_v4(), repo.id);
        let tag_creator = Arc::new(FakeTagCreator::default());
        let use_case = DeleteTagUseCase::new(
            Arc::new(FakeRepositories::new(vec![repo.clone()])),
            Arc::new(FakeReleases::new(vec![release])),
            tag_creator.clone(),
        );

        let result = use_case.execute(repo.id, "v1.0.0".to_string()).await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
        assert!(
            tag_creator.deleted().is_empty(),
            "the git delete must never run when a release still references the tag"
        );
    }

    #[tokio::test]
    async fn an_unknown_repository_is_not_found() {
        let repo = repository();
        let use_case = DeleteTagUseCase::new(
            Arc::new(FakeRepositories::new(vec![repo])),
            Arc::new(FakeReleases::empty()),
            Arc::new(FakeTagCreator::default()),
        );

        let result = use_case.execute(Uuid::new_v4(), "v1.0.0".to_string()).await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }
}
