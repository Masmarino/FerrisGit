use std::path::{Path, PathBuf};

use crate::git_cli;
use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::merge_executor::{
    MergeExecutorPort, MergeMethod, MergeOutcome, MergeRef, MergeRequestMerge,
};

pub struct GitMergeExecutor {
    storage_root: PathBuf,
}

impl GitMergeExecutor {
    pub fn new(storage_root: PathBuf) -> Self {
        Self { storage_root }
    }
}

/// The port has no user identity and the production container has no git config, so without this
/// commit-tree fails with "Author identity unknown".
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
    let output = git_cli::run(repo_path, args, envs, None).await?;
    Ok((output.success, output.stdout_trimmed(), output.stderr))
}

/// The tip of `refs/heads/{branch}`.
async fn branch_tip(repo_path: &Path, branch: &str, role: &str) -> Result<String, DomainError> {
    let (ok, tip, stderr) =
        run_git(repo_path, &["rev-parse", &format!("refs/heads/{branch}")]).await?;
    if !ok {
        return Err(DomainError::Infrastructure(format!(
            "failed to resolve {role} branch {branch}: {stderr}"
        )));
    }
    Ok(tip)
}

/// The tree of merging `theirs` into `ours`, with `merge_base` as the common ancestor when given (to replay one commit),
/// or the computed one. `None` when they conflict.
async fn merge_tree(
    repo_path: &Path,
    merge_base: Option<&str>,
    ours: &str,
    theirs: &str,
) -> Result<Option<String>, DomainError> {
    let base_arg = merge_base.map(|base| format!("--merge-base={base}"));
    let mut args = vec!["merge-tree", "--write-tree"];
    args.extend(base_arg.as_deref());
    args.extend([ours, theirs]);
    let output = git_cli::run(repo_path, &args, &[], None).await?;
    let tree_oid = match output.code {
        Some(0) => String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_string(),
        Some(1) => return Ok(None),
        _ => {
            return Err(DomainError::Infrastructure(format!(
                "git merge-tree failed: {}",
                output.stderr
            )));
        }
    };
    if tree_oid.is_empty() {
        return Err(DomainError::Infrastructure(
            "git merge-tree produced no tree oid".to_string(),
        ));
    }
    Ok(Some(tree_oid))
}

/// The two-parent merge commit of `source_tip` into `target_tip`, written to the object store only. `None` when the
/// two conflict.
async fn merge_commit(
    repo_path: &Path,
    target_tip: &str,
    source_tip: &str,
    message: &str,
) -> Result<Option<String>, DomainError> {
    let Some(tree) = merge_tree(repo_path, None, target_tip, source_tip).await? else {
        return Ok(None);
    };
    commit_tree(
        repo_path,
        &tree,
        &[target_tip, source_tip],
        message,
        &MERGE_IDENTITY,
    )
    .await
    .map(Some)
}

/// Writes a commit of `tree` on `parents`, the message read from stdin so it is kept byte for byte.
async fn commit_tree(
    repo_path: &Path,
    tree: &str,
    parents: &[&str],
    message: &str,
    env: &[(&str, &str)],
) -> Result<String, DomainError> {
    let mut args = vec!["commit-tree", tree];
    for parent in parents {
        args.extend(["-p", parent]);
    }
    let message = if message.ends_with('\n') {
        message.to_string()
    } else {
        format!("{message}\n")
    };
    let output = git_cli::run(repo_path, &args, env, Some(message.as_bytes())).await?;
    if !output.success {
        return Err(DomainError::Infrastructure(format!(
            "git commit-tree failed: {}",
            output.stderr
        )));
    }
    Ok(output.stdout_trimmed())
}

/// The commits of `source_tip` not in `target_tip`, oldest first, replayed one by one on top of `target_tip`. Each keeps
/// its author, date and message; FerrisGit is the committer. `None` on a conflict. A target the source already contains
/// is fast-forwarded: nothing needs rewriting.
async fn rebase_onto(
    repo_path: &Path,
    target_tip: &str,
    source_tip: &str,
) -> Result<Option<String>, DomainError> {
    let (ok, _, _) = run_git(
        repo_path,
        &["merge-base", "--is-ancestor", target_tip, source_tip],
    )
    .await?;
    if ok {
        return Ok(Some(source_tip.to_string()));
    }
    let range = format!("{target_tip}..{source_tip}");
    let (ok, merges, stderr) = run_git(repo_path, &["rev-list", "--min-parents=2", &range]).await?;
    if !ok {
        return Err(DomainError::Infrastructure(format!(
            "git rev-list failed: {stderr}"
        )));
    }
    if !merges.is_empty() {
        return Err(DomainError::Validation(
            "the source branch contains merge commits and cannot be rebased: use a merge commit or a squash instead"
                .to_string(),
        ));
    }
    let (ok, commits, stderr) = run_git(
        repo_path,
        &["rev-list", "--reverse", "--topo-order", &range],
    )
    .await?;
    if !ok {
        return Err(DomainError::Infrastructure(format!(
            "git rev-list failed: {stderr}"
        )));
    }

    let mut new_tip = target_tip.to_string();
    for commit in commits.lines().filter(|line| !line.is_empty()) {
        let parent = format!("{commit}^");
        let Some(tree) = merge_tree(repo_path, Some(&parent), &new_tip, commit).await? else {
            return Ok(None);
        };
        let (ok, author, stderr) = run_git(
            repo_path,
            &[
                "show",
                "-s",
                "--format=%an%x00%ae%x00%ad",
                "--date=raw",
                commit,
            ],
        )
        .await?;
        if !ok {
            return Err(DomainError::Infrastructure(format!(
                "failed to read commit {commit}: {stderr}"
            )));
        }
        let mut fields = author.split('\0');
        let (Some(name), Some(email), Some(date)) = (fields.next(), fields.next(), fields.next())
        else {
            return Err(DomainError::Infrastructure(format!(
                "unexpected author line for commit {commit}"
            )));
        };
        let message =
            git_cli::run(repo_path, &["show", "-s", "--format=%B", commit], &[], None).await?;
        if !message.success {
            return Err(DomainError::Infrastructure(format!(
                "failed to read the message of commit {commit}: {}",
                message.stderr
            )));
        }
        let message = String::from_utf8_lossy(&message.stdout);
        let env = [
            ("GIT_AUTHOR_NAME", name),
            ("GIT_AUTHOR_EMAIL", email),
            ("GIT_AUTHOR_DATE", date),
            MERGE_IDENTITY[2],
            MERGE_IDENTITY[3],
        ];
        new_tip = commit_tree(repo_path, &tree, &[&new_tip], message.trim_end(), &env).await?;
    }
    Ok(Some(new_tip))
}

#[async_trait]
impl MergeExecutorPort for GitMergeExecutor {
    async fn merge(
        &self,
        repository_disk_path: &str,
        merge: &MergeRequestMerge,
    ) -> Result<MergeOutcome, DomainError> {
        let repo_path = self.storage_root.join(repository_disk_path);

        let target_ref = format!("refs/heads/{}", merge.target_branch);
        let target_tip = branch_tip(&repo_path, &merge.target_branch, "target").await?;
        let source_tip = branch_tip(&repo_path, &merge.source_branch, "source").await?;

        let new_tip = match merge.method {
            MergeMethod::Merge => {
                let Some(commit_sha) =
                    merge_commit(&repo_path, &target_tip, &source_tip, &merge.message).await?
                else {
                    return Ok(MergeOutcome::Conflicting);
                };
                commit_sha
            }
            MergeMethod::Squash => {
                let Some(tree) = merge_tree(&repo_path, None, &target_tip, &source_tip).await?
                else {
                    return Ok(MergeOutcome::Conflicting);
                };
                let author = merge.squash_author.as_ref();
                let env = [
                    (
                        "GIT_AUTHOR_NAME",
                        author.map_or(MERGE_IDENTITY[0].1, |a| a.name.as_str()),
                    ),
                    (
                        "GIT_AUTHOR_EMAIL",
                        author.map_or(MERGE_IDENTITY[1].1, |a| a.email.as_str()),
                    ),
                    MERGE_IDENTITY[2],
                    MERGE_IDENTITY[3],
                ];
                commit_tree(&repo_path, &tree, &[&target_tip], &merge.message, &env).await?
            }
            MergeMethod::Rebase => {
                let Some(tip) = rebase_onto(&repo_path, &target_tip, &source_tip).await? else {
                    return Ok(MergeOutcome::Conflicting);
                };
                tip
            }
        };

        let (ok, _, stderr) = run_git(
            &repo_path,
            &["update-ref", &target_ref, &new_tip, &target_tip],
        )
        .await?;
        if !ok {
            return Err(DomainError::Conflict(format!(
                "target branch {} changed since this merge started, please retry: {stderr}",
                merge.target_branch
            )));
        }

        Ok(MergeOutcome::Merged {
            commit_sha: new_tip,
        })
    }

    async fn write_merge_ref(
        &self,
        repository_disk_path: &str,
        source_branch: &str,
        target_branch: &str,
        ref_name: &str,
        message: &str,
    ) -> Result<Option<MergeRef>, DomainError> {
        if ref_name.starts_with("refs/heads/") || !ref_name.starts_with("refs/") {
            return Err(DomainError::Validation(format!(
                "{ref_name} is not a ref outside the branches"
            )));
        }
        let repo_path = self.storage_root.join(repository_disk_path);
        let target_sha = branch_tip(&repo_path, target_branch, "target").await?;
        let source_sha = branch_tip(&repo_path, source_branch, "source").await?;
        let Some(commit_sha) = merge_commit(&repo_path, &target_sha, &source_sha, message).await?
        else {
            return Ok(None);
        };
        let (ok, _, stderr) = run_git(&repo_path, &["update-ref", ref_name, &commit_sha]).await?;
        if !ok {
            return Err(DomainError::Infrastructure(format!(
                "failed to write {ref_name}: {stderr}"
            )));
        }
        Ok(Some(MergeRef {
            commit_sha,
            source_sha,
            target_sha,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_git::{git, git_command, git_stdout, init_bare_with_clone, rev_parse};

    fn merge_of(method: MergeMethod) -> MergeRequestMerge {
        MergeRequestMerge {
            source_branch: "feature".to_string(),
            target_branch: "main".to_string(),
            method,
            message: "Merge feature into main".to_string(),
            squash_author: None,
        }
    }

    /// Bare repo with `feature` one commit ahead of `main`.
    fn init_bare_repo_with_diverging_branches(dir: &Path) -> PathBuf {
        let (bare, work) = init_bare_with_clone(dir);
        let commit = |message: &str| git(&work, &["commit", "--allow-empty", "-q", "-m", message]);

        commit("root");
        git(&work, &["push", "-q", "origin", "main"]);
        git(&work, &["checkout", "-q", "-b", "feature"]);
        commit("feature work");
        git(&work, &["push", "-q", "origin", "feature"]);
        bare
    }

    #[tokio::test]
    async fn a_clean_merge_moves_the_target_ref_to_a_real_two_parent_commit() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = init_bare_repo_with_diverging_branches(tmp.path());
        let target_tip_before = rev_parse(&bare, "refs/heads/main");
        let executor = GitMergeExecutor::new(tmp.path().to_path_buf());

        let outcome = executor
            .merge("bare.git", &merge_of(MergeMethod::Merge))
            .await
            .unwrap();

        let MergeOutcome::Merged { commit_sha } = outcome else {
            panic!("expected a clean merge")
        };
        let target_tip_after = rev_parse(&bare, "refs/heads/main");
        assert_eq!(
            target_tip_after, commit_sha,
            "the target ref must now point at the new merge commit"
        );
        assert_ne!(target_tip_after, target_tip_before);

        let parents_output = git_stdout(&bare, &["log", "--pretty=%P", "-1", &commit_sha]);
        let parents: Vec<&str> = parents_output.split_whitespace().collect();
        assert_eq!(
            parents.len(),
            2,
            "a merge commit must have exactly two parents"
        );
    }

    #[tokio::test]
    async fn the_merge_commit_carries_a_fixed_server_identity_instead_of_depending_on_git_config() {
        // No global git identity here, like in production: commit-tree used to fail.
        let tmp = tempfile::tempdir().unwrap();
        let bare = init_bare_repo_with_diverging_branches(tmp.path());
        let executor = GitMergeExecutor::new(tmp.path().to_path_buf());

        let outcome = executor
            .merge("bare.git", &merge_of(MergeMethod::Merge))
            .await
            .unwrap();

        let MergeOutcome::Merged { commit_sha } = outcome else {
            panic!("expected a clean merge")
        };
        let log = git_stdout(
            &bare,
            &["log", "--pretty=%an <%ae> / %cn <%ce>", "-1", &commit_sha],
        );
        assert_eq!(
            log,
            "FerrisGit <noreply@ferrisgit.local> / FerrisGit <noreply@ferrisgit.local>"
        );
    }

    #[tokio::test]
    async fn a_conflicting_merge_leaves_the_repository_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let (bare, work) = init_bare_with_clone(tmp.path());
        let run = |args: &[&str]| git(&work, args);
        let commit = |message: &str| {
            git(&work, &["commit", "-q", "-m", message]);
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
            .merge("bare.git", &merge_of(MergeMethod::Merge))
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
        // The race inside merge() can't be reproduced reliably, so test the compare-and-swap directly.
        let tmp = tempfile::tempdir().unwrap();
        let (bare, work) = init_bare_with_clone(tmp.path());
        let run = |args: &[&str]| git(&work, args);
        let commit = |message: &str| {
            git(&work, &["commit", "--allow-empty", "-q", "-m", message]);
        };
        commit("root");
        run(&["push", "-q", "origin", "main"]);
        let stale_tip = rev_parse(&bare, "refs/heads/main");
        commit("a real, later commit that moved main");
        run(&["push", "-q", "origin", "main"]);
        let current_tip = rev_parse(&bare, "refs/heads/main");
        assert_ne!(stale_tip, current_tip);

        let status = git_command(&bare)
            .args(["update-ref", "refs/heads/main", &current_tip, &stale_tip])
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

    #[tokio::test]
    async fn a_merge_ref_holds_the_merge_of_the_two_branches_and_moves_neither() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = init_bare_repo_with_diverging_branches(tmp.path());
        let main_before = rev_parse(&bare, "refs/heads/main");
        let feature_before = rev_parse(&bare, "refs/heads/feature");
        let executor = GitMergeExecutor::new(tmp.path().to_path_buf());

        let merge_ref = executor
            .write_merge_ref(
                "bare.git",
                "feature",
                "main",
                "refs/merge-requests/1/merge",
                "Merge feature into main",
            )
            .await
            .unwrap()
            .expect("the branches merge cleanly");

        assert_eq!(merge_ref.target_sha, main_before);
        assert_eq!(merge_ref.source_sha, feature_before);
        assert_eq!(rev_parse(&bare, "refs/heads/main"), main_before);
        assert_eq!(rev_parse(&bare, "refs/heads/feature"), feature_before);
        assert_eq!(
            rev_parse(&bare, "refs/merge-requests/1/merge"),
            merge_ref.commit_sha
        );
        let parents = git_stdout(&bare, &["log", "--pretty=%P", "-1", &merge_ref.commit_sha]);
        assert_eq!(parents, format!("{main_before} {feature_before}"));
    }

    #[tokio::test]
    async fn a_merge_ref_is_never_written_over_a_branch() {
        let tmp = tempfile::tempdir().unwrap();
        init_bare_repo_with_diverging_branches(tmp.path());
        let executor = GitMergeExecutor::new(tmp.path().to_path_buf());

        for ref_name in ["refs/heads/main", "main"] {
            assert!(matches!(
                executor
                    .write_merge_ref("bare.git", "feature", "main", ref_name, "m")
                    .await,
                Err(DomainError::Validation(_))
            ));
        }
    }

    /// `main` with `base.txt`, then `main` and `feature` each one commit further, touching different files.
    fn init_diverged(dir: &Path) -> (PathBuf, PathBuf) {
        let (bare, work) = init_bare_with_clone(dir);
        let commit_file = |file: &str, content: &str, message: &str, author: &str| {
            std::fs::write(work.join(file), content).unwrap();
            git(&work, &["add", "."]);
            git(
                &work,
                &[
                    "-c",
                    &format!("user.name={author}"),
                    "-c",
                    &format!("user.email={author}@example.com"),
                    "commit",
                    "-q",
                    "-m",
                    message,
                ],
            );
        };
        commit_file("base.txt", "base\n", "base", "root");
        git(&work, &["push", "-q", "origin", "main"]);
        git(&work, &["checkout", "-q", "-b", "feature"]);
        commit_file("a.txt", "a\n", "add a", "alice");
        commit_file("b.txt", "b\n", "add b\n\nwith a body", "bob");
        git(&work, &["push", "-q", "origin", "feature"]);
        git(&work, &["checkout", "-q", "main"]);
        commit_file("main.txt", "main\n", "main moves on", "carol");
        git(&work, &["push", "-q", "origin", "main"]);
        (bare, work)
    }

    fn log(bare: &Path, args: &[&str]) -> Vec<String> {
        let mut full = vec!["log"];
        full.extend(args);
        git_stdout(bare, &full)
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[tokio::test]
    async fn a_squash_lands_the_whole_change_as_one_commit_by_the_merge_request_author() {
        let tmp = tempfile::tempdir().unwrap();
        let (bare, _work) = init_diverged(tmp.path());
        let main_before = rev_parse(&bare, "refs/heads/main");
        let feature_before = rev_parse(&bare, "refs/heads/feature");
        let executor = GitMergeExecutor::new(tmp.path().to_path_buf());

        let outcome = executor
            .merge(
                "bare.git",
                &MergeRequestMerge {
                    message: "Add a and b".to_string(),
                    squash_author: Some(ferrisgit_domain::merge_executor::CommitAuthor {
                        name: "alice".to_string(),
                        email: "alice@example.com".to_string(),
                    }),
                    ..merge_of(MergeMethod::Squash)
                },
            )
            .await
            .unwrap();

        let MergeOutcome::Merged { commit_sha } = outcome else {
            panic!("expected a clean squash")
        };
        assert_eq!(rev_parse(&bare, "refs/heads/main"), commit_sha);
        assert_eq!(
            log(&bare, &["--pretty=%P|%an <%ae>|%cn|%s", "-1", &commit_sha]),
            vec![format!(
                "{main_before}|alice <alice@example.com>|FerrisGit|Add a and b"
            )]
        );
        let files = git_stdout(&bare, &["ls-tree", "--name-only", &commit_sha]);
        assert_eq!(files, "a.txt\nb.txt\nbase.txt\nmain.txt");
        assert_eq!(rev_parse(&bare, "refs/heads/feature"), feature_before);
    }

    #[tokio::test]
    async fn a_rebase_replays_each_commit_on_the_target_keeping_its_author_and_message() {
        let tmp = tempfile::tempdir().unwrap();
        let (bare, _work) = init_diverged(tmp.path());
        let main_before = rev_parse(&bare, "refs/heads/main");
        let feature_before = rev_parse(&bare, "refs/heads/feature");
        let executor = GitMergeExecutor::new(tmp.path().to_path_buf());

        let outcome = executor
            .merge("bare.git", &merge_of(MergeMethod::Rebase))
            .await
            .unwrap();

        let MergeOutcome::Merged { commit_sha } = outcome else {
            panic!("expected a clean rebase")
        };
        assert_eq!(rev_parse(&bare, "refs/heads/main"), commit_sha);
        assert_eq!(
            log(&bare, &["--pretty=%an|%cn|%s", "-4", "main"]),
            vec![
                "bob|FerrisGit|add b",
                "alice|FerrisGit|add a",
                "carol|carol|main moves on",
                "root|root|base",
            ]
        );
        assert_eq!(
            git_stdout(&bare, &["show", "-s", "--format=%B", &commit_sha]),
            "add b\n\nwith a body"
        );
        assert_eq!(
            log(&bare, &["--min-parents=2", "--oneline", "main"]),
            Vec::<String>::new()
        );
        assert_eq!(rev_parse(&bare, "main~2"), main_before);
        assert_eq!(rev_parse(&bare, "refs/heads/feature"), feature_before);
    }

    #[tokio::test]
    async fn a_rebase_onto_a_target_the_source_already_contains_is_a_fast_forward() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = init_bare_repo_with_diverging_branches(tmp.path());
        let feature = rev_parse(&bare, "refs/heads/feature");
        let executor = GitMergeExecutor::new(tmp.path().to_path_buf());

        let outcome = executor
            .merge("bare.git", &merge_of(MergeMethod::Rebase))
            .await
            .unwrap();

        assert_eq!(
            outcome,
            MergeOutcome::Merged {
                commit_sha: feature.clone()
            }
        );
        assert_eq!(rev_parse(&bare, "refs/heads/main"), feature);
    }

    #[tokio::test]
    async fn a_branch_holding_a_merge_commit_is_not_rebased() {
        let tmp = tempfile::tempdir().unwrap();
        let (bare, work) = init_diverged(tmp.path());
        git(&work, &["checkout", "-q", "feature"]);
        git(&work, &["merge", "-q", "--no-edit", "main"]);
        git(&work, &["checkout", "-q", "-b", "side", "main"]);
        std::fs::write(work.join("side.txt"), "side\n").unwrap();
        git(&work, &["add", "."]);
        git(&work, &["commit", "-q", "-m", "side"]);
        git(&work, &["push", "-q", "origin", "side:main", "feature"]);
        let main_before = rev_parse(&bare, "refs/heads/main");
        let executor = GitMergeExecutor::new(tmp.path().to_path_buf());

        let result = executor
            .merge("bare.git", &merge_of(MergeMethod::Rebase))
            .await;

        assert!(
            matches!(&result, Err(DomainError::Validation(m)) if m.contains("merge commits")),
            "{result:?}"
        );
        assert_eq!(rev_parse(&bare, "refs/heads/main"), main_before);
    }

    #[tokio::test]
    async fn a_conflicting_squash_or_rebase_leaves_the_target_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let (bare, work) = init_diverged(tmp.path());
        std::fs::write(work.join("a.txt"), "main's own a\n").unwrap();
        git(&work, &["add", "."]);
        git(&work, &["commit", "-q", "-m", "main adds a too"]);
        git(&work, &["push", "-q", "origin", "main"]);
        let main_before = rev_parse(&bare, "refs/heads/main");
        let executor = GitMergeExecutor::new(tmp.path().to_path_buf());

        for method in [MergeMethod::Squash, MergeMethod::Rebase] {
            let outcome = executor.merge("bare.git", &merge_of(method)).await.unwrap();
            assert_eq!(outcome, MergeOutcome::Conflicting, "{method:?}");
            assert_eq!(rev_parse(&bare, "refs/heads/main"), main_before);
        }
    }
}
