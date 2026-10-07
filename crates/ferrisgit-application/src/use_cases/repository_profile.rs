use std::collections::BTreeMap;
use std::sync::Arc;

use ferrisgit_domain::branch::BranchReaderPort;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::pipeline_file_reader::PipelineFileReaderPort;
use ferrisgit_domain::repository_profile::{
    MAX_DEPTH, MAX_FILES, MAX_MANIFEST_BYTES, RepositoryFileListerPort, RepositoryProfile,
    SKIPPED_DIRS, detect, files_to_read,
};

/// What the default branch of a repository is made of, for the pipeline editor to propose a pipeline that fits it.
pub struct DetectRepositoryProfileUseCase {
    branches: Arc<dyn BranchReaderPort>,
    lister: Arc<dyn RepositoryFileListerPort>,
    files: Arc<dyn PipelineFileReaderPort>,
}

impl DetectRepositoryProfileUseCase {
    pub fn new(
        branches: Arc<dyn BranchReaderPort>,
        lister: Arc<dyn RepositoryFileListerPort>,
        files: Arc<dyn PipelineFileReaderPort>,
    ) -> Self {
        Self {
            branches,
            lister,
            files,
        }
    }

    /// An empty profile for a repository with no commit: there is nothing to read yet.
    pub async fn execute(
        &self,
        repository_disk_path: &str,
    ) -> Result<RepositoryProfile, DomainError> {
        let default = self
            .branches
            .list_branches(repository_disk_path)
            .await?
            .into_iter()
            .find(|branch| branch.is_default);
        let Some(default) = default else {
            return Ok(detect(&[], &BTreeMap::new()));
        };
        let paths = self
            .lister
            .list_files_at_revision(
                repository_disk_path,
                &default.tip_sha,
                MAX_DEPTH,
                SKIPPED_DIRS,
                MAX_FILES,
            )
            .await?;
        let mut contents = BTreeMap::new();
        for path in files_to_read(&paths) {
            let bytes = self
                .files
                .read_file_at_revision(repository_disk_path, &default.tip_sha, &path)
                .await?;
            // A manifest too big or not text says nothing reliable: the project is judged on the others.
            if let Some(text) = bytes
                .filter(|bytes| bytes.len() <= MAX_MANIFEST_BYTES)
                .and_then(|bytes| String::from_utf8(bytes).ok())
            {
                contents.insert(path, text);
            }
        }
        Ok(detect(&paths, &contents))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeBranchReader;
    use async_trait::async_trait;
    use ferrisgit_domain::branch::BranchInfo;
    use ferrisgit_domain::repository_profile::ProjectKind;
    use std::sync::Mutex;

    struct Tree {
        files: BTreeMap<String, Vec<u8>>,
        asked: Mutex<Vec<(String, usize)>>,
    }

    impl Tree {
        fn new(files: &[(&str, &[u8])]) -> Arc<Self> {
            Arc::new(Self {
                files: files
                    .iter()
                    .map(|(path, bytes)| (path.to_string(), bytes.to_vec()))
                    .collect(),
                asked: Mutex::new(vec![]),
            })
        }
    }

    #[async_trait]
    impl RepositoryFileListerPort for Tree {
        async fn list_files_at_revision(
            &self,
            _disk: &str,
            revision: &str,
            max_depth: usize,
            _skip: &[&str],
            _limit: usize,
        ) -> Result<Vec<String>, DomainError> {
            self.asked
                .lock()
                .unwrap()
                .push((revision.to_string(), max_depth));
            Ok(self.files.keys().cloned().collect())
        }
    }

    #[async_trait]
    impl PipelineFileReaderPort for Tree {
        async fn read_file_at_revision(
            &self,
            _disk: &str,
            _revision: &str,
            path: &str,
        ) -> Result<Option<Vec<u8>>, DomainError> {
            Ok(self.files.get(path).cloned())
        }
    }

    fn use_case(tree: &Arc<Tree>, branches: Vec<BranchInfo>) -> DetectRepositoryProfileUseCase {
        DetectRepositoryProfileUseCase::new(
            Arc::new(FakeBranchReader::new(branches)),
            tree.clone(),
            tree.clone(),
        )
    }

    fn main_at(sha: &str) -> Vec<BranchInfo> {
        vec![
            BranchInfo {
                name: "feature".into(),
                tip_sha: "other".into(),
                is_default: false,
            },
            BranchInfo {
                name: "main".into(),
                tip_sha: sha.into(),
                is_default: true,
            },
        ]
    }

    #[tokio::test]
    async fn reads_the_default_branch_and_the_manifests_it_holds() {
        let tree = Tree::new(&[
            ("go.mod", b"module x\n\ngo 1.22.1\n"),
            ("main.go", b"package main"),
        ]);

        let profile = use_case(&tree, main_at("tip"))
            .execute("disk")
            .await
            .unwrap();

        assert_eq!(
            profile.projects[0].kind,
            ProjectKind::Go {
                go_version: Some("1.22".into())
            }
        );
        assert_eq!(
            *tree.asked.lock().unwrap(),
            [("tip".to_string(), MAX_DEPTH)]
        );
    }

    #[tokio::test]
    async fn judges_a_project_without_a_manifest_that_is_not_text() {
        let tree = Tree::new(&[("package.json", &[0xff, 0xfe, 0x00])]);

        let profile = use_case(&tree, main_at("tip"))
            .execute("disk")
            .await
            .unwrap();

        assert!(profile.projects.is_empty());
    }

    #[tokio::test]
    async fn has_nothing_to_say_about_a_repository_without_a_commit() {
        let tree = Tree::new(&[("Cargo.toml", b"[package]")]);

        let profile = use_case(&tree, vec![]).execute("disk").await.unwrap();

        assert!(profile.projects.is_empty());
        assert!(tree.asked.lock().unwrap().is_empty());
    }
}
