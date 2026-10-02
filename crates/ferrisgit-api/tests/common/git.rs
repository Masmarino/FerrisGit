//! Git subprocesses for the tests, on a blocking thread: the server runs in-process on the same runtime, so a blocking
//! `git` call would starve the clone or push it's waiting on.

use std::path::Path;
use std::process::Command;

/// Runs `git <args>` in `cwd`, asserts it succeeded and returns its trimmed stdout.
pub async fn git(args: &[&str], cwd: &Path) -> String {
    let args_for_message = args.to_vec();
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let cwd = cwd.to_path_buf();
    let output = tokio::task::spawn_blocking(move || {
        Command::new("git").args(&args).current_dir(&cwd).output()
    })
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "git {args_for_message:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Writes `file`, stages everything and commits it with a throwaway identity.
pub async fn commit_file(repo_path: &Path, file: &str, content: &str, message: &str) {
    std::fs::write(repo_path.join(file), content).unwrap();
    git(&["add", "."], repo_path).await;
    git(
        &[
            "-c",
            "user.email=t@t.com",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            message,
        ],
        repo_path,
    )
    .await;
}

/// Builds a repository holding `files` in a fresh directory, commits them and pushes HEAD to `branch` of
/// `remote_url`. Returns whether the push succeeded, so a test can assert a refusal as well as a success.
pub async fn push_files(
    remote_url: &str,
    branch: &str,
    message: &str,
    files: &[(&str, &[u8])],
) -> bool {
    let work_dir = tempfile::tempdir().unwrap();
    let dir = work_dir.path();
    git(&["init", "-q"], dir).await;
    for (path, content) in files {
        let full_path = dir.join(path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&full_path, content).unwrap();
    }
    git(&["add", "."], dir).await;
    git(
        &[
            "-c",
            "user.email=a@b.c",
            "-c",
            "user.name=A",
            "commit",
            "-q",
            "-m",
            message,
        ],
        dir,
    )
    .await;
    git(&["remote", "add", "origin", remote_url], dir).await;
    let (remote, refspec) = (remote_url.to_string(), format!("HEAD:{branch}"));
    let dir = dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        Command::new("git")
            .args(["push", "-q", "origin", &refspec])
            .current_dir(&dir)
            .status()
            .map(|status| status.success())
            .unwrap_or_else(|error| panic!("git push to {remote} did not run: {error}"))
    })
    .await
    .unwrap()
}
