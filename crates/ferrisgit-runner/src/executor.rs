use std::collections::BTreeMap;
use std::path::Path;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use uuid::Uuid;

use crate::client::{ClaimedJob, JobResultStatus, RunnerClient, build_clone_url};

/// Shared by a job and its Docker daemon: the daemon's socket, its `docker` client in `bin`, the client's configuration
/// and plugins (buildx, compose) in `config`.
const DOCKER_DIR: &str = "/ferrisgit-docker";

/// The job's Docker daemon, when it has one: the container and the volume it shares with the job.
pub struct JobDocker {
    pub container: String,
    pub volume: String,
}

impl JobDocker {
    pub fn for_job(job_id: Uuid) -> Self {
        Self {
            container: format!("ferrisgit-job-{job_id}-docker"),
            volume: format!("ferrisgit-job-{job_id}-docker"),
        }
    }
}

/// `docker run` of the job. With a daemon, the job reaches it through the shared volume's socket, and finds the copied
/// client last on its `PATH`, so an image that has its own keeps it.
pub fn build_docker_run_args(
    image: &str,
    workdir: &Path,
    env: &BTreeMap<String, String>,
    script: &[String],
    docker: Option<&JobDocker>,
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
    let mut command = script.join(" && ");
    if let Some(docker) = docker {
        args.extend([
            "-v".to_string(),
            format!("{}:{DOCKER_DIR}", docker.volume),
            "-e".to_string(),
            format!("DOCKER_HOST=unix://{DOCKER_DIR}/docker.sock"),
            "-e".to_string(),
            format!("DOCKER_CONFIG={DOCKER_DIR}/config"),
        ]);
        command = format!("export PATH=\"$PATH:{DOCKER_DIR}/bin\" && {command}");
    }
    args.push(image.to_string());
    args.push("sh".to_string());
    args.push("-c".to_string());
    args.push(command);
    args
}

/// `docker run` of the job's daemon: Docker-in-Docker, privileged, listening on the shared volume's socket only, never on
/// the network. It copies its client into the volume first.
pub fn build_docker_daemon_args(daemon_image: &str, docker: &JobDocker) -> Vec<String> {
    vec![
        "run".to_string(),
        "-d".to_string(),
        "--privileged".to_string(),
        "--name".to_string(),
        docker.container.clone(),
        "-e".to_string(),
        "DOCKER_TLS_CERTDIR=".to_string(),
        "-v".to_string(),
        format!("{}:{DOCKER_DIR}", docker.volume),
        "--entrypoint".to_string(),
        "sh".to_string(),
        daemon_image.to_string(),
        "-c".to_string(),
        // The same as the Kubernetes engine's (job_containers.rs, `docker_daemon_command`).
        format!(
            "mkdir -p {DOCKER_DIR}/bin {DOCKER_DIR}/config/cli-plugins && cp /usr/local/bin/docker {DOCKER_DIR}/bin/docker && cp /usr/local/libexec/docker/cli-plugins/* {DOCKER_DIR}/config/cli-plugins/ && exec dockerd-entrypoint.sh dockerd --host=unix://{DOCKER_DIR}/docker.sock"
        ),
    ]
}

/// Removes `.git` from a fresh clone. The clone URL holds the runner token and `git clone` writes it to `.git/config`.
/// The workdir is bind-mounted into the job container, so a script step could read it and get read access to every repo
/// plus the ability to claim jobs. Steps only need the working tree.
pub fn strip_git_metadata(workdir: &Path) -> std::io::Result<()> {
    let git_dir = workdir.join(".git");
    match std::fs::remove_dir_all(&git_dir) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

/// Replaces every occurrence of the secrets in `line` with `***`. Covers the clone token, which a failing `git clone`
/// echoes to stderr, and the masked CI variables a step may print.
fn redact_secrets(line: &str, secrets: &[&str]) -> String {
    let mut redacted = line.to_string();
    for secret in secrets {
        if !secret.is_empty() {
            redacted = redacted.replace(secret, "***");
        }
    }
    redacted
}

/// Sends each line of `output` to the job's log endpoint as it comes, secrets redacted.
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

/// Streams combined stdout and stderr line by line to the job's log endpoint instead of buffering until exit.
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

/// Whether the repository in `workdir` holds `commit_sha`, asked quietly so a miss leaves nothing in the job log.
async fn has_commit(workdir: &Path, commit_sha: &str) -> bool {
    Command::new("git")
        .args(["cat-file", "-e", &format!("{commit_sha}^{{commit}}")])
        .current_dir(workdir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .is_ok_and(|status| status.success())
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
    if !run_streamed(client, job.id, "git", &clone, workdir, &secrets).await {
        return JobResultStatus::Failed;
    }
    // A merge request pipeline runs on the merge of two branches, a commit no branch points to: the clone doesn't
    // have it, so it is fetched by its id.
    if !has_commit(workdir, &job.commit_sha).await {
        let fetch = [
            "fetch".to_string(),
            "-q".to_string(),
            "origin".to_string(),
            job.commit_sha.clone(),
        ];
        if !run_streamed(client, job.id, "git", &fetch, workdir, &secrets).await {
            return JobResultStatus::Failed;
        }
    }
    let checkout = ["checkout".to_string(), job.commit_sha.clone()];
    if !run_streamed(client, job.id, "git", &checkout, workdir, &secrets).await {
        return JobResultStatus::Failed;
    }

    // Has to happen before the bind mount, `.git/config` holds the runner token.
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
    let docker = match &job.docker_image {
        Some(image) => {
            let docker = JobDocker::for_job(job.id);
            if !start_docker_daemon(client, job.id, image, &docker, workdir).await {
                stop_docker_daemon(&docker, workdir).await;
                return JobResultStatus::Failed;
            }
            Some(docker)
        }
        None => None,
    };
    let docker_args =
        build_docker_run_args(&job.image, workdir, &env, &job.script, docker.as_ref());
    let succeeded = run_streamed(client, job.id, "docker", &docker_args, workdir, &secrets).await;
    if let Some(docker) = &docker {
        stop_docker_daemon(docker, workdir).await;
    }
    if succeeded {
        JobResultStatus::Success
    } else {
        JobResultStatus::Failed
    }
}

/// Starts the job's daemon and waits until it answers, two minutes at most; says in the job's log when it does not.
async fn start_docker_daemon(
    client: &RunnerClient,
    job_id: Uuid,
    image: &str,
    docker: &JobDocker,
    workdir: &Path,
) -> bool {
    let started = Command::new("docker")
        .args(build_docker_daemon_args(image, docker))
        .current_dir(workdir)
        .stdout(Stdio::null())
        .output()
        .await;
    match started {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            let _ = client
                .append_logs(
                    job_id,
                    &format!(
                        "failed to start the job's Docker daemon: {}\n",
                        String::from_utf8_lossy(&output.stderr).trim()
                    ),
                )
                .await;
            return false;
        }
        Err(err) => {
            let _ = client
                .append_logs(job_id, &format!("failed to start docker: {err}\n"))
                .await;
            return false;
        }
    }
    let socket = format!("--host=unix://{DOCKER_DIR}/docker.sock");
    for _ in 0..120 {
        let ready = Command::new("docker")
            .args(["exec", &docker.container, "docker", &socket, "info"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .is_ok_and(|status| status.success());
        if ready {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    let _ = client
        .append_logs(
            job_id,
            "the job's Docker daemon did not answer within two minutes\n",
        )
        .await;
    false
}

/// Removes the daemon, its images and layers, and the shared volume, whatever the job did.
async fn stop_docker_daemon(docker: &JobDocker, workdir: &Path) {
    for args in [
        vec!["rm", "--force", "--volumes", docker.container.as_str()],
        vec!["volume", "rm", "--force", docker.volume.as_str()],
    ] {
        let _ = Command::new("docker")
            .args(args)
            .current_dir(workdir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
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

        let args = build_docker_run_args("rust:1.82", Path::new("/tmp/job-1"), &env, &script, None);

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
            None,
        );

        assert!(!args.contains(&"-e".to_string()));
        assert_eq!(args.last().unwrap(), "echo hi");
    }

    #[test]
    fn with_a_daemon_the_job_shares_its_socket_and_finds_its_client_last_on_the_path() {
        let docker = JobDocker::for_job(Uuid::nil());

        let args = build_docker_run_args(
            "rust:1",
            Path::new("/tmp/job-3"),
            &BTreeMap::new(),
            &["docker build .".to_string()],
            Some(&docker),
        );

        let volume = format!("{}:/ferrisgit-docker", docker.volume);
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "-v" && pair[1] == volume)
        );
        assert!(
            args.windows(2).any(|pair| pair[0] == "-e"
                && pair[1] == "DOCKER_HOST=unix:///ferrisgit-docker/docker.sock")
        );
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "-e" && pair[1] == "DOCKER_CONFIG=/ferrisgit-docker/config")
        );
        assert!(
            !args.contains(&"--privileged".to_string()),
            "the job itself is not privileged"
        );
        assert_eq!(
            args.last().unwrap(),
            "export PATH=\"$PATH:/ferrisgit-docker/bin\" && docker build ."
        );
    }

    #[test]
    fn the_daemon_is_privileged_and_listens_on_the_shared_socket_only() {
        let docker = JobDocker::for_job(Uuid::nil());

        let args = build_docker_daemon_args("docker:29-dind", &docker);

        assert!(args.contains(&"--privileged".to_string()));
        assert!(args.contains(&"DOCKER_TLS_CERTDIR=".to_string()));
        let command = args.last().unwrap();
        assert!(command.ends_with("dockerd --host=unix:///ferrisgit-docker/docker.sock"));
        assert!(!command.contains("tcp://"), "no network listener");
        assert!(!args.iter().any(|arg| arg == "-p" || arg == "--publish"));
    }

    /// Runs real `git` against a real temp repository.
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

        // `file://user:token@/path` isn't a valid remote, so clone a plain path and rewrite the remote URL to the credentialed
        // one `build_clone_url` would produce.
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
