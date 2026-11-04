use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::pipeline::Pipeline;
use ferrisgit_domain::release::{NewRelease, Release, ReleaseStorePort};
use ferrisgit_domain::repository::{Repository, RepositoryStorePort};
use ferrisgit_domain::tag::{TagCreatorPort, TagReaderPort};
use uuid::Uuid;

use crate::use_cases::create_pipeline::{CreatePipelineUseCase, PipelineTrigger};

pub struct CreateReleaseUseCase {
    releases: Arc<dyn ReleaseStorePort>,
    repositories: Arc<dyn RepositoryStorePort>,
    tags: Arc<dyn TagReaderPort>,
    tag_creator: Arc<dyn TagCreatorPort>,
    create_pipeline: CreatePipelineUseCase,
}

impl CreateReleaseUseCase {
    pub fn new(
        releases: Arc<dyn ReleaseStorePort>,
        repositories: Arc<dyn RepositoryStorePort>,
        tags: Arc<dyn TagReaderPort>,
        tag_creator: Arc<dyn TagCreatorPort>,
        create_pipeline: CreatePipelineUseCase,
    ) -> Self {
        Self {
            releases,
            repositories,
            tags,
            tag_creator,
            create_pipeline,
        }
    }

    /// An existing tag on `target_commit_sha` is reused as is. One pointing elsewhere is rejected, since a release
    /// follows whatever commit its tag targets and we never move tags. A missing tag is created first, and runs a
    /// pipeline as a pushed tag does.
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
        let tag_created = match existing_tags.iter().find(|t| t.name == tag_name) {
            Some(existing) if existing.target_sha == target_commit_sha => false,
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
                true
            }
        };

        let release = self
            .releases
            .create(NewRelease {
                repository_id,
                tag_name: tag_name.clone(),
                title,
                notes,
                draft,
                prerelease,
                author_id,
            })
            .await;

        // After the release, so that a job can attach its files to it. And even if the release failed: the tag exists now,
        // and a retry would reuse it and run nothing.
        if tag_created
            && let Err(err) = self
                .run_pipeline_of_new_tag(&repo, &tag_name, author_id)
                .await
        {
            tracing::error!(error = %err, repository_id = %repo.id, tag_name, "failed to create pipeline after creating a tag");
        }

        release
    }

    /// The commit comes from the tag, as for a push: the request may name it by a short sha, and a pipeline needs the full
    /// one of a commit.
    async fn run_pipeline_of_new_tag(
        &self,
        repo: &Repository,
        tag_name: &str,
        author_id: Uuid,
    ) -> Result<Option<Pipeline>, DomainError> {
        let tags = self.tags.list_tags(&repo.disk_path).await?;
        let Some(tag) = tags.iter().find(|t| t.name == tag_name) else {
            return Ok(None);
        };
        self.create_pipeline
            .execute(
                repo.id,
                &repo.disk_path,
                &tag.target_sha,
                author_id,
                PipelineTrigger::Tag {
                    name: tag_name.to_string(),
                },
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use ferrisgit_domain::pipeline_file_reader::PipelineFileReaderPort;
    use ferrisgit_domain::settings::RepositorySettings;
    use ferrisgit_domain::tag::TagInfo;

    use crate::job_execution_resolver::JobExecutionResolver;
    use crate::test_support::{
        FakeEvents, FakeExecution, FakeFileReader, FakeJobs, FakeNotifications, FakePipelines,
        FakeReleases, FakeRepositories, FakeRepositorySettings, FakeSystemSettings, FakeTagCreator,
        FakeUsers, FakeWebhooks,
    };
    use crate::use_cases::fixtures::{repository_with_id, system_settings};

    const TAG_JOBS_YAML: &[u8] = b"stages: [ship]\njobs:\n  publish:\n    stage: ship\n    image: alpine\n    script: [\"true\"]\n    rules:\n      tags: [\"v*\"]\n  on-main:\n    stage: ship\n    image: alpine\n    script: [\"true\"]\n    rules:\n      branches: [main]\n";

    /// The tags of the repository, the ones the use case created included. Those peel to a commit whose sha is not what the
    /// request said, as a short sha does.
    struct FakeTagReader {
        existing: Vec<TagInfo>,
        created: Arc<FakeTagCreator>,
    }
    #[async_trait]
    impl TagReaderPort for FakeTagReader {
        async fn list_tags(
            &self,
            _repository_disk_path: &str,
        ) -> Result<Vec<TagInfo>, DomainError> {
            let created = self
                .created
                .created()
                .into_iter()
                .map(|(_, name, sha)| TagInfo {
                    name,
                    target_sha: format!("{sha}-full"),
                });
            Ok(self.existing.iter().cloned().chain(created).collect())
        }
    }

    struct BrokenFileReader;
    #[async_trait]
    impl PipelineFileReaderPort for BrokenFileReader {
        async fn read_file_at_revision(
            &self,
            _repository_disk_path: &str,
            _revision: &str,
            _path: &str,
        ) -> Result<Option<Vec<u8>>, DomainError> {
            Err(DomainError::Infrastructure("the disk is gone".to_string()))
        }

        async fn changed_paths(
            &self,
            _repository_disk_path: &str,
            _base_revision: &str,
            _head_revision: &str,
        ) -> Result<Option<Vec<String>>, DomainError> {
            Ok(None)
        }
    }

    struct World {
        use_case: CreateReleaseUseCase,
        tag_creator: Arc<FakeTagCreator>,
        releases: Arc<FakeReleases>,
        pipelines: Arc<FakePipelines>,
        jobs: Arc<FakeJobs>,
    }

    fn world_reading(
        repo: Repository,
        existing_tags: Vec<TagInfo>,
        file_reader: Arc<dyn PipelineFileReaderPort>,
    ) -> World {
        let tag_creator = Arc::new(FakeTagCreator::default());
        let releases = Arc::new(FakeReleases::empty());
        let pipelines = Arc::new(FakePipelines::empty());
        let jobs = Arc::new(FakeJobs::empty());
        let repositories = Arc::new(FakeRepositories::new(vec![repo.clone()]));
        let execution = Arc::new(FakeExecution::new());
        let create_pipeline = CreatePipelineUseCase::new(
            Arc::new(FakeRepositorySettings::new(RepositorySettings {
                repository_id: repo.id,
                pipeline_file_path: ".ferrisgit-ci.yml".to_string(),
                ci_enabled: true,
                required_approvals: 0,
                pipeline_must_succeed: false,
                default_merge_method: Default::default(),
            })),
            Arc::new(FakeSystemSettings::new(system_settings())),
            pipelines.clone(),
            jobs.clone(),
            file_reader,
            Arc::new(FakeEvents::new()),
            Arc::new(JobExecutionResolver::new(execution.clone(), execution)),
            repositories.clone(),
            Arc::new(FakeUsers::new(vec![])),
            Arc::new(FakeNotifications::empty()),
            Arc::new(FakeWebhooks::new()),
        );
        let use_case = CreateReleaseUseCase::new(
            releases.clone(),
            repositories,
            Arc::new(FakeTagReader {
                existing: existing_tags,
                created: tag_creator.clone(),
            }),
            tag_creator.clone(),
            create_pipeline,
        );
        World {
            use_case,
            tag_creator,
            releases,
            pipelines,
            jobs,
        }
    }

    fn world(repo: Repository, existing_tags: Vec<TagInfo>) -> World {
        world_reading(
            repo,
            existing_tags,
            Arc::new(FakeFileReader::new(Some(TAG_JOBS_YAML.to_vec()))),
        )
    }

    /// A non-draft release of `v1.0.0` on `abc123`.
    async fn release(
        world: &World,
        repo_id: Uuid,
        author_id: Uuid,
    ) -> Result<Release, DomainError> {
        world
            .use_case
            .execute(
                repo_id,
                "v1.0.0".to_string(),
                "abc123".to_string(),
                "First release".to_string(),
                "notes".to_string(),
                false,
                false,
                author_id,
            )
            .await
    }

    #[tokio::test]
    async fn creating_a_release_for_a_brand_new_tag_creates_the_tag_then_the_release() {
        let repo_id = Uuid::new_v4();
        let world = world(repository_with_id(repo_id, Uuid::new_v4()), vec![]);

        release(&world, repo_id, Uuid::new_v4()).await.unwrap();

        let created_tags = world.tag_creator.created();
        assert_eq!(created_tags.len(), 1);
        assert_eq!(
            created_tags[0],
            (
                "hello.git".to_string(),
                "v1.0.0".to_string(),
                "abc123".to_string()
            )
        );
        assert_eq!(world.releases.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn a_new_tag_runs_the_jobs_of_a_tag_on_the_commit_it_points_at() {
        let repo_id = Uuid::new_v4();
        let author_id = Uuid::new_v4();
        let world = world(repository_with_id(repo_id, Uuid::new_v4()), vec![]);

        release(&world, repo_id, author_id).await.unwrap();

        let pipelines = world.pipelines.snapshot();
        assert_eq!(pipelines.len(), 1);
        assert_eq!(pipelines[0].commit_sha, "abc123-full");
        assert_eq!(pipelines[0].triggered_by, author_id);
        let jobs: Vec<String> = world.jobs.snapshot().into_iter().map(|j| j.name).collect();
        assert_eq!(jobs, vec!["publish"]);
    }

    #[tokio::test]
    async fn a_pipeline_that_cannot_start_leaves_the_release_created() {
        let repo_id = Uuid::new_v4();
        let world = world_reading(
            repository_with_id(repo_id, Uuid::new_v4()),
            vec![],
            Arc::new(BrokenFileReader),
        );

        release(&world, repo_id, Uuid::new_v4()).await.unwrap();

        assert_eq!(world.releases.snapshot().len(), 1);
        assert!(world.pipelines.snapshot().is_empty());
    }

    #[tokio::test]
    async fn creating_a_release_for_a_tag_that_already_points_at_the_requested_commit_does_not_recreate_it()
     {
        let repo_id = Uuid::new_v4();
        let world = world(
            repository_with_id(repo_id, Uuid::new_v4()),
            vec![TagInfo {
                name: "v1.0.0".to_string(),
                target_sha: "abc123".to_string(),
            }],
        );

        release(&world, repo_id, Uuid::new_v4()).await.unwrap();

        assert!(
            world.tag_creator.created().is_empty(),
            "an already-correct tag must never be recreated"
        );
        assert_eq!(world.releases.snapshot().len(), 1);
        assert!(
            world.pipelines.snapshot().is_empty(),
            "only a tag that is really created runs a pipeline"
        );
    }

    #[tokio::test]
    async fn creating_a_release_for_a_tag_that_points_elsewhere_is_rejected() {
        let repo_id = Uuid::new_v4();
        let world = world(
            repository_with_id(repo_id, Uuid::new_v4()),
            vec![TagInfo {
                name: "v1.0.0".to_string(),
                target_sha: "different-sha".to_string(),
            }],
        );

        let result = release(&world, repo_id, Uuid::new_v4()).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
        assert!(world.tag_creator.created().is_empty());
        assert!(
            world.releases.snapshot().is_empty(),
            "no release row must be created when the tag conflict is rejected"
        );
        assert!(world.pipelines.snapshot().is_empty());
    }

    #[tokio::test]
    async fn an_unknown_repository_is_not_found() {
        let world = world(repository_with_id(Uuid::new_v4(), Uuid::new_v4()), vec![]);

        let result = release(&world, Uuid::new_v4(), Uuid::new_v4()).await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }
}
