use std::sync::Arc;

use ferrisgit_domain::branch::BranchReaderPort;
use ferrisgit_domain::branch_file_writer::BranchFileWriterPort;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::merge_request::{MergeRequest, MergeRequestStorePort};
use ferrisgit_domain::pipeline_definition::{check_pipeline_definition, read_pipeline_definition};
use ferrisgit_domain::pipeline_file_reader::PipelineFileReaderPort;
use ferrisgit_domain::settings::RepositorySettingsStorePort;
use ferrisgit_domain::user::UserRepositoryPort;
use uuid::Uuid;

use crate::use_cases::create_merge_request::CreateMergeRequestUseCase;

/// The repository's pipeline file as it is on the default branch: what the editor starts from.
#[derive(Debug, Clone, PartialEq)]
pub struct RepositoryPipelineFile {
    /// The path the repository settings give, `.ferrisgit-ci.yml` by default.
    pub path: String,
    /// `None` for a repository with no commit yet.
    pub branch: Option<String>,
    /// The tip of the default branch the file was read at. A proposal is checked against it.
    pub base_sha: Option<String>,
    /// `None` when the branch has no such file.
    pub yaml: Option<String>,
}

pub struct ReadRepositoryPipelineFileUseCase {
    settings: Arc<dyn RepositorySettingsStorePort>,
    branches: Arc<dyn BranchReaderPort>,
    files: Arc<dyn PipelineFileReaderPort>,
}

impl ReadRepositoryPipelineFileUseCase {
    pub fn new(
        settings: Arc<dyn RepositorySettingsStorePort>,
        branches: Arc<dyn BranchReaderPort>,
        files: Arc<dyn PipelineFileReaderPort>,
    ) -> Self {
        Self {
            settings,
            branches,
            files,
        }
    }

    pub async fn execute(
        &self,
        repository_id: Uuid,
        repository_disk_path: &str,
    ) -> Result<RepositoryPipelineFile, DomainError> {
        let path = self
            .settings
            .get_or_create_default(repository_id)
            .await?
            .pipeline_file_path;
        let Some(default_branch) =
            default_branch(self.branches.as_ref(), repository_disk_path).await?
        else {
            return Ok(RepositoryPipelineFile {
                path,
                branch: None,
                base_sha: None,
                yaml: None,
            });
        };
        let bytes = self
            .files
            .read_file_at_revision(repository_disk_path, &default_branch.tip_sha, &path)
            .await?;
        let yaml = bytes
            .map(|bytes| {
                String::from_utf8(bytes)
                    .map_err(|_| DomainError::Validation(format!("{path} is not valid UTF-8 text")))
            })
            .transpose()?;
        Ok(RepositoryPipelineFile {
            path,
            branch: Some(default_branch.name),
            base_sha: Some(default_branch.tip_sha),
            yaml,
        })
    }
}

/// The repository's default branch, `None` for a repository with no commit yet.
pub(crate) async fn default_branch(
    branches: &dyn BranchReaderPort,
    repository_disk_path: &str,
) -> Result<Option<ferrisgit_domain::branch::BranchInfo>, DomainError> {
    Ok(branches
        .list_branches(repository_disk_path)
        .await?
        .into_iter()
        .find(|branch| branch.is_default))
}

#[derive(Debug, Clone)]
pub struct ProposedPipelineDefinition {
    pub branch: String,
    pub commit_sha: String,
    pub merge_request: MergeRequest,
}

/// Saves an edited pipeline file the way any change is made here: on a new branch, with a merge request into the
/// default branch, so that it is reviewed and merged like anything else. The default branch itself is never written to.
pub struct ProposePipelineDefinitionUseCase {
    settings: Arc<dyn RepositorySettingsStorePort>,
    users: Arc<dyn UserRepositoryPort>,
    branches: Arc<dyn BranchReaderPort>,
    files: Arc<dyn PipelineFileReaderPort>,
    writer: Arc<dyn BranchFileWriterPort>,
    merge_requests: Arc<dyn MergeRequestStorePort>,
}

pub struct PipelineProposal {
    pub repository_id: Uuid,
    pub repository_disk_path: String,
    pub author_id: Uuid,
    pub yaml: String,
    /// The `base_sha` the editor was opened at. The file must not have changed on the default branch since.
    pub base_sha: String,
    pub title: String,
    pub description: String,
}

impl ProposePipelineDefinitionUseCase {
    pub fn new(
        settings: Arc<dyn RepositorySettingsStorePort>,
        users: Arc<dyn UserRepositoryPort>,
        branches: Arc<dyn BranchReaderPort>,
        files: Arc<dyn PipelineFileReaderPort>,
        writer: Arc<dyn BranchFileWriterPort>,
        merge_requests: Arc<dyn MergeRequestStorePort>,
    ) -> Self {
        Self {
            settings,
            users,
            branches,
            files,
            writer,
            merge_requests,
        }
    }

    pub async fn execute(
        &self,
        proposal: PipelineProposal,
    ) -> Result<ProposedPipelineDefinition, DomainError> {
        // Refuse what the engine would refuse, so that a saved file always produces a pipeline.
        let definition = read_pipeline_definition(&proposal.yaml)
            .map_err(|e| DomainError::Validation(e.to_string()))?;
        if let Some(problem) = check_pipeline_definition(&definition).first() {
            return Err(DomainError::Validation(problem.to_string()));
        }

        let disk_path = proposal.repository_disk_path.as_str();
        let path = self
            .settings
            .get_or_create_default(proposal.repository_id)
            .await?
            .pipeline_file_path;
        let target = default_branch(self.branches.as_ref(), disk_path)
            .await?
            .ok_or_else(|| {
                DomainError::Validation(
                    "the repository has no commit yet: push a first one before editing its pipeline"
                        .to_string(),
                )
            })?;

        let current = self
            .files
            .read_file_at_revision(disk_path, &target.tip_sha, &path)
            .await?;
        if target.tip_sha != proposal.base_sha {
            let opened = self
                .files
                .read_file_at_revision(disk_path, &proposal.base_sha, &path)
                .await?;
            if opened != current {
                return Err(DomainError::Conflict(format!(
                    "{path} changed on {} since the editor was opened: reload it and redo the change",
                    target.name
                )));
            }
        }
        if current.as_deref() == Some(proposal.yaml.as_bytes()) {
            return Err(DomainError::Validation(
                "the file is the same as the one in the repository: there is nothing to propose"
                    .to_string(),
            ));
        }

        let author = self
            .users
            .find_by_id(proposal.author_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("user".to_string()))?;
        let title = match proposal.title.trim() {
            "" => "Update the pipeline",
            title => title,
        }
        .to_string();
        let branch = format!(
            "pipeline-editor/{}",
            &Uuid::new_v4().simple().to_string()[..8]
        );
        let commit_sha = self
            .writer
            .commit_file_to_new_branch(
                disk_path,
                &branch,
                &target.tip_sha,
                &path,
                &proposal.yaml,
                &title,
                &author.username,
                &author.email,
            )
            .await?;

        let opened =
            CreateMergeRequestUseCase::new(self.merge_requests.clone(), self.branches.clone())
                .execute(
                    proposal.repository_id,
                    disk_path,
                    proposal.author_id,
                    branch.clone(),
                    target.name,
                    title,
                    proposal.description,
                )
                .await;
        let merge_request = match opened {
            Ok(merge_request) => merge_request,
            Err(error) => {
                // Without its merge request the branch is useless, and nobody would find it: remove it.
                if let Err(cleanup) = self
                    .writer
                    .delete_new_branch(disk_path, &branch, &commit_sha)
                    .await
                {
                    tracing::warn!(%branch, error = %cleanup, "could not remove the branch of a pipeline proposal");
                }
                return Err(error);
            }
        };
        Ok(ProposedPipelineDefinition {
            branch,
            commit_sha,
            merge_request,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeBranchReader, FakeMergeRequests, FakeRepositorySettings, FakeUsers,
    };
    use crate::use_cases::fixtures::user;
    use async_trait::async_trait;
    use ferrisgit_domain::branch::BranchInfo;
    use ferrisgit_domain::settings::RepositorySettings;
    use std::collections::HashMap;
    use std::sync::Mutex;

    const OLD: &str = "stages: [build]\njobs:\n  a:\n    stage: build\n    image: rust:1\n    script: [cargo build]\n";
    const NEW: &str = "stages: [build]\njobs:\n  a:\n    stage: build\n    image: rust:1\n    script: [cargo build, cargo test]\n";

    /// Files by (revision, path).
    struct Files(HashMap<(String, String), Vec<u8>>);
    #[async_trait]
    impl PipelineFileReaderPort for Files {
        async fn read_file_at_revision(
            &self,
            _disk: &str,
            revision: &str,
            path: &str,
        ) -> Result<Option<Vec<u8>>, DomainError> {
            Ok(self
                .0
                .get(&(revision.to_string(), path.to_string()))
                .cloned())
        }
    }

    struct Commit {
        branch: String,
        base: String,
        path: String,
        content: String,
        committer: String,
    }

    #[derive(Default)]
    struct Writer {
        commits: Mutex<Vec<Commit>>,
        deleted: Mutex<Vec<(String, String)>>,
    }
    #[async_trait]
    impl BranchFileWriterPort for Writer {
        async fn commit_file_to_new_branch(
            &self,
            _disk: &str,
            new_branch: &str,
            base_sha: &str,
            file_path: &str,
            content: &str,
            _message: &str,
            committer_name: &str,
            _committer_email: &str,
        ) -> Result<String, DomainError> {
            self.commits.lock().unwrap().push(Commit {
                branch: new_branch.to_string(),
                base: base_sha.to_string(),
                path: file_path.to_string(),
                content: content.to_string(),
                committer: committer_name.to_string(),
            });
            Ok("newsha".to_string())
        }

        async fn delete_new_branch(
            &self,
            _disk: &str,
            branch: &str,
            commit_sha: &str,
        ) -> Result<(), DomainError> {
            self.deleted
                .lock()
                .unwrap()
                .push((branch.to_string(), commit_sha.to_string()));
            Ok(())
        }
    }

    struct Setup {
        use_case: ProposePipelineDefinitionUseCase,
        writer: Arc<Writer>,
        merge_requests: Arc<FakeMergeRequests>,
        author_id: Uuid,
    }

    fn branch(name: &str, tip: &str, is_default: bool) -> BranchInfo {
        BranchInfo {
            name: name.to_string(),
            tip_sha: tip.to_string(),
            is_default,
        }
    }

    fn setup(branches: Vec<BranchInfo>, files: &[(&str, &str, &str)]) -> Setup {
        setup_with(branches, files, true)
    }

    /// `report_new: false` hides the branch the writer makes, so opening its merge request fails.
    fn setup_with(
        branches: Vec<BranchInfo>,
        files: &[(&str, &str, &str)],
        report_new: bool,
    ) -> Setup {
        let author = user("alice");
        let author_id = author.id;
        let writer = Arc::new(Writer::default());
        let merge_requests = Arc::new(FakeMergeRequests::empty());
        // The writer's branch is listed once it exists, because opening the merge request checks for it.
        let branches = Arc::new(BranchesWithNew {
            inner: FakeBranchReader::new(branches),
            writer: writer.clone(),
            report_new,
        });
        let files = Files(
            files
                .iter()
                .map(|(rev, path, content)| {
                    (
                        (rev.to_string(), path.to_string()),
                        content.as_bytes().to_vec(),
                    )
                })
                .collect(),
        );
        let use_case = ProposePipelineDefinitionUseCase::new(
            Arc::new(FakeRepositorySettings::new(RepositorySettings {
                repository_id: Uuid::new_v4(),
                pipeline_file_path: ".ferrisgit-ci.yml".to_string(),
                ci_enabled: true,
                required_approvals: 0,
            })),
            Arc::new(FakeUsers::new(vec![author])),
            branches,
            Arc::new(files),
            writer.clone(),
            merge_requests.clone(),
        );
        Setup {
            use_case,
            writer,
            merge_requests,
            author_id,
        }
    }

    struct BranchesWithNew {
        inner: FakeBranchReader,
        writer: Arc<Writer>,
        report_new: bool,
    }
    #[async_trait]
    impl BranchReaderPort for BranchesWithNew {
        async fn list_branches(&self, disk: &str) -> Result<Vec<BranchInfo>, DomainError> {
            let mut all = self.inner.list_branches(disk).await?;
            if !self.report_new {
                return Ok(all);
            }
            for commit in self.writer.commits.lock().unwrap().iter() {
                all.push(branch(&commit.branch, "newsha", false));
            }
            Ok(all)
        }
    }

    fn proposal(setup: &Setup, yaml: &str, base_sha: &str) -> PipelineProposal {
        PipelineProposal {
            repository_id: Uuid::new_v4(),
            repository_disk_path: "hello.git".to_string(),
            author_id: setup.author_id,
            yaml: yaml.to_string(),
            base_sha: base_sha.to_string(),
            title: "Run the tests too".to_string(),
            description: String::new(),
        }
    }

    #[tokio::test]
    async fn commits_the_file_on_a_new_branch_and_opens_a_merge_request_into_the_default_branch() {
        let s = setup(
            vec![branch("main", "tip1", true)],
            &[("tip1", ".ferrisgit-ci.yml", OLD)],
        );

        let proposed = s.use_case.execute(proposal(&s, NEW, "tip1")).await.unwrap();

        let commits = s.writer.commits.lock().unwrap();
        assert_eq!(commits.len(), 1);
        let Commit {
            branch,
            base,
            path,
            content,
            committer,
        } = &commits[0];
        assert!(branch.starts_with("pipeline-editor/"), "{branch}");
        assert_eq!(
            (
                base.as_str(),
                path.as_str(),
                content.as_str(),
                committer.as_str()
            ),
            ("tip1", ".ferrisgit-ci.yml", NEW, "alice")
        );
        assert_eq!(&proposed.branch, branch);
        assert_eq!(proposed.commit_sha, "newsha");
        assert_eq!(proposed.merge_request.source_branch, *branch);
        assert_eq!(proposed.merge_request.target_branch, "main");
        assert_eq!(proposed.merge_request.title, "Run the tests too");
        assert_eq!(s.merge_requests.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn the_new_branch_is_removed_when_its_merge_request_cannot_be_opened() {
        let s = setup_with(
            vec![branch("main", "tip1", true)],
            &[("tip1", ".ferrisgit-ci.yml", OLD)],
            false,
        );

        let result = s.use_case.execute(proposal(&s, NEW, "tip1")).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
        let made = s.writer.commits.lock().unwrap()[0].branch.clone();
        assert_eq!(
            *s.writer.deleted.lock().unwrap(),
            vec![(made, "newsha".to_string())]
        );
        assert!(s.merge_requests.snapshot().is_empty());
    }

    #[tokio::test]
    async fn a_file_the_engine_would_refuse_is_refused_and_nothing_is_written() {
        let s = setup(vec![branch("main", "tip1", true)], &[]);
        let broken =
            "stages: [build]\njobs:\n  a:\n    stage: test\n    image: x\n    script: [y]\n";

        for yaml in [broken, "jobs: [", ""] {
            let result = s.use_case.execute(proposal(&s, yaml, "tip1")).await;
            assert!(
                matches!(result, Err(DomainError::Validation(_))),
                "{yaml:?}"
            );
        }
        assert!(s.writer.commits.lock().unwrap().is_empty());
        assert!(s.merge_requests.snapshot().is_empty());
    }

    #[tokio::test]
    async fn the_file_changing_on_the_default_branch_since_the_editor_opened_is_a_conflict() {
        let s = setup(
            vec![branch("main", "tip2", true)],
            &[
                ("tip1", ".ferrisgit-ci.yml", OLD),
                ("tip2", ".ferrisgit-ci.yml", "stages: [other]\njobs: {}\n"),
            ],
        );

        let result = s.use_case.execute(proposal(&s, NEW, "tip1")).await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
        assert!(s.writer.commits.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn other_files_moving_the_default_branch_do_not_get_in_the_way() {
        let s = setup(
            vec![branch("main", "tip2", true)],
            &[
                ("tip1", ".ferrisgit-ci.yml", OLD),
                ("tip2", ".ferrisgit-ci.yml", OLD),
            ],
        );

        s.use_case.execute(proposal(&s, NEW, "tip1")).await.unwrap();

        // The commit is built on the current tip, so the merge request holds the file change and nothing else.
        assert_eq!(s.writer.commits.lock().unwrap()[0].base, "tip2");
    }

    #[tokio::test]
    async fn a_new_file_is_proposed_when_the_branch_has_none() {
        let s = setup(vec![branch("main", "tip1", true)], &[]);

        s.use_case.execute(proposal(&s, NEW, "tip1")).await.unwrap();

        assert_eq!(s.writer.commits.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn proposing_the_file_as_it_is_is_refused() {
        let s = setup(
            vec![branch("main", "tip1", true)],
            &[("tip1", ".ferrisgit-ci.yml", OLD)],
        );

        let result = s.use_case.execute(proposal(&s, OLD, "tip1")).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
        assert!(s.writer.commits.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_repository_without_commits_cannot_be_proposed_to() {
        let s = setup(vec![], &[]);

        let result = s.use_case.execute(proposal(&s, NEW, "none")).await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn a_blank_title_falls_back_to_a_default_one() {
        let s = setup(vec![branch("main", "tip1", true)], &[]);
        let mut p = proposal(&s, NEW, "tip1");
        p.title = "   ".to_string();

        let proposed = s.use_case.execute(p).await.unwrap();

        assert_eq!(proposed.merge_request.title, "Update the pipeline");
    }

    #[tokio::test]
    async fn reading_gives_the_file_its_path_the_branch_and_the_tip() {
        let files = Files(
            [(
                ("tip1".to_string(), "ci/pipeline.yml".to_string()),
                OLD.as_bytes().to_vec(),
            )]
            .into(),
        );
        let reader = ReadRepositoryPipelineFileUseCase::new(
            Arc::new(FakeRepositorySettings::new(RepositorySettings {
                repository_id: Uuid::new_v4(),
                pipeline_file_path: "ci/pipeline.yml".to_string(),
                ci_enabled: true,
                required_approvals: 0,
            })),
            Arc::new(FakeBranchReader::new(vec![branch("trunk", "tip1", true)])),
            Arc::new(files),
        );

        let file = reader.execute(Uuid::new_v4(), "hello.git").await.unwrap();

        assert_eq!(
            file,
            RepositoryPipelineFile {
                path: "ci/pipeline.yml".to_string(),
                branch: Some("trunk".to_string()),
                base_sha: Some("tip1".to_string()),
                yaml: Some(OLD.to_string()),
            }
        );
    }

    #[tokio::test]
    async fn reading_an_empty_repository_gives_no_branch_and_no_file() {
        let reader = ReadRepositoryPipelineFileUseCase::new(
            Arc::new(FakeRepositorySettings::new(RepositorySettings {
                repository_id: Uuid::new_v4(),
                pipeline_file_path: ".ferrisgit-ci.yml".to_string(),
                ci_enabled: true,
                required_approvals: 0,
            })),
            Arc::new(FakeBranchReader::empty()),
            Arc::new(Files(HashMap::new())),
        );

        let file = reader.execute(Uuid::new_v4(), "hello.git").await.unwrap();

        assert_eq!((file.branch, file.base_sha, file.yaml), (None, None, None));
    }
}
