use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::use_cases::pipeline_definition_proposal::default_branch;
use ferrisgit_domain::branch::BranchReaderPort;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::repository_profile::{
    MAX_DEPTH, MAX_FILES, MAX_MANIFEST_BYTES, RepositoryFilesPort, RepositoryProfile, SKIPPED_DIRS,
    detect, files_to_read,
};

/// How many profiles the server keeps: a few hundred repositories' worth, a few kilobytes each.
pub const PROFILE_CACHE_CAPACITY: usize = 256;

/// Profiles already read, by repository and commit. A commit never changes, so a kept profile never goes stale, and
/// opening the editor again skips a tree walk and up to `MAX_MANIFESTS` reads. Past its capacity, the oldest profile is
/// dropped.
pub struct RepositoryProfileCache {
    capacity: usize,
    entries: Mutex<VecDeque<(ProfileKey, RepositoryProfile)>>,
}

/// A repository's disk path and a commit of it.
type ProfileKey = (String, String);

impl RepositoryProfileCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: Mutex::new(VecDeque::new()),
        }
    }

    fn get(&self, key: &ProfileKey) -> Option<RepositoryProfile> {
        let entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        entries
            .iter()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, profile)| profile.clone())
    }

    fn put(&self, key: ProfileKey, profile: RepositoryProfile) {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.capacity == 0 || entries.iter().any(|(candidate, _)| *candidate == key) {
            return;
        }
        if entries.len() == self.capacity {
            entries.pop_front();
        }
        entries.push_back((key, profile));
    }
}

/// What the default branch of a repository is made of, for the pipeline editor to propose a pipeline that fits it.
pub struct DetectRepositoryProfileUseCase {
    branches: Arc<dyn BranchReaderPort>,
    files: Arc<dyn RepositoryFilesPort>,
    cache: Arc<RepositoryProfileCache>,
}

impl DetectRepositoryProfileUseCase {
    pub fn new(
        branches: Arc<dyn BranchReaderPort>,
        files: Arc<dyn RepositoryFilesPort>,
        cache: Arc<RepositoryProfileCache>,
    ) -> Self {
        Self {
            branches,
            files,
            cache,
        }
    }

    /// A repository without any commit gets an empty profile: there is nothing to read yet.
    pub async fn execute(
        &self,
        repository_disk_path: &str,
    ) -> Result<RepositoryProfile, DomainError> {
        let Some(default) = default_branch(self.branches.as_ref(), repository_disk_path).await?
        else {
            return Ok(detect(&[], &BTreeMap::new()));
        };
        let key = (repository_disk_path.to_string(), default.tip_sha.clone());
        if let Some(profile) = self.cache.get(&key) {
            return Ok(profile);
        }
        let paths = self
            .files
            .list_files_at_revision(
                repository_disk_path,
                &default.tip_sha,
                MAX_DEPTH,
                SKIPPED_DIRS,
                MAX_FILES,
            )
            .await?;
        // A manifest that is too big or not text is left out, and the project is judged on the others.
        let contents = self
            .files
            .read_text_files_at_revision(
                repository_disk_path,
                &default.tip_sha,
                &files_to_read(&paths),
                MAX_MANIFEST_BYTES,
            )
            .await?;
        let profile = detect(&paths, &contents);
        self.cache.put(key, profile.clone());
        Ok(profile)
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
    impl RepositoryFilesPort for Tree {
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

        async fn read_text_files_at_revision(
            &self,
            _disk: &str,
            _revision: &str,
            paths: &[String],
            max_bytes: usize,
        ) -> Result<BTreeMap<String, String>, DomainError> {
            Ok(paths
                .iter()
                .filter_map(|path| self.files.get(path).map(|bytes| (path, bytes)))
                .filter(|(_, bytes)| bytes.len() <= max_bytes)
                .filter_map(|(path, bytes)| {
                    String::from_utf8(bytes.clone())
                        .ok()
                        .map(|text| (path.clone(), text))
                })
                .collect())
        }
    }

    fn use_case(tree: &Arc<Tree>, branches: Vec<BranchInfo>) -> DetectRepositoryProfileUseCase {
        with_cache(tree, branches, &Arc::new(RepositoryProfileCache::new(4)))
    }

    fn with_cache(
        tree: &Arc<Tree>,
        branches: Vec<BranchInfo>,
        cache: &Arc<RepositoryProfileCache>,
    ) -> DetectRepositoryProfileUseCase {
        DetectRepositoryProfileUseCase::new(
            Arc::new(FakeBranchReader::new(branches)),
            tree.clone(),
            cache.clone(),
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
    async fn reads_a_commit_once_and_a_new_one_again() {
        let tree = Tree::new(&[("go.mod", b"module x\n\ngo 1.22\n")]);
        let cache = Arc::new(RepositoryProfileCache::new(4));

        let first = with_cache(&tree, main_at("tip1"), &cache)
            .execute("disk")
            .await
            .unwrap();
        let again = with_cache(&tree, main_at("tip1"), &cache)
            .execute("disk")
            .await
            .unwrap();
        with_cache(&tree, main_at("tip2"), &cache)
            .execute("disk")
            .await
            .unwrap();
        with_cache(&tree, main_at("tip1"), &cache)
            .execute("other-disk")
            .await
            .unwrap();

        assert_eq!(first, again);
        let revisions: Vec<String> = tree
            .asked
            .lock()
            .unwrap()
            .iter()
            .map(|(rev, _)| rev.clone())
            .collect();
        assert_eq!(revisions, ["tip1", "tip2", "tip1"]);
    }

    #[tokio::test]
    async fn keeps_only_the_latest_profiles_past_its_capacity() {
        let tree = Tree::new(&[("go.mod", b"module x\n")]);
        let cache = Arc::new(RepositoryProfileCache::new(2));
        for tip in ["a", "b", "c", "a"] {
            with_cache(&tree, main_at(tip), &cache)
                .execute("disk")
                .await
                .unwrap();
        }

        let revisions: Vec<String> = tree
            .asked
            .lock()
            .unwrap()
            .iter()
            .map(|(rev, _)| rev.clone())
            .collect();
        assert_eq!(revisions, ["a", "b", "c", "a"], "a was dropped for c");
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
