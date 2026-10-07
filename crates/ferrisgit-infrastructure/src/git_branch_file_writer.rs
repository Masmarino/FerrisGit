use std::path::{Path, PathBuf};

use crate::git_cli::{self, identity_env};
use async_trait::async_trait;
use ferrisgit_domain::branch_file_writer::BranchFileWriterPort;
use ferrisgit_domain::error::DomainError;
use uuid::Uuid;

pub struct GitBranchFileWriter {
    storage_root: PathBuf,
}

impl GitBranchFileWriter {
    pub fn new(storage_root: PathBuf) -> Self {
        Self { storage_root }
    }
}

/// Runs git and returns its trimmed stdout, turning a failing exit status into an infrastructure error naming `what`.
async fn git_ok(
    repo_path: &Path,
    args: &[&str],
    env: &[(&str, &str)],
    stdin_data: Option<&str>,
    what: &str,
) -> Result<String, DomainError> {
    let output = git_cli::run(repo_path, args, env, stdin_data.map(str::as_bytes)).await?;
    if !output.success {
        return Err(DomainError::Infrastructure(format!(
            "git {what} failed: {}",
            output.stderr
        )));
    }
    Ok(output.stdout_trimmed())
}

#[async_trait]
impl BranchFileWriterPort for GitBranchFileWriter {
    async fn commit_file_to_new_branch(
        &self,
        repository_disk_path: &str,
        new_branch: &str,
        base_sha: &str,
        file_path: &str,
        content: &str,
        message: &str,
        committer_name: &str,
        committer_email: &str,
    ) -> Result<String, DomainError> {
        let repo_path = self.storage_root.join(repository_disk_path);

        let valid_name = git_cli::run(
            &repo_path,
            &["check-ref-format", "--branch", new_branch],
            &[],
            None,
        )
        .await?;
        if !valid_name.success || new_branch.starts_with('-') {
            return Err(DomainError::Validation(format!(
                "not a valid branch name: {new_branch}"
            )));
        }

        let blob_oid = git_ok(
            &repo_path,
            &["hash-object", "-w", "--stdin"],
            &[],
            Some(content),
            "hash-object",
        )
        .await?;

        // The tree is built in a scratch index, so the repository's own index (a bare repository has none) is untouched.
        let index_path =
            std::env::temp_dir().join(format!("ferrisgit-branch-file-index-{}", Uuid::new_v4()));
        let index_path_str = index_path.to_string_lossy().into_owned();
        let index_env: [(&str, &str); 1] = [("GIT_INDEX_FILE", index_path_str.as_str())];
        let tree = async {
            git_ok(
                &repo_path,
                &["read-tree", base_sha],
                &index_env,
                None,
                "read-tree",
            )
            .await?;
            let cacheinfo = format!("100644,{blob_oid},{file_path}");
            git_ok(
                &repo_path,
                &["update-index", "--add", "--cacheinfo", &cacheinfo],
                &index_env,
                None,
                "update-index",
            )
            .await?;
            git_ok(&repo_path, &["write-tree"], &index_env, None, "write-tree").await
        }
        .await;
        std::fs::remove_file(&index_path).ok();
        let tree_oid = tree?;

        let commit_env = identity_env(committer_name, committer_email);
        let commit_sha = git_ok(
            &repo_path,
            &["commit-tree", &tree_oid, "-p", base_sha, "-m", message],
            &commit_env,
            None,
            "commit-tree",
        )
        .await?;

        // An empty old value means "the ref must not exist yet".
        let branch_ref = format!("refs/heads/{new_branch}");
        let updated = git_cli::run(
            &repo_path,
            &["update-ref", &branch_ref, &commit_sha, ""],
            &[],
            None,
        )
        .await?;
        if !updated.success {
            return Err(DomainError::Conflict(format!(
                "the branch {new_branch} already exists: {}",
                updated.stderr
            )));
        }
        Ok(commit_sha)
    }

    async fn delete_new_branch(
        &self,
        repository_disk_path: &str,
        branch: &str,
        commit_sha: &str,
    ) -> Result<(), DomainError> {
        let repo_path = self.storage_root.join(repository_disk_path);
        // With an old value, git deletes the ref only if it still holds it.
        let branch_ref = format!("refs/heads/{branch}");
        let deleted = git_cli::run(
            &repo_path,
            &["update-ref", "-d", &branch_ref, commit_sha],
            &[],
            None,
        )
        .await?;
        if !deleted.success {
            return Err(DomainError::Conflict(format!(
                "the branch {branch} is no longer at {commit_sha}: {}",
                deleted.stderr
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_git::{git, git_stdout_untrimmed, init_bare_with_clone, rev_parse};

    fn repo_with_a_commit(dir: &Path) -> (PathBuf, String) {
        let (bare, work) = init_bare_with_clone(dir);
        std::fs::write(work.join("README.md"), "hello\n").unwrap();
        git(&work, &["add", "."]);
        git(&work, &["commit", "-q", "-m", "root"]);
        git(&work, &["push", "-q", "origin", "main"]);
        let tip = rev_parse(&bare, "refs/heads/main");
        (bare, tip)
    }

    #[tokio::test]
    async fn commits_the_file_on_a_new_branch_and_leaves_the_base_branch_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let (bare, tip) = repo_with_a_commit(tmp.path());
        let writer = GitBranchFileWriter::new(tmp.path().to_path_buf());

        let sha = writer
            .commit_file_to_new_branch(
                "bare.git",
                "pipeline-editor/abc",
                &tip,
                ".ferrisgit-ci.yml",
                "stages: [build]\njobs: {}\n",
                "Edit the pipeline",
                "florian",
                "florian@example.com",
            )
            .await
            .unwrap();

        assert_eq!(rev_parse(&bare, "refs/heads/pipeline-editor/abc"), sha);
        assert_eq!(rev_parse(&bare, "refs/heads/main"), tip);
        assert_eq!(
            git_stdout_untrimmed(&bare, &["show", &format!("{sha}:.ferrisgit-ci.yml")]),
            "stages: [build]\njobs: {}\n"
        );
        assert_eq!(
            git_stdout_untrimmed(&bare, &["show", &format!("{sha}:README.md")]),
            "hello\n",
            "the rest of the tree is kept"
        );
        assert_eq!(rev_parse(&bare, &format!("{sha}^")), tip);
    }

    #[tokio::test]
    async fn replaces_the_file_when_the_base_already_has_it() {
        let tmp = tempfile::tempdir().unwrap();
        let (bare, tip) = repo_with_a_commit(tmp.path());
        let writer = GitBranchFileWriter::new(tmp.path().to_path_buf());

        let sha = writer
            .commit_file_to_new_branch(
                "bare.git",
                "edit",
                &tip,
                "README.md",
                "bye\n",
                "m",
                "f",
                "f@e.com",
            )
            .await
            .unwrap();

        assert_eq!(
            git_stdout_untrimmed(&bare, &["show", &format!("{sha}:README.md")]),
            "bye\n"
        );
    }

    #[tokio::test]
    async fn a_branch_name_that_is_taken_is_a_conflict_and_the_branch_is_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let (bare, tip) = repo_with_a_commit(tmp.path());
        let writer = GitBranchFileWriter::new(tmp.path().to_path_buf());
        let first = writer
            .commit_file_to_new_branch(
                "bare.git", "edit", &tip, "a.txt", "1\n", "m", "f", "f@e.com",
            )
            .await
            .unwrap();

        let second = writer
            .commit_file_to_new_branch(
                "bare.git", "edit", &tip, "a.txt", "2\n", "m", "f", "f@e.com",
            )
            .await;

        assert!(matches!(second, Err(DomainError::Conflict(_))));
        assert_eq!(rev_parse(&bare, "refs/heads/edit"), first);
    }

    #[tokio::test]
    async fn deletes_the_new_branch_only_while_it_holds_the_commit_it_was_made_with() {
        let tmp = tempfile::tempdir().unwrap();
        let (bare, tip) = repo_with_a_commit(tmp.path());
        let writer = GitBranchFileWriter::new(tmp.path().to_path_buf());
        let sha = writer
            .commit_file_to_new_branch(
                "bare.git", "edit", &tip, "a.txt", "1\n", "m", "f", "f@e.com",
            )
            .await
            .unwrap();

        let moved = writer.delete_new_branch("bare.git", "edit", &tip).await;
        assert!(matches!(moved, Err(DomainError::Conflict(_))));
        assert_eq!(rev_parse(&bare, "refs/heads/edit"), sha);

        writer
            .delete_new_branch("bare.git", "edit", &sha)
            .await
            .unwrap();
        assert!(
            git_stdout_untrimmed(&bare, &["branch", "--list", "edit"]).is_empty(),
            "the branch is gone"
        );
        assert_eq!(rev_parse(&bare, "refs/heads/main"), tip);
    }

    #[tokio::test]
    async fn refuses_a_name_that_is_not_a_branch_name() {
        let tmp = tempfile::tempdir().unwrap();
        let (_bare, tip) = repo_with_a_commit(tmp.path());
        let writer = GitBranchFileWriter::new(tmp.path().to_path_buf());

        for name in ["a b", "-x", "a..b", "main.lock", ""] {
            let result = writer
                .commit_file_to_new_branch(
                    "bare.git", name, &tip, "a.txt", "x\n", "m", "f", "f@e.com",
                )
                .await;
            assert!(
                matches!(result, Err(DomainError::Validation(_))),
                "{name:?}"
            );
        }
    }
}
