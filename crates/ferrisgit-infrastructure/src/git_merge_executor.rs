use std::path::{Path, PathBuf};

use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::merge_executor::{MergeExecutorPort, MergeOutcome};
use tokio::process::Command;

pub struct GitMergeExecutor {
    storage_root: PathBuf,
}

impl GitMergeExecutor {
    pub fn new(storage_root: PathBuf) -> Self {
        Self { storage_root }
    }
}

/// Merge commits are authored by the server itself: the port carries no user identity, and the
/// production container has no global git config, so without this `git commit-tree` fails with
/// "Author identity unknown".
const MERGE_IDENTITY: [(&str, &str); 4] = [
    ("GIT_AUTHOR_NAME", "FerrisGit"),
    ("GIT_AUTHOR_EMAIL", "noreply@ferrisgit.local"),
    ("GIT_COMMITTER_NAME", "FerrisGit"),
    ("GIT_COMMITTER_EMAIL", "noreply@ferrisgit.local"),
];

async fn run_git(repo_path: &Path, args: &[&str]) -> Result<(bool, String, String), DomainError> {
    run_git_with_env(repo_path, args, &[]).await
}

async fn run_git_with_env(
    repo_path: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Result<(bool, String, String), DomainError> {
    let output = Command::new("git")
        .args(args)
        .envs(envs.iter().copied())
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

#[async_trait]
impl MergeExecutorPort for GitMergeExecutor {
    async fn merge(
        &self,
        repository_disk_path: &str,
        source_branch: &str,
        target_branch: &str,
        message: &str,
    ) -> Result<MergeOutcome, DomainError> {
        let repo_path = self.storage_root.join(repository_disk_path);

        let target_ref = format!("refs/heads/{target_branch}");
        let source_ref = format!("refs/heads/{source_branch}");
        let (ok, target_tip, stderr) = run_git(&repo_path, &["rev-parse", &target_ref]).await?;
        if !ok {
            return Err(DomainError::Infrastructure(format!(
                "failed to resolve target branch {target_branch}: {stderr}"
            )));
        }
        let (ok, source_tip, stderr) = run_git(&repo_path, &["rev-parse", &source_ref]).await?;
        if !ok {
            return Err(DomainError::Infrastructure(format!(
                "failed to resolve source branch {source_branch}: {stderr}"
            )));
        }

        let output = Command::new("git")
            .args(["merge-tree", "--write-tree", &target_tip, &source_tip])
            .current_dir(&repo_path)
            .output()
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;

        let tree_oid = match output.status.code() {
            Some(0) => String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or_default()
                .trim()
                .to_string(),
            Some(1) => return Ok(MergeOutcome::Conflicting),
            _ => {
                return Err(DomainError::Infrastructure(format!(
                    "git merge-tree failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
        };
        if tree_oid.is_empty() {
            return Err(DomainError::Infrastructure(
                "git merge-tree produced no tree oid".to_string(),
            ));
        }

        let (ok, commit_sha, stderr) = run_git_with_env(
            &repo_path,
            &[
                "commit-tree",
                &tree_oid,
                "-p",
                &target_tip,
                "-p",
                &source_tip,
                "-m",
                message,
            ],
            &MERGE_IDENTITY,
        )
        .await?;
        if !ok {
            return Err(DomainError::Infrastructure(format!(
                "git commit-tree failed: {stderr}"
            )));
        }

        let (ok, _, stderr) = run_git(
            &repo_path,
            &["update-ref", &target_ref, &commit_sha, &target_tip],
        )
        .await?;
        if !ok {
            return Err(DomainError::Conflict(format!(
                "target branch {target_branch} changed since this merge started, please retry: {stderr}"
            )));
        }

        Ok(MergeOutcome::Merged { commit_sha })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;

    fn init_bare_repo_with_diverging_branches(dir: &Path) -> (String, String) {
        // Builds history in a working checkout and pushes into a bare repo, as production stores repositories.
        let bare = dir.join("bare.git");
        assert!(
            StdCommand::new("git")
                .args(["init", "--bare", "-q", "-b", "main"])
                .arg(&bare)
                .status()
                .unwrap()
                .success()
        );

        let work = dir.join("work");
        assert!(
            StdCommand::new("git")
                .args(["clone", "-q"])
                .arg(&bare)
                .arg(&work)
                .status()
                .unwrap()
                .success()
        );
        let run = |args: &[&str]| {
            assert!(
                StdCommand::new("git")
                    .args(args)
                    .current_dir(&work)
                    .status()
                    .unwrap()
                    .success()
            );
        };
        let commit = |message: &str| {
            assert!(
                StdCommand::new("git")
                    .args([
                        "-c",
                        "user.email=t@t.com",
                        "-c",
                        "user.name=t",
                        "commit",
                        "--allow-empty",
                        "-q",
                        "-m",
                        message
                    ])
                    .current_dir(&work)
                    .status()
                    .unwrap()
                    .success()
            );
        };

        commit("root");
        run(&["push", "-q", "origin", "main"]);
        run(&["checkout", "-q", "-b", "feature"]);
        commit("feature work");
        run(&["push", "-q", "origin", "feature"]);
        run(&["checkout", "-q", "main"]);

        (
            bare.to_string_lossy().to_string(),
            work.to_string_lossy().to_string(),
        )
    }

    fn rev_parse(repo_path: &Path, rev: &str) -> String {
        let output = StdCommand::new("git")
            .args(["rev-parse", rev])
            .current_dir(repo_path)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    #[tokio::test]
    async fn a_clean_merge_moves_the_target_ref_to_a_real_two_parent_commit() {
        let tmp = tempfile::tempdir().unwrap();
        let (bare_path, work_path) = init_bare_repo_with_diverging_branches(tmp.path());
        let target_tip_before = rev_parse(bare_path.as_ref(), "refs/heads/main");
        let executor = GitMergeExecutor::new(tmp.path().to_path_buf());

        let outcome = executor
            .merge("bare.git", "feature", "main", "Merge feature into main")
            .await
            .unwrap();

        let MergeOutcome::Merged { commit_sha } = outcome else {
            panic!("expected a clean merge")
        };
        let target_tip_after = rev_parse(bare_path.as_ref(), "refs/heads/main");
        assert_eq!(
            target_tip_after, commit_sha,
            "the target ref must now point at the new merge commit"
        );
        assert_ne!(target_tip_after, target_tip_before);

        let parents_output = StdCommand::new("git")
            .args(["log", "--pretty=%P", "-1", &commit_sha])
            .current_dir(&bare_path)
            .output()
            .unwrap();
        let parents: Vec<&str> = std::str::from_utf8(&parents_output.stdout)
            .unwrap()
            .split_whitespace()
            .collect();
        assert_eq!(
            parents.len(),
            2,
            "a merge commit must have exactly two parents"
        );
        let _ = work_path; // only used to build history via a real working checkout
    }

    #[tokio::test]
    async fn the_merge_commit_carries_a_fixed_server_identity_instead_of_depending_on_git_config() {
        // Regression: without a global git identity (as in the production container) `git commit-tree` failed.
        let tmp = tempfile::tempdir().unwrap();
        let (bare_path, _work_path) = init_bare_repo_with_diverging_branches(tmp.path());
        let executor = GitMergeExecutor::new(tmp.path().to_path_buf());

        let outcome = executor
            .merge("bare.git", "feature", "main", "Merge feature into main")
            .await
            .unwrap();

        let MergeOutcome::Merged { commit_sha } = outcome else {
            panic!("expected a clean merge")
        };
        let log = StdCommand::new("git")
            .args(["log", "--pretty=%an <%ae> / %cn <%ce>", "-1", &commit_sha])
            .current_dir(&bare_path)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&log.stdout).trim(),
            "FerrisGit <noreply@ferrisgit.local> / FerrisGit <noreply@ferrisgit.local>"
        );
    }

    #[tokio::test]
    async fn a_conflicting_merge_leaves_the_repository_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("bare.git");
        assert!(
            StdCommand::new("git")
                .args(["init", "--bare", "-q", "-b", "main"])
                .arg(&bare)
                .status()
                .unwrap()
                .success()
        );
        let work = tmp.path().join("work");
        assert!(
            StdCommand::new("git")
                .args(["clone", "-q"])
                .arg(&bare)
                .arg(&work)
                .status()
                .unwrap()
                .success()
        );
        let run = |args: &[&str]| {
            assert!(
                StdCommand::new("git")
                    .args(args)
                    .current_dir(&work)
                    .status()
                    .unwrap()
                    .success()
            );
        };
        let commit = |message: &str| {
            assert!(
                StdCommand::new("git")
                    .args([
                        "-c",
                        "user.email=t@t.com",
                        "-c",
                        "user.name=t",
                        "commit",
                        "-q",
                        "-m",
                        message
                    ])
                    .current_dir(&work)
                    .status()
                    .unwrap()
                    .success()
            );
        };

        std::fs::write(work.join("README.md"), "line one\n").unwrap();
        run(&["add", "."]);
        commit("root");
        run(&["push", "-q", "origin", "main"]);
        run(&["checkout", "-q", "-b", "feature"]);
        std::fs::write(work.join("README.md"), "feature's version\n").unwrap();
        run(&["add", "."]);
        commit("feature changes the same line");
        run(&["push", "-q", "origin", "feature"]);
        run(&["checkout", "-q", "main"]);
        std::fs::write(work.join("README.md"), "main's version\n").unwrap();
        run(&["add", "."]);
        commit("main changes the same line differently");
        run(&["push", "-q", "origin", "main"]);

        let target_tip_before = rev_parse(&bare, "refs/heads/main");
        let executor = GitMergeExecutor::new(tmp.path().to_path_buf());

        let outcome = executor
            .merge("bare.git", "feature", "main", "Merge feature into main")
            .await
            .unwrap();

        assert_eq!(outcome, MergeOutcome::Conflicting);
        let target_tip_after = rev_parse(&bare, "refs/heads/main");
        assert_eq!(
            target_tip_after, target_tip_before,
            "a conflicting merge must not move the target ref at all"
        );
    }

    #[tokio::test]
    async fn update_ref_with_a_stale_expected_old_value_is_rejected_by_git_itself() {
        // Tests the compare-and-swap directly, because the race inside `merge()` can't be reproduced deterministically.
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("bare.git");
        assert!(
            StdCommand::new("git")
                .args(["init", "--bare", "-q", "-b", "main"])
                .arg(&bare)
                .status()
                .unwrap()
                .success()
        );
        let work = tmp.path().join("work");
        assert!(
            StdCommand::new("git")
                .args(["clone", "-q"])
                .arg(&bare)
                .arg(&work)
                .status()
                .unwrap()
                .success()
        );
        let run = |args: &[&str]| {
            assert!(
                StdCommand::new("git")
                    .args(args)
                    .current_dir(&work)
                    .status()
                    .unwrap()
                    .success()
            );
        };
        let commit = |message: &str| {
            assert!(
                StdCommand::new("git")
                    .args([
                        "-c",
                        "user.email=t@t.com",
                        "-c",
                        "user.name=t",
                        "commit",
                        "--allow-empty",
                        "-q",
                        "-m",
                        message
                    ])
                    .current_dir(&work)
                    .status()
                    .unwrap()
                    .success()
            );
        };
        commit("root");
        run(&["push", "-q", "origin", "main"]);
        let stale_tip = rev_parse(&bare, "refs/heads/main");
        commit("a real, later commit that moved main");
        run(&["push", "-q", "origin", "main"]);
        let current_tip = rev_parse(&bare, "refs/heads/main");
        assert_ne!(stale_tip, current_tip);

        let status = StdCommand::new("git")
            .args(["update-ref", "refs/heads/main", &current_tip, &stale_tip])
            .current_dir(&bare)
            .status()
            .unwrap();

        assert!(
            !status.success(),
            "update-ref must refuse to write when the given expected-old value is stale"
        );
        assert_eq!(
            rev_parse(&bare, "refs/heads/main"),
            current_tip,
            "the ref must be unchanged after the rejected write"
        );
    }
}
