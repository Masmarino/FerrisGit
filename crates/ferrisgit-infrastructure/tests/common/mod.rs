//! Fakes and seeding shared by the `kind`-backed integration tests.

use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::pipeline_events::{JobEvent, PipelineEvent, PipelineEventPublisherPort};
use ferrisgit_domain::pipeline_file_reader::PipelineFileReaderPort;
use ferrisgit_domain::settings::{RepositorySettings, RepositorySettingsStorePort};
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use ferrisgit_domain::webhook_event::WebhookEvent;
use uuid::Uuid;

pub struct FakeRepositorySettings;
#[async_trait]
impl RepositorySettingsStorePort for FakeRepositorySettings {
    async fn get_or_create_default(
        &self,
        repository_id: Uuid,
    ) -> Result<RepositorySettings, DomainError> {
        Ok(RepositorySettings {
            repository_id,
            pipeline_file_path: ".ferrisgit-ci.yml".to_string(),
            ci_enabled: true,
            required_approvals: 0,
            pipeline_must_succeed: false,
            default_merge_method: Default::default(),
        })
    }
    async fn update(
        &self,
        _repository_id: Uuid,
        _update: ferrisgit_domain::settings::RepositorySettingsUpdate,
    ) -> Result<RepositorySettings, DomainError> {
        unimplemented!()
    }
    async fn list_ci_variables(
        &self,
        _repository_id: Uuid,
    ) -> Result<Vec<ferrisgit_domain::settings::CiVariable>, DomainError> {
        unimplemented!()
    }
    async fn set_ci_variable(
        &self,
        _new_variable: ferrisgit_domain::settings::NewCiVariable,
    ) -> Result<ferrisgit_domain::settings::CiVariable, DomainError> {
        unimplemented!()
    }
    async fn delete_ci_variable(&self, _id: Uuid, _repository_id: Uuid) -> Result<(), DomainError> {
        unimplemented!()
    }
    async fn resolve_ci_variables_plaintext(
        &self,
        _repository_id: Uuid,
    ) -> Result<std::collections::BTreeMap<String, String>, DomainError> {
        unimplemented!()
    }
}

pub struct FakeFileReader(pub String);
#[async_trait]
impl PipelineFileReaderPort for FakeFileReader {
    async fn read_file_at_revision(
        &self,
        _repository_disk_path: &str,
        _commit_sha: &str,
        _path: &str,
    ) -> Result<Option<Vec<u8>>, DomainError> {
        Ok(Some(self.0.clone().into_bytes()))
    }
}

#[derive(Default)]
pub struct FakeEvents;
#[async_trait]
impl PipelineEventPublisherPort for FakeEvents {
    async fn publish_pipeline_event(
        &self,
        _pipeline_id: Uuid,
        _event: PipelineEvent,
    ) -> Result<(), DomainError> {
        Ok(())
    }
    async fn publish_job_event(&self, _job_id: Uuid, _event: JobEvent) -> Result<(), DomainError> {
        Ok(())
    }
}

pub struct FakeWebhooks;
#[async_trait]
impl WebhookDispatcherPort for FakeWebhooks {
    async fn dispatch(
        &self,
        _repository_id: Uuid,
        _event: WebhookEvent,
    ) -> Result<(), DomainError> {
        Ok(())
    }
}

/// Inserts a user and a repository directly: the foreign keys are plumbing these tests aren't proving.
/// Returns `(repository_id, owner_id)`.
pub async fn insert_repository(pool: &sqlx::PgPool) -> (Uuid, Uuid) {
    let repository_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, username, email, password_hash) VALUES ($1, $2, $3, $4)")
        .bind(owner_id)
        .bind(format!("owner-{owner_id}"))
        .bind(format!("owner-{owner_id}@example.com"))
        .bind("not-a-real-hash")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO repositories (id, owner_id, name, disk_path) VALUES ($1, $2, $3, $4)")
        .bind(repository_id)
        .bind(owner_id)
        .bind(format!("repo-{repository_id}"))
        .bind("unused-disk-path")
        .execute(pool)
        .await
        .unwrap();
    (repository_id, owner_id)
}

/// A repository a kind Pod can clone: a bare repository with one commit, served over git's "dumb" HTTP protocol by
/// `python3` on every interface. Pods reach it at host.docker.internal (Docker Desktop). Stopped on drop.
pub struct GitServer {
    pub url: String,
    pub commit: String,
    child: std::process::Child,
    _root: tempfile::TempDir,
}

impl Drop for GitServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn git(cwd: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Serves `acme/widget.git`, whose one commit holds `files`.
pub fn serve_repository(files: &[(&str, &str)]) -> GitServer {
    let root = tempfile::tempdir().unwrap();
    let work = root.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    git(&work, &["init", "-q"]);
    for (path, content) in files {
        std::fs::write(work.join(path), content).unwrap();
    }
    git(&work, &["add", "."]);
    git(
        &work,
        &[
            "-c",
            "user.email=t@t.t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "seed",
        ],
    );
    let commit = git(&work, &["rev-parse", "HEAD"]);
    let served = root.path().join("served");
    std::fs::create_dir_all(served.join("acme")).unwrap();
    git(
        root.path(),
        &[
            "clone",
            "-q",
            "--bare",
            work.to_str().unwrap(),
            served.join("acme/widget.git").to_str().unwrap(),
        ],
    );
    git(&served.join("acme/widget.git"), &["update-server-info"]);

    let port = std::net::TcpListener::bind("0.0.0.0:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let child = std::process::Command::new("python3")
        .args([
            "-m",
            "http.server",
            &port.to_string(),
            "--bind",
            "0.0.0.0",
            "--directory",
        ])
        .arg(&served)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("python3 serves the test repository");
    for _ in 0..50 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    GitServer {
        url: format!("http://host.docker.internal:{port}"),
        commit,
        child,
        _root: root,
    }
}

/// The job environment of the served repository: no CI variable, Docker as asked.
pub struct ServedEnvironments {
    pub repository_id: Uuid,
    pub commit: String,
    pub docker: bool,
}

#[async_trait]
impl ferrisgit_application::use_cases::job_environment::JobEnvironmentSource
    for ServedEnvironments
{
    async fn environment(
        &self,
        _job: &ferrisgit_domain::job::Job,
    ) -> Result<ferrisgit_application::use_cases::job_environment::JobEnvironment, DomainError>
    {
        Ok(
            ferrisgit_application::use_cases::job_environment::JobEnvironment {
                repository_id: self.repository_id,
                repository_path: "acme/widget".to_string(),
                commit_sha: self.commit.clone(),
                ci_variables: Default::default(),
                masked_values: vec![],
                docker: self.docker,
            },
        )
    }

    async fn issue_clone_token(&self, job_id: Uuid) -> Result<String, DomainError> {
        // The dumb HTTP server reads no credentials.
        Ok(format!("fgj_{job_id}"))
    }
}
