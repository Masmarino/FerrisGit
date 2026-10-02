use std::path::{Path, PathBuf};

use crate::git_cli;
use async_trait::async_trait;
use ferrisgit_domain::apply_suggestion_executor::ApplySuggestionExecutorPort;
use ferrisgit_domain::error::DomainError;
use uuid::Uuid;

pub struct GitApplySuggestionExecutor {
    storage_root: PathBuf,
}

impl GitApplySuggestionExecutor {
    pub fn new(storage_root: PathBuf) -> Self {
        Self { storage_root }
    }
}

/// Returns stdout untrimmed: callers reading file content must not trim it, and callers that want a
/// single token use `run_git`, which trims.
async fn run_git_raw(
    repo_path: &Path,
    args: &[&str],
    env: &[(&str, &str)],
    stdin_data: Option<&str>,
) -> Result<(bool, Vec<u8>, String), DomainError> {
    let output = git_cli::run(repo_path, args, env, stdin_data.map(str::as_bytes)).await?;
    Ok((output.success, output.stdout, output.stderr))
}

async fn run_git(
    repo_path: &Path,
    args: &[&str],
    env: &[(&str, &str)],
    stdin_data: Option<&str>,
) -> Result<(bool, String, String), DomainError> {
    let output = git_cli::run(repo_path, args, env, stdin_data.map(str::as_bytes)).await?;
    Ok((output.success, output.stdout_trimmed(), output.stderr))
}

/// Splits `content` into lines that keep their trailing `\n` (the last keeps none if the file has none),
/// matching `DiffLine::content` so spliced lines need no newline adjustment.
fn split_keep_newlines(content: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (i, c) in content.char_indices() {
        if c == '\n' {
            lines.push(content[start..=i].to_string());
            start = i + 1;
        }
    }
    if start < content.len() {
        lines.push(content[start..].to_string());
    }
    lines
}

#[async_trait]
impl ApplySuggestionExecutorPort for GitApplySuggestionExecutor {
    async fn apply_replacement(
        &self,
        repository_disk_path: &str,
        branch: &str,
        expected_tip: &str,
        file_path: &str,
        start_line: i32,
        end_line: i32,
        new_content: &str,
        message: &str,
        committer_name: &str,
        committer_email: &str,
    ) -> Result<String, DomainError> {
        let repo_path = self.storage_root.join(repository_disk_path);

        let (ok, file_content_bytes, stderr) = run_git_raw(
            &repo_path,
            &["show", &format!("{expected_tip}:{file_path}")],
            &[],
            None,
        )
        .await?;
        if !ok {
            return Err(DomainError::Infrastructure(format!(
                "failed to read {file_path} at {expected_tip}: {stderr}"
            )));
        }
        let file_content = String::from_utf8_lossy(&file_content_bytes).into_owned();

        let mut lines = split_keep_newlines(&file_content);
        let start_idx = (start_line as usize).saturating_sub(1);
        let end_idx = end_line as usize;
        if start_line < 1 || end_idx > lines.len() || start_idx >= end_idx {
            return Err(DomainError::Validation(
                "suggestion's line range is no longer valid for this file".to_string(),
            ));
        }
        let new_lines = split_keep_newlines(new_content);
        lines.splice(start_idx..end_idx, new_lines);
        let new_file_content = lines.concat();

        let (ok, blob_oid, stderr) = run_git(
            &repo_path,
            &["hash-object", "-w", "--stdin"],
            &[],
            Some(&new_file_content),
        )
        .await?;
        if !ok {
            return Err(DomainError::Infrastructure(format!(
                "git hash-object failed: {stderr}"
            )));
        }

        let index_path =
            std::env::temp_dir().join(format!("ferrisgit-suggestion-index-{}", Uuid::new_v4()));
        let index_path_str = index_path.to_string_lossy().into_owned();
        let index_env: [(&str, &str); 1] = [("GIT_INDEX_FILE", index_path_str.as_str())];

        let (ok, _, stderr) =
            run_git(&repo_path, &["read-tree", expected_tip], &index_env, None).await?;
        if !ok {
            return Err(DomainError::Infrastructure(format!(
                "git read-tree failed: {stderr}"
            )));
        }
        let cacheinfo = format!("100644,{blob_oid},{file_path}");
        let (ok, _, stderr) = run_git(
            &repo_path,
            &["update-index", "--add", "--cacheinfo", &cacheinfo],
            &index_env,
            None,
        )
        .await?;
        if !ok {
            std::fs::remove_file(&index_path).ok();
            return Err(DomainError::Infrastructure(format!(
                "git update-index failed: {stderr}"
            )));
        }
        let (ok, tree_oid, stderr) = run_git(&repo_path, &["write-tree"], &index_env, None).await?;
        std::fs::remove_file(&index_path).ok();
        if !ok {
            return Err(DomainError::Infrastructure(format!(
                "git write-tree failed: {stderr}"
            )));
        }

        let identity_env = [
            ("GIT_AUTHOR_NAME", committer_name),
            ("GIT_AUTHOR_EMAIL", committer_email),
            ("GIT_COMMITTER_NAME", committer_name),
            ("GIT_COMMITTER_EMAIL", committer_email),
        ];
        let (ok, commit_sha, stderr) = run_git(
            &repo_path,
            &["commit-tree", &tree_oid, "-p", expected_tip, "-m", message],
            &identity_env,
            None,
        )
        .await?;
        if !ok {
            return Err(DomainError::Infrastructure(format!(
                "git commit-tree failed: {stderr}"
            )));
        }

        let branch_ref = format!("refs/heads/{branch}");
        let (ok, _, stderr) = run_git(
            &repo_path,
            &["update-ref", &branch_ref, &commit_sha, expected_tip],
            &[],
            None,
        )
        .await?;
        if !ok {
            return Err(DomainError::Conflict(format!(
                "the source branch changed since this suggestion was last checked — reload and try again: {stderr}"
            )));
        }

        Ok(commit_sha)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_git::{git, git_stdout_untrimmed, init_bare_with_clone, rev_parse};

    fn init_bare_repo_with_a_file(dir: &Path, filename: &str, content: &str) -> (PathBuf, PathBuf) {
        let (bare, work) = init_bare_with_clone(dir);
        std::fs::write(work.join(filename), content).unwrap();
        git(&work, &["add", "."]);
        git(&work, &["commit", "-q", "-m", "root"]);
        git(&work, &["push", "-q", "origin", "main"]);
        (bare, work)
    }

    fn show(repo_path: &Path, rev_path: &str) -> String {
        git_stdout_untrimmed(repo_path, &["show", rev_path])
    }

    #[tokio::test]
    async fn replaces_a_single_line_and_moves_the_branch_ref() {
        let tmp = tempfile::tempdir().unwrap();
        let (bare_path, _work_path) =
            init_bare_repo_with_a_file(tmp.path(), "README.md", "line one\nline two\nline three\n");
        let tip = rev_parse(&bare_path, "refs/heads/main");
        let executor = GitApplySuggestionExecutor::new(tmp.path().to_path_buf());

        let commit_sha = executor
            .apply_replacement(
                "bare.git",
                "main",
                &tip,
                "README.md",
                2,
                2,
                "line TWO\n",
                "Apply suggestion",
                "florian",
                "florian@example.com",
            )
            .await
            .unwrap();

        let tip_after = rev_parse(&bare_path, "refs/heads/main");
        assert_eq!(
            tip_after, commit_sha,
            "the branch ref must now point at the new commit"
        );
        let content = show(&bare_path, &format!("{commit_sha}:README.md"));
        assert_eq!(content, "line one\nline TWO\nline three\n");
    }

    #[tokio::test]
    async fn replaces_a_multi_line_range_with_a_different_number_of_replacement_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let (bare_path, _work_path) = init_bare_repo_with_a_file(
            tmp.path(),
            "README.md",
            "line one\nline two\nline three\nline four\n",
        );
        let tip = rev_parse(&bare_path, "refs/heads/main");
        let executor = GitApplySuggestionExecutor::new(tmp.path().to_path_buf());

        let commit_sha = executor
            .apply_replacement(
                "bare.git",
                "main",
                &tip,
                "README.md",
                2,
                3,
                "replacement one\nreplacement two\nreplacement three\n",
                "Apply suggestion",
                "florian",
                "florian@example.com",
            )
            .await
            .unwrap();

        let content = show(&bare_path, &format!("{commit_sha}:README.md"));
        assert_eq!(
            content,
            "line one\nreplacement one\nreplacement two\nreplacement three\nline four\n"
        );
    }

    #[tokio::test]
    async fn works_on_a_file_nested_in_a_subdirectory() {
        let tmp = tempfile::tempdir().unwrap();
        let (bare, work) = init_bare_with_clone(tmp.path());
        std::fs::create_dir_all(work.join("src").join("app")).unwrap();
        std::fs::write(
            work.join("src").join("app").join("main.rs"),
            "fn main() {\n    old();\n}\n",
        )
        .unwrap();
        git(&work, &["add", "."]);
        git(&work, &["commit", "-q", "-m", "root"]);
        git(&work, &["push", "-q", "origin", "main"]);
        let tip = rev_parse(&bare, "refs/heads/main");
        let executor = GitApplySuggestionExecutor::new(tmp.path().to_path_buf());

        let commit_sha = executor
            .apply_replacement(
                "bare.git",
                "main",
                &tip,
                "src/app/main.rs",
                2,
                2,
                "    new();\n",
                "Apply suggestion",
                "florian",
                "florian@example.com",
            )
            .await
            .unwrap();

        let content = show(&bare, &format!("{commit_sha}:src/app/main.rs"));
        assert_eq!(content, "fn main() {\n    new();\n}\n");
    }

    #[tokio::test]
    async fn replaces_the_first_line_of_a_file() {
        let tmp = tempfile::tempdir().unwrap();
        let (bare_path, _work_path) =
            init_bare_repo_with_a_file(tmp.path(), "README.md", "line one\nline two\nline three\n");
        let tip = rev_parse(&bare_path, "refs/heads/main");
        let executor = GitApplySuggestionExecutor::new(tmp.path().to_path_buf());

        let commit_sha = executor
            .apply_replacement(
                "bare.git",
                "main",
                &tip,
                "README.md",
                1,
                1,
                "LINE ONE\n",
                "Apply suggestion",
                "florian",
                "florian@example.com",
            )
            .await
            .unwrap();

        let content = show(&bare_path, &format!("{commit_sha}:README.md"));
        assert_eq!(content, "LINE ONE\nline two\nline three\n");
    }

    #[tokio::test]
    async fn replaces_the_last_line_of_a_file_that_has_no_trailing_newline() {
        let tmp = tempfile::tempdir().unwrap();
        let (bare_path, _work_path) =
            init_bare_repo_with_a_file(tmp.path(), "README.md", "line one\nline two");
        let tip = rev_parse(&bare_path, "refs/heads/main");
        let executor = GitApplySuggestionExecutor::new(tmp.path().to_path_buf());

        let commit_sha = executor
            .apply_replacement(
                "bare.git",
                "main",
                &tip,
                "README.md",
                2,
                2,
                "line TWO",
                "Apply suggestion",
                "florian",
                "florian@example.com",
            )
            .await
            .unwrap();

        let content = show(&bare_path, &format!("{commit_sha}:README.md"));
        assert_eq!(
            content, "line one\nline TWO",
            "the replacement's own lack of a trailing newline must be preserved, not silently added"
        );
    }

    #[tokio::test]
    async fn an_empty_replacement_deletes_a_single_line_rather_than_erroring() {
        let tmp = tempfile::tempdir().unwrap();
        let (bare_path, _work_path) =
            init_bare_repo_with_a_file(tmp.path(), "README.md", "line one\nline two\nline three\n");
        let tip = rev_parse(&bare_path, "refs/heads/main");
        let executor = GitApplySuggestionExecutor::new(tmp.path().to_path_buf());

        let commit_sha = executor
            .apply_replacement(
                "bare.git",
                "main",
                &tip,
                "README.md",
                2,
                2,
                "",
                "Apply suggestion",
                "florian",
                "florian@example.com",
            )
            .await
            .unwrap();

        let content = show(&bare_path, &format!("{commit_sha}:README.md"));
        assert_eq!(
            content, "line one\nline three\n",
            "an empty suggestion must delete the anchored line outright, not leave a blank line or error"
        );
    }

    #[tokio::test]
    async fn an_empty_replacement_covering_the_whole_file_leaves_a_valid_empty_file() {
        let tmp = tempfile::tempdir().unwrap();
        let (bare_path, _work_path) =
            init_bare_repo_with_a_file(tmp.path(), "README.md", "line one\nline two\n");
        let tip = rev_parse(&bare_path, "refs/heads/main");
        let executor = GitApplySuggestionExecutor::new(tmp.path().to_path_buf());

        let commit_sha = executor
            .apply_replacement(
                "bare.git",
                "main",
                &tip,
                "README.md",
                1,
                2,
                "",
                "Apply suggestion",
                "florian",
                "florian@example.com",
            )
            .await
            .unwrap();

        let content = show(&bare_path, &format!("{commit_sha}:README.md"));
        assert_eq!(
            content, "",
            "deleting every line of a file via a suggestion must leave a valid, empty file, not error or corrupt the tree"
        );
    }

    #[tokio::test]
    async fn a_stale_expected_tip_is_rejected_and_the_branch_ref_is_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let (bare_path, work_path) =
            init_bare_repo_with_a_file(tmp.path(), "README.md", "line one\nline two\n");
        let stale_tip = rev_parse(&bare_path, "refs/heads/main");

        // Move the branch tip past the stale `expected_tip`, as another push racing ahead would.
        git(
            &work_path,
            &[
                "commit",
                "--allow-empty",
                "-q",
                "-m",
                "a later, unrelated commit",
            ],
        );
        git(&work_path, &["push", "-q", "origin", "main"]);
        let real_tip = rev_parse(&bare_path, "refs/heads/main");
        assert_ne!(real_tip, stale_tip);

        let executor = GitApplySuggestionExecutor::new(tmp.path().to_path_buf());
        let result = executor
            .apply_replacement(
                "bare.git",
                "main",
                &stale_tip,
                "README.md",
                2,
                2,
                "line TWO\n",
                "Apply suggestion",
                "florian",
                "florian@example.com",
            )
            .await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
        let tip_after = rev_parse(&bare_path, "refs/heads/main");
        assert_eq!(
            tip_after, real_tip,
            "a rejected apply must not move the branch ref at all"
        );
    }
}
