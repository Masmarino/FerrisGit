use async_trait::async_trait;
use uuid::Uuid;

use crate::error::DomainError;

/// The Docker daemon a job gets when the admin allows Docker in jobs: Docker-in-Docker, which needs a privileged
/// container. Pinned by digest, like every image FerrisGit runs itself. Its static `docker` client is copied for the job,
/// so `docker` works whatever the job's image.
pub const DOCKER_DAEMON_IMAGE: &str =
    "docker:29-dind@sha256:0b6d18a4a222e71c88be6034926b4f6cde2fc33c2218643d03ccfddc65995470";

/// What a Kubernetes job clones its repository with, before the job's own image starts.
pub const CLONE_IMAGE: &str =
    "alpine/git:v2.54.0@sha256:a4bb51f1a3553df194ce679fc1db721d8bfba2046fa1a88fe9d4ac551ffbce25";

/// Shared by the job and its Docker daemon: the daemon's socket, the `docker` client in `bin`, and in `config` the
/// client's configuration with its buildx and compose plugins.
pub const DOCKER_DIR: &str = "/ferrisgit-docker";

/// What the job's daemon runs: it copies its client and plugins for the job, then listens on the shared socket only,
/// never on the network.
pub fn docker_daemon_command() -> String {
    format!(
        "mkdir -p {DOCKER_DIR}/bin {DOCKER_DIR}/config/cli-plugins && cp /usr/local/bin/docker {DOCKER_DIR}/bin/docker && cp /usr/local/libexec/docker/cli-plugins/* {DOCKER_DIR}/config/cli-plugins/ && exec dockerd-entrypoint.sh dockerd --host=unix://{DOCKER_DIR}/docker.sock"
    )
}

/// The environment a job gets beside its daemon: where the daemon is, and the client's configuration.
pub fn docker_job_env() -> [(&'static str, String); 2] {
    [
        ("DOCKER_HOST", format!("unix://{DOCKER_DIR}/docker.sock")),
        ("DOCKER_CONFIG", format!("{DOCKER_DIR}/config")),
    ]
}

/// Where a job finds its repository, and runs from.
pub const WORKSPACE_DIR: &str = "/workspace";

/// The script a job runs: its lines joined with `&&`, so the first that fails fails the job. With Docker, the copied
/// client comes last on the `PATH`, so an image that has its own keeps it.
pub fn job_shell_script(script: &[String], docker: bool) -> String {
    let lines = script.join(" && ");
    if docker {
        format!("export PATH=\"$PATH:{DOCKER_DIR}/bin\" && {lines}")
    } else {
        lines
    }
}

/// The admin switch for Docker in jobs. Off until an admin turns it on.
#[async_trait]
pub trait DockerInJobsPort: Send + Sync {
    async fn is_enabled(&self) -> Result<bool, DomainError>;
    async fn set_enabled(&self, enabled: bool) -> Result<(), DomainError>;
}

/// The token a Kubernetes job clones its repository with. Only its hash is stored.
#[async_trait]
pub trait JobCloneTokenPort: Send + Sync {
    async fn issue(&self, job_id: Uuid, token_hash: &str) -> Result<(), DomainError>;
    /// The repository a token reads, while its job is pending or running; `None` for an unknown token or a finished job.
    async fn repository_of_unfinished_job(
        &self,
        token_hash: &str,
    ) -> Result<Option<Uuid>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_runs_its_lines_in_turn_and_with_docker_finds_the_copied_client_last() {
        let script = vec!["cargo build".to_string(), "cargo test".to_string()];

        assert_eq!(
            job_shell_script(&script, false),
            "cargo build && cargo test"
        );
        assert_eq!(
            job_shell_script(&script, true),
            "export PATH=\"$PATH:/ferrisgit-docker/bin\" && cargo build && cargo test"
        );
    }
}
