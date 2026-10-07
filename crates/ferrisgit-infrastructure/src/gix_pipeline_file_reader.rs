use std::path::PathBuf;

use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::pipeline_file_reader::PipelineFileReaderPort;
use ferrisgit_domain::repository_profile::RepositoryFileListerPort;

use crate::error::blocking;
use crate::gix_reader::GixRepositoryReader;

pub struct GixPipelineFileReader {
    storage_root: PathBuf,
}

impl GixPipelineFileReader {
    pub fn new(storage_root: PathBuf) -> Self {
        Self { storage_root }
    }
}

#[async_trait]
impl PipelineFileReaderPort for GixPipelineFileReader {
    async fn read_file_at_revision(
        &self,
        repository_disk_path: &str,
        revision: &str,
        path: &str,
    ) -> Result<Option<Vec<u8>>, DomainError> {
        let full_path = self.storage_root.join(repository_disk_path);
        let revision = revision.to_string();
        let path = path.to_string();
        // Git work stays off the async executor: it once deadlocked a #[sqlx::test] runtime.
        blocking(move || GixRepositoryReader.read_file_at_revision(&full_path, &revision, &path))
            .await
    }
}

#[async_trait]
impl RepositoryFileListerPort for GixPipelineFileReader {
    async fn list_files_at_revision(
        &self,
        repository_disk_path: &str,
        revision: &str,
        max_depth: usize,
        skip: &[&str],
        limit: usize,
    ) -> Result<Vec<String>, DomainError> {
        let full_path = self.storage_root.join(repository_disk_path);
        let revision = revision.to_string();
        let skip: Vec<String> = skip.iter().map(|name| name.to_string()).collect();
        blocking(move || {
            let skip: Vec<&str> = skip.iter().map(String::as_str).collect();
            GixRepositoryReader
                .list_files_at_revision(&full_path, &revision, max_depth, &skip, limit)
                .map(Option::unwrap_or_default)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_git::git;

    fn init_repo_with_pipeline_file(dir: &std::path::Path) {
        let run = |args: &[&str]| git(dir, args);
        run(&["init", "-q"]);
        std::fs::write(dir.join(".ferrisgit-ci.yml"), "stages: [build]").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "add pipeline"]);
    }

    #[tokio::test]
    async fn reads_the_pipeline_files_content_at_the_given_revision() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_pipeline_file(tmp.path());
        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();

        let adapter = GixPipelineFileReader::new(tmp.path().parent().unwrap().to_path_buf());
        let disk_path = tmp.path().file_name().unwrap().to_str().unwrap();
        let content = adapter
            .read_file_at_revision(disk_path, &sha, ".ferrisgit-ci.yml")
            .await
            .unwrap();

        assert_eq!(content, Some(b"stages: [build]".to_vec()));
    }

    #[tokio::test]
    async fn returns_none_when_the_pipeline_file_does_not_exist_at_that_revision() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_pipeline_file(tmp.path());
        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();

        let adapter = GixPipelineFileReader::new(tmp.path().parent().unwrap().to_path_buf());
        let disk_path = tmp.path().file_name().unwrap().to_str().unwrap();
        let content = adapter
            .read_file_at_revision(disk_path, &sha, "no-such-file.yml")
            .await
            .unwrap();

        assert_eq!(content, None);
    }

    #[tokio::test]
    async fn lists_files_and_folders_breadth_first_without_entering_skipped_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        for (path, text) in [
            ("Cargo.toml", "[workspace]"),
            ("crates/api/Cargo.toml", "[package]"),
            ("crates/api/src/deep/four/levels.rs", ""),
            ("frontend/package.json", "{}"),
            ("node_modules/left-pad/package.json", "{}"),
        ] {
            let file = dir.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
        git(dir, &["init", "-q"]);
        git(dir, &["add", "-f", "."]);
        git(dir, &["commit", "-q", "-m", "layout"]);
        let sha = GixRepositoryReader.list_commits(dir, 1).unwrap()[0]
            .sha
            .clone();
        let adapter = GixPipelineFileReader::new(dir.parent().unwrap().to_path_buf());
        let disk_path = dir.file_name().unwrap().to_str().unwrap();

        let paths = adapter
            .list_files_at_revision(disk_path, &sha, 3, &["node_modules"], 100)
            .await
            .unwrap();

        assert_eq!(
            paths,
            [
                "Cargo.toml",
                "crates/",
                "frontend/",
                "node_modules/",
                "crates/api/",
                "frontend/package.json",
                "crates/api/Cargo.toml",
                "crates/api/src/",
                "crates/api/src/deep/",
            ]
        );
        let cut = adapter
            .list_files_at_revision(disk_path, &sha, 3, &[], 2)
            .await
            .unwrap();
        assert_eq!(cut, ["Cargo.toml", "crates/"]);
    }
}
