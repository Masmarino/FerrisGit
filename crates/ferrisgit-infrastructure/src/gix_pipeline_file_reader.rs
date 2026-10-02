use std::path::PathBuf;

use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::pipeline_file_reader::PipelineFileReaderPort;

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
        // Keep blocking work off the async executor: git subprocess calls once deadlocked a `#[sqlx::test]` executor.
        blocking(move || GixRepositoryReader.read_file_at_revision(&full_path, &revision, &path))
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
}
