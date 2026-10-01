use std::path::{Path, PathBuf};

use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::tag::TagCreatorPort;
use tokio::process::Command;

pub struct GitTagCreator {
    storage_root: PathBuf,
}

impl GitTagCreator {
    pub fn new(storage_root: PathBuf) -> Self {
        Self { storage_root }
    }
}

async fn run_git(repo_path: &Path, args: &[&str]) -> Result<(bool, String, String), DomainError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
    Ok((
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
        String::from_utf8_lossy(&output.stderr).trim().to_string(),
    ))
}

/// Accepts a plausible short or full git object id (7 to 40 lowercase hex characters), so nothing
/// else can become a positional `git update-ref` argument.
fn is_plausible_commit_sha(value: &str) -> bool {
    (7..=40).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
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
        // With an empty old-value, `update-ref` fails atomically if the ref already exists, so there's no
        // need for a racy existence check.
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

        // `update-ref -d` exits 0 even for a missing ref. Passing the current sha as old-value turns the
        // delete into a CAS that fails when the tag is already gone, which `NotFound` below relies on.
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

    fn init_repo_with_a_commit(dir: &Path) -> String {
        let run = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .status()
                .unwrap()
        };
        assert!(run(&["init", "-q"]).success());
        std::fs::write(dir.join("README.md"), "hello\n").unwrap();
        assert!(run(&["add", "."]).success());
        assert!(
            run(&[
                "-c",
                "user.email=t@t.com",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "-m",
                "root"
            ])
            .success()
        );
        let output = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    #[tokio::test]
    async fn creating_a_tag_that_does_not_exist_yet_succeeds() {
        let tmp = tempfile::tempdir().unwrap();
        let sha = init_repo_with_a_commit(tmp.path());
        let creator = GitTagCreator::new(tmp.path().parent().unwrap().to_path_buf());
        let disk_path = tmp.path().file_name().unwrap().to_str().unwrap();

        creator.create_tag(disk_path, "v1.0.0", &sha).await.unwrap();

        let output = std::process::Command::new("git")
            .args(["rev-parse", "refs/tags/v1.0.0"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), sha);
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

        let output = std::process::Command::new("git")
            .args(["tag"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        assert!(
            String::from_utf8_lossy(&output.stdout).trim().is_empty(),
            "the tag must be gone"
        );
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

        let output = std::process::Command::new("git")
            .args(["tag"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "v2.0.0");
    }

    #[tokio::test]
    async fn a_target_commit_sha_that_is_not_plausible_hex_is_rejected_before_touching_git() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_a_commit(tmp.path());
        let creator = GitTagCreator::new(tmp.path().parent().unwrap().to_path_buf());
        let disk_path = tmp.path().file_name().unwrap().to_str().unwrap();

        let result = creator.create_tag(disk_path, "v1.0.0", "--not-a-sha").await;
        assert!(matches!(result, Err(DomainError::Validation(_))));

        let output = std::process::Command::new("git")
            .args(["tag"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        assert!(
            String::from_utf8_lossy(&output.stdout).trim().is_empty(),
            "no tag must have been created"
        );
    }

    #[test]
    fn is_plausible_commit_sha_accepts_short_and_full_lowercase_hex_and_rejects_everything_else() {
        assert!(is_plausible_commit_sha("abc1234"));
        assert!(is_plausible_commit_sha(&"a".repeat(40)));
        assert!(!is_plausible_commit_sha("abc123"), "too short");
        assert!(!is_plausible_commit_sha(&"a".repeat(41)), "too long");
        assert!(
            !is_plausible_commit_sha("ABC1234"),
            "uppercase hex must be rejected"
        );
        assert!(
            !is_plausible_commit_sha("main"),
            "a branch name is not a sha"
        );
        assert!(
            !is_plausible_commit_sha("--upload-pack=x"),
            "must not look like a flag"
        );
    }
}
