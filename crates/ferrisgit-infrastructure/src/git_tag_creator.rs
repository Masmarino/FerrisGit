use std::path::{Path, PathBuf};

use crate::git_cli::{self, is_plausible_commit_sha};
use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::tag::TagCreatorPort;

pub struct GitTagCreator {
    storage_root: PathBuf,
}

impl GitTagCreator {
    pub fn new(storage_root: PathBuf) -> Self {
        Self { storage_root }
    }
}

async fn run_git(repo_path: &Path, args: &[&str]) -> Result<(bool, String, String), DomainError> {
    let output = git_cli::run(repo_path, args, &[], None).await?;
    Ok((output.success, output.stdout_trimmed(), output.stderr))
}

#[async_trait]
impl TagCreatorPort for GitTagCreator {
    async fn create_tag(
        &self,
        repository_disk_path: &str,
        tag_name: &str,
        target_commit_sha: &str,
    ) -> Result<(), DomainError> {
        if !is_plausible_commit_sha(target_commit_sha) {
            return Err(DomainError::Validation(format!(
                "'{target_commit_sha}' is not a plausible commit sha"
            )));
        }
        let repo_path = self.storage_root.join(repository_disk_path);
        let tag_ref = format!("refs/tags/{tag_name}");
        // An empty old-value makes update-ref fail atomically if the ref exists, so no racy existence check.
        let (ok, _, stderr) =
            run_git(&repo_path, &["update-ref", &tag_ref, target_commit_sha, ""]).await?;
        if !ok {
            return Err(DomainError::Conflict(format!(
                "a tag named '{tag_name}' already exists: {stderr}"
            )));
        }
        Ok(())
    }

    async fn delete_tag(
        &self,
        repository_disk_path: &str,
        tag_name: &str,
    ) -> Result<(), DomainError> {
        let repo_path = self.storage_root.join(repository_disk_path);
        let tag_ref = format!("refs/tags/{tag_name}");

        // update-ref -d exits 0 for a missing ref. Passing the current sha as old-value makes it fail,
        // which the NotFound below relies on.
        let (resolved, current_sha, _) =
            run_git(&repo_path, &["rev-parse", "--verify", "--quiet", &tag_ref]).await?;
        if !resolved {
            return Err(DomainError::NotFound(format!("tag '{tag_name}'")));
        }

        let (ok, _, stderr) =
            run_git(&repo_path, &["update-ref", "-d", &tag_ref, &current_sha]).await?;
        if !ok {
            return Err(DomainError::NotFound(format!("tag '{tag_name}': {stderr}")));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_git::{git, git_stdout};

    fn init_repo_with_a_commit(dir: &Path) -> String {
        git(dir, &["init", "-q"]);
        std::fs::write(dir.join("README.md"), "hello\n").unwrap();
        git(dir, &["add", "."]);
        git(dir, &["commit", "-q", "-m", "root"]);
        git_stdout(dir, &["rev-parse", "HEAD"])
    }

    #[tokio::test]
    async fn creating_a_tag_that_does_not_exist_yet_succeeds() {
        let tmp = tempfile::tempdir().unwrap();
        let sha = init_repo_with_a_commit(tmp.path());
        let creator = GitTagCreator::new(tmp.path().parent().unwrap().to_path_buf());
        let disk_path = tmp.path().file_name().unwrap().to_str().unwrap();

        creator.create_tag(disk_path, "v1.0.0", &sha).await.unwrap();

        assert_eq!(
            git_stdout(tmp.path(), &["rev-parse", "refs/tags/v1.0.0"]),
            sha
        );
    }

    #[tokio::test]
    async fn creating_a_tag_that_already_exists_is_a_conflict() {
        let tmp = tempfile::tempdir().unwrap();
        let sha = init_repo_with_a_commit(tmp.path());
        let creator = GitTagCreator::new(tmp.path().parent().unwrap().to_path_buf());
        let disk_path = tmp.path().file_name().unwrap().to_str().unwrap();
        creator.create_tag(disk_path, "v1.0.0", &sha).await.unwrap();

        let result = creator.create_tag(disk_path, "v1.0.0", &sha).await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
    }

    #[tokio::test]
    async fn deleting_an_existing_tag_removes_it() {
        let tmp = tempfile::tempdir().unwrap();
        let sha = init_repo_with_a_commit(tmp.path());
        let creator = GitTagCreator::new(tmp.path().parent().unwrap().to_path_buf());
        let disk_path = tmp.path().file_name().unwrap().to_str().unwrap();
        creator.create_tag(disk_path, "v1.0.0", &sha).await.unwrap();

        creator.delete_tag(disk_path, "v1.0.0").await.unwrap();

        let tags = git_stdout(tmp.path(), &["tag"]);
        assert!(tags.is_empty(), "the tag must be gone");
    }

    #[tokio::test]
    async fn deleting_a_tag_that_does_not_exist_is_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_a_commit(tmp.path());
        let creator = GitTagCreator::new(tmp.path().parent().unwrap().to_path_buf());
        let disk_path = tmp.path().file_name().unwrap().to_str().unwrap();

        let result = creator.delete_tag(disk_path, "does-not-exist").await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn deleting_one_tag_leaves_other_tags_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let sha = init_repo_with_a_commit(tmp.path());
        let creator = GitTagCreator::new(tmp.path().parent().unwrap().to_path_buf());
        let disk_path = tmp.path().file_name().unwrap().to_str().unwrap();
        creator.create_tag(disk_path, "v1.0.0", &sha).await.unwrap();
        creator.create_tag(disk_path, "v2.0.0", &sha).await.unwrap();

        creator.delete_tag(disk_path, "v1.0.0").await.unwrap();

        assert_eq!(git_stdout(tmp.path(), &["tag"]), "v2.0.0");
    }

    #[tokio::test]
    async fn a_target_commit_sha_that_is_not_plausible_hex_is_rejected_before_touching_git() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_a_commit(tmp.path());
        let creator = GitTagCreator::new(tmp.path().parent().unwrap().to_path_buf());
        let disk_path = tmp.path().file_name().unwrap().to_str().unwrap();

        let result = creator.create_tag(disk_path, "v1.0.0", "--not-a-sha").await;
        assert!(matches!(result, Err(DomainError::Validation(_))));

        let tags = git_stdout(tmp.path(), &["tag"]);
        assert!(tags.is_empty(), "no tag must have been created");
    }
}
