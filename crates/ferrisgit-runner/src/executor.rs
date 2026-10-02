use std::collections::BTreeMap;
use std::path::Path;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use uuid::Uuid;

use crate::client::{ClaimedJob, JobResultStatus, RunnerClient, build_clone_url};

pub fn build_docker_run_args(
    image: &str,
    workdir: &Path,
    env: &BTreeMap<String, String>,
    script: &[String],
) -> Vec<String> {
    let mut args = vec![
        "run".to_string(),
        "--rm".to_string(),
        "-v".to_string(),
        format!("{}:/workspace", workdir.display()),
        "-w".to_string(),
        "/workspace".to_string(),
    ];
    for (key, value) in env {
        args.push("-e".to_string());
        args.push(format!("{key}={value}"));
    }
    args.push(image.to_string());
    args.push("sh".to_string());
    args.push("-c".to_string());
    args.push(script.join(" && "));
    args
}

/// Removes `.git` from a freshly cloned workdir.
///
/// The clone URL carries the runner's token as userinfo and `git clone` persists it in `.git/config`. The workdir is
/// bind-mounted into the job container, so a `script:` step could read that token, which grants read access to every
/// repository and the ability to claim jobs. Steps only need the working tree, so dropping `.git` costs nothing.
pub fn strip_git_metadata(workdir: &Path) -> std::io::Result<()> {
    let git_dir = workdir.join(".git");
    match std::fs::remove_dir_all(&git_dir) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

/// Replaces every occurrence of any of `secrets` in `line` with `***`.
///
/// This covers the clone URL's token (a failing `git clone` echoes it to stderr) and the `masked` CI variable values a
/// step may print. Otherwise both would reach the job's log stream.
fn redact_secrets(line: &str, secrets: &[&str]) -> String {
    let mut redacted = line.to_string();
    for secret in secrets {
        if !secret.is_empty() {
            redacted = redacted.replace(secret, "***");
        }
    }
    redacted
}

/// Sends each line of `output` to the job's log endpoint as it is produced, secrets redacted.
async fn forward_lines(
    output: impl AsyncRead + Unpin,
    client: &RunnerClient,
    job_id: Uuid,
    secrets: &[&str],
) {
    let mut lines = BufReader::new(output).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let redacted = redact_secrets(&line, secrets);
        let _ = client.append_logs(job_id, &format!("{redacted}\n")).await;
    }
}

/// Streams each line of combined stdout+stderr to the job's log endpoint as produced, not buffered until exit.
async fn run_streamed(
    client: &RunnerClient,
    job_id: Uuid,
    program: &str,
    args: &[String],
    cwd: &Path,
    secrets: &[&str],
) -> bool {
    let mut child = match Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            let _ = client
                .append_logs(job_id, &format!("failed to start {program}: {err}\n"))
                .await;
            return false;
        }
    };

    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");
    tokio::join!(
        forward_lines(stdout, client, job_id, secrets),
        forward_lines(stderr, client, job_id, secrets),
    );

    matches!(child.wait().await, Ok(status) if status.success())
}

pub async fn run_job(
    client: &RunnerClient,
    job: &ClaimedJob,
    workdir_root: &Path,
) -> JobResultStatus {
    let workdir = workdir_root.join(job.id.to_string());
    if let Err(err) = std::fs::create_dir_all(&workdir) {
        let _ = client
            .append_logs(job.id, &format!("failed to create workdir: {err}\n"))
            .await;
        return JobResultStatus::Failed;
    }

    let status = run_in_workdir(client, job, &workdir).await;
    let _ = std::fs::remove_dir_all(&workdir);
    status
}

async fn run_in_workdir(
    client: &RunnerClient,
    job: &ClaimedJob,
    workdir: &Path,
) -> JobResultStatus {
    let mut secrets: Vec<&str> = vec![&client.token];
    secrets.extend(job.masked_values.iter().map(String::as_str));

    let clone_url = build_clone_url(
        &client.server_url,
        &client.token,
        &job.repository_owner,
        &job.repository_name,
    );
    let clone = ["clone".to_string(), clone_url, ".".to_string()];
    let checkout = ["checkout".to_string(), job.commit_sha.clone()];
    for git_args in [clone.as_slice(), checkout.as_slice()] {
        if !run_streamed(client, job.id, "git", git_args, workdir, &secrets).await {
            return JobResultStatus::Failed;
        }
    }

    // Must run before the workdir is bind-mounted: `.git/config` holds the runner token.
    if let Err(err) = strip_git_metadata(workdir) {
        let _ = client
            .append_logs(
                job.id,
                &format!("failed to strip git metadata from the workdir: {err}\n"),
            )
            .await;
        return JobResultStatus::Failed;
    }

    let mut env = job.variables.clone();
    env.extend(job.ci_variables.clone());
    let docker_args = build_docker_run_args(&job.image, workdir, &env, &job.script);
    if run_streamed(client, job.id, "docker", &docker_args, workdir, &secrets).await {
        JobResultStatus::Success
    } else {
        JobResultStatus::Failed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_docker_run_args_mounts_the_workdir_and_chains_the_script_with_and() {
        let mut env = BTreeMap::new();
        env.insert("RUST_LOG".to_string(), "debug".to_string());
        let script = vec!["cargo build".to_string(), "cargo test".to_string()];

        let args = build_docker_run_args("rust:1.82", Path::new("/tmp/job-1"), &env, &script);

        assert_eq!(
            args,
            vec![
                "run",
                "--rm",
                "-v",
                "/tmp/job-1:/workspace",
                "-w",
                "/workspace",
                "-e",
                "RUST_LOG=debug",
                "rust:1.82",
                "sh",
                "-c",
                "cargo build && cargo test",
            ]
        );
    }

    #[test]
    fn build_docker_run_args_with_no_env_vars_omits_any_dash_e_flags() {
        let args = build_docker_run_args(
            "alpine",
            Path::new("/tmp/job-2"),
            &BTreeMap::new(),
            &["echo hi".to_string()],
        );

        assert!(!args.contains(&"-e".to_string()));
        assert_eq!(args.last().unwrap(), "echo hi");
    }

    /// Real `git` subprocesses against a real temp repository.
    fn git(cwd: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .status()
            .expect("failed to run git");
        assert!(status.success(), "git {args:?} failed in {}", cwd.display());
    }

    #[test]
    fn stripping_git_metadata_removes_the_credential_bearing_clone_url_from_the_workdir() {
        let tmp = tempfile::tempdir().unwrap();
        let origin = tmp.path().join("origin.git");
        std::fs::create_dir_all(&origin).unwrap();
        git(
            tmp.path(),
            &[
                "init",
                "--bare",
                "-q",
                "-b",
                "main",
                origin.to_str().unwrap(),
            ],
        );

        let seed = tmp.path().join("seed");
        std::fs::create_dir_all(&seed).unwrap();
        git(&seed, &["init", "-q"]);
        std::fs::write(seed.join("README.md"), "hello\n").unwrap();
        git(&seed, &["add", "."]);
        git(
            &seed,
            &[
                "-c",
                "user.email=t@example.com",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "-m",
                "seed",
            ],
        );
        git(
            &seed,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        git(&seed, &["push", "-q", "origin", "HEAD:refs/heads/main"]);

        // Credentials in the userinfo, as `build_clone_url` produces: `file://user:token@/path` is not a valid
        // remote, so clone a plain path and rewrite the persisted remote URL.
        let workdir = tmp.path().join("workdir");
        std::fs::create_dir_all(&workdir).unwrap();
        git(
            tmp.path(),
            &[
                "clone",
                "-q",
                origin.to_str().unwrap(),
                workdir.to_str().unwrap(),
            ],
        );
        let clone_url = crate::client::build_clone_url(
            "http://localhost:8080",
            "fgr_supersecret",
            "florian",
            "hello",
        );
        git(&workdir, &["remote", "set-url", "origin", &clone_url]);

        let config_before = std::fs::read_to_string(workdir.join(".git").join("config")).unwrap();
        assert!(
            config_before.contains("fgr_supersecret"),
            "precondition: the runner token must really be persisted in .git/config"
        );

        strip_git_metadata(&workdir).unwrap();

        assert!(
            !workdir.join(".git").exists(),
            ".git must be gone before the workdir is bind-mounted into the job container"
        );
        assert!(
            workdir.join("README.md").exists(),
            "the checked-out working tree the job actually needs must survive"
        );
        let mut remaining = Vec::new();
        for entry in walk(&workdir) {
            if std::fs::read_to_string(&entry).is_ok_and(|text| text.contains("fgr_supersecret")) {
                remaining.push(entry);
            }
        }
        assert!(
            remaining.is_empty(),
            "the runner token is still readable from the job workdir at: {remaining:?}"
        );
    }

    fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk(&path));
            } else {
                out.push(path);
            }
        }
        out
    }

    #[test]
    fn stripping_git_metadata_is_a_no_op_when_there_is_no_dot_git_directory() {
        let tmp = tempfile::tempdir().unwrap();
        strip_git_metadata(tmp.path()).expect("a workdir with no .git must not be an error");
    }

    #[test]
    fn redact_secrets_replaces_every_occurrence_of_a_secret() {
        let line =
            "fatal: could not read Username for 'http://runner:fgr_supersecret@host/o/r.git'";

        let redacted = redact_secrets(line, &["fgr_supersecret"]);

        assert!(!redacted.contains("fgr_supersecret"));
        assert_eq!(
            redacted,
            "fatal: could not read Username for 'http://runner:***@host/o/r.git'"
        );
    }

    #[test]
    fn redact_secrets_is_a_no_op_when_no_secret_appears_in_the_line() {
        let line = "Cloning into '.'...";

        assert_eq!(redact_secrets(line, &["fgr_supersecret"]), line);
    }

    #[test]
    fn redact_secrets_skips_empty_secrets() {
        let line = "some ordinary log output";

        assert_eq!(redact_secrets(line, &[""]), line);
    }

    #[test]
    fn redact_secrets_redacts_every_secret_in_the_list_independently() {
        let line = "token=fgr_supersecret masked_var=super-secret-db-password";

        let redacted = redact_secrets(line, &["fgr_supersecret", "super-secret-db-password"]);

        assert_eq!(redacted, "token=*** masked_var=***");
    }
}
