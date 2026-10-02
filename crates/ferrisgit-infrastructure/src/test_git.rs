//! Helpers for tests that build real repositories with the `git` binary.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A `git` command in `dir` with a fixed identity, so commits work without a global config. A test that
/// cares about authorship passes its own `-c` flags, which come later on the command line and win.
pub(crate) fn git_command(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .args(["-c", "user.email=t@t.com", "-c", "user.name=t"])
        .current_dir(dir);
    command
}

/// Runs `git` in `dir` and panics unless it succeeds.
pub(crate) fn git(dir: &Path, args: &[&str]) {
    let mut command = git_command(dir);
    command.args(args);
    run_checked(command);
}

pub(crate) fn run_checked(mut command: Command) {
    let status = command.status().unwrap();
    assert!(status.success(), "{command:?} failed");
}

/// Runs `git` in `dir`, panics unless it succeeds, and returns its trimmed stdout.
pub(crate) fn git_stdout(dir: &Path, args: &[&str]) -> String {
    let mut command = git_command(dir);
    command.args(args);
    let output = command.output().unwrap();
    assert!(output.status.success(), "{command:?} failed");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Like `git_stdout`, but keeps the output exactly as git wrote it (file contents end in a newline).
pub(crate) fn git_stdout_untrimmed(dir: &Path, args: &[&str]) -> String {
    let mut command = git_command(dir);
    command.args(args);
    let output = command.output().unwrap();
    assert!(output.status.success(), "{command:?} failed");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// An empty bare repository `bare.git` (default branch `main`) and a clone of it in `work`, both under
/// `dir`: history is built in the checkout and pushed, the way the server's bare repositories get theirs.
pub(crate) fn init_bare_with_clone(dir: &Path) -> (PathBuf, PathBuf) {
    let bare = dir.join("bare.git");
    let work = dir.join("work");
    let mut init = Command::new("git");
    init.args(["init", "--bare", "-q", "-b", "main"]).arg(&bare);
    run_checked(init);
    let mut clone = Command::new("git");
    clone.args(["clone", "-q"]).arg(&bare).arg(&work);
    run_checked(clone);
    (bare, work)
}

pub(crate) fn rev_parse(dir: &Path, spec: &str) -> String {
    git_stdout(dir, &["rev-parse", spec])
}

/// Whether `git` exits successfully in `dir`; a failing exit status is an answer here, not a panic.
pub(crate) fn git_succeeds(dir: &Path, args: &[&str]) -> bool {
    git_command(dir).args(args).status().unwrap().success()
}

/// Commits one file in a throwaway checkout and pushes it to `branch` of the bare repository, the way
/// a user's `git push` would, bypassing the server's own write paths.
pub(crate) fn push_one_commit(repo_path: &Path, branch: &str, file_name: &str, content: &str) {
    let work_dir = tempfile::tempdir().unwrap();
    git(work_dir.path(), &["init", "-q"]);
    std::fs::write(work_dir.path().join(file_name), content).unwrap();
    git(work_dir.path(), &["add", "."]);
    git(work_dir.path(), &["commit", "-q", "-m", "seed"]);
    let target = format!("HEAD:refs/heads/{branch}");
    git(
        work_dir.path(),
        &["push", "-q", repo_path.to_str().unwrap(), &target],
    );
}
