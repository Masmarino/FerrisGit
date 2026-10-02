use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::release::{NewRelease, Release, ReleaseStorePort};
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::tag::{TagCreatorPort, TagReaderPort};
use uuid::Uuid;

pub struct CreateReleaseUseCase {
    releases: Arc<dyn ReleaseStorePort>,
    repositories: Arc<dyn RepositoryStorePort>,
    tags: Arc<dyn TagReaderPort>,
    tag_creator: Arc<dyn TagCreatorPort>,
}

impl CreateReleaseUseCase {
    pub fn new(
        releases: Arc<dyn ReleaseStorePort>,
        repositories: Arc<dyn RepositoryStorePort>,
        tags: Arc<dyn TagReaderPort>,
        tag_creator: Arc<dyn TagCreatorPort>,
    ) -> Self {
        Self {
            releases,
            repositories,
            tags,
            tag_creator,
        }
    }

    /// If `tag_name` already exists and points at `target_commit_sha`, it is reused as is (no write). If it exists but
    /// points somewhere else, the request is rejected: moving an existing tag is out of scope, and a release always
    /// follows whatever commit its tag already targets. If it doesn't exist yet, it is created first.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        repository_id: Uuid,
        tag_name: String,
        target_commit_sha: String,
        title: String,
        notes: String,
        draft: bool,
        prerelease: bool,
        author_id: Uuid,
    ) -> Result<Release, DomainError> {
        let repo = self
            .repositories
            .find_by_id(repository_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("repository".to_string()))?;

        let existing_tags = self.tags.list_tags(&repo.disk_path).await?;
        match existing_tags.iter().find(|t| t.name == tag_name) {
            Some(existing) if existing.target_sha == target_commit_sha => {}
            Some(existing) => {
                return Err(DomainError::Validation(format!(
                    "tag '{tag_name}' already exists and points at a different commit ({}) than requested ({target_commit_sha})",
                    existing.target_sha
                )));
            }
            None => {
                self.tag_creator
                    .create_tag(&repo.disk_path, &tag_name, &target_commit_sha)
                    .await?;
            }
        }

        self.releases
            .create(NewRelease {
                repository_id,
                tag_name,
                title,
                notes,
                draft,
                prerelease,
                author_id,
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use ferrisgit_domain::repository::Repository;
    use ferrisgit_domain::tag::TagInfo;

    use crate::test_support::{FakeReleases, FakeRepositories, FakeTagCreator};
    use crate::use_cases::fixtures::repository_with_id;

    struct FakeTagReader(Vec<TagInfo>);
    #[async_trait]
    impl TagReaderPort for FakeTagReader {
        async fn list_tags(
            &self,
            _repository_disk_path: &str,
        ) -> Result<Vec<TagInfo>, DomainError> {
            Ok(self.0.clone())
        }
    }

    fn use_case(
        repo: Repository,
        existing_tags: Vec<TagInfo>,
    ) -> (CreateReleaseUseCase, Arc<FakeTagCreator>, Arc<FakeReleases>) {
        let tag_creator = Arc::new(FakeTagCreator::default());
        let releases = Arc::new(FakeReleases::empty());
        let use_case = CreateReleaseUseCase::new(
            releases.clone(),
            Arc::new(FakeRepositories::new(vec![repo])),
            Arc::new(FakeTagReader(existing_tags)),
            tag_creator.clone(),
        );
        (use_case, tag_creator, releases)
    }

    #[tokio::test]
    async fn creating_a_release_for_a_brand_new_tag_creates_the_tag_then_the_release() {
        let repo_id = Uuid::new_v4();
        let (use_case, tag_creator, releases) =
            use_case(repository_with_id(repo_id, Uuid::new_v4()), vec![]);

        use_case
            .execute(
                repo_id,
                "v1.0.0".to_string(),
                "abc123".to_string(),
                "First release".to_string(),
                "notes".to_string(),
                false,
                false,
                Uuid::new_v4(),
            )
            .await
            .unwrap();

        let created_tags = tag_creator.created();
        assert_eq!(created_tags.len(), 1);
        assert_eq!(
            created_tags[0],
            (
                "hello.git".to_string(),
                "v1.0.0".to_string(),
                "abc123".to_string()
            )
        );
        assert_eq!(releases.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn creating_a_release_for_a_tag_that_already_points_at_the_requested_commit_does_not_recreate_it()
     {
        let repo_id = Uuid::new_v4();
        let (use_case, tag_creator, releases) = use_case(
            repository_with_id(repo_id, Uuid::new_v4()),
            vec![TagInfo {
                name: "v1.0.0".to_string(),
                target_sha: "abc123".to_string(),
            }],
        );

        use_case
            .execute(
                repo_id,
                "v1.0.0".to_string(),
                "abc123".to_string(),
                "First release".to_string(),
                "notes".to_string(),
                false,
                false,
                Uuid::new_v4(),
            )
            .await
            .unwrap();

        assert!(
            tag_creator.created().is_empty(),
            "an already-correct tag must never be recreated"
        );
        assert_eq!(releases.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn creating_a_release_for_a_tag_that_points_elsewhere_is_rejected() {
        let repo_id = Uuid::new_v4();
        let (use_case, tag_creator, releases) = use_case(
            repository_with_id(repo_id, Uuid::new_v4()),
            vec![TagInfo {
                name: "v1.0.0".to_string(),
                target_sha: "different-sha".to_string(),
            }],
        );

        let result = use_case
            .execute(
                repo_id,
                "v1.0.0".to_string(),
                "abc123".to_string(),
                "First release".to_string(),
                "notes".to_string(),
                false,
                false,
                Uuid::new_v4(),
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
        assert!(tag_creator.created().is_empty());
        assert!(
            releases.snapshot().is_empty(),
            "no release row must be created when the tag conflict is rejected"
        );
    }

    #[tokio::test]
    async fn an_unknown_repository_is_not_found() {
        let (use_case, _tag_creator, _releases) =
            use_case(repository_with_id(Uuid::new_v4(), Uuid::new_v4()), vec![]);

        let result = use_case
            .execute(
                Uuid::new_v4(),
                "v1.0.0".to_string(),
                "abc123".to_string(),
                "t".to_string(),
                "n".to_string(),
                false,
                false,
                Uuid::new_v4(),
            )
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }
}
