use std::path::PathBuf;
use std::process::Stdio;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum GitBackendError {
    #[error("git command failed to start: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("malformed CGI response from git http-backend")]
    MalformedCgiResponse,
    #[error("git http-backend timed out")]
    Timeout,
}

pub struct SmartHttpRequest {
    pub path_info: String,
    pub method: String,
    pub query_string: String,
    pub content_type: String,
    pub body: Vec<u8>,
}

pub struct SmartHttpResponse {
    pub status: u16,
    pub content_type: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

pub struct GitBackend {
    storage_root: PathBuf,
}

impl GitBackend {
    pub fn new(storage_root: PathBuf) -> Self {
        Self { storage_root }
    }

    pub fn init_bare_repo(&self, disk_path: &str) -> std::io::Result<()> {
        let full_path = self.storage_root.join(disk_path);
        std::fs::create_dir_all(&full_path)?;
        let status = std::process::Command::new("git")
            .args(["init", "--bare"])
            .arg(&full_path)
            .status()?;
        if !status.success() {
            return Err(std::io::Error::other("git init --bare failed"));
        }
        // http-backend answers git-receive-pack with a bare 403 unless http.receivepack is set.
        // Write access is checked upstream.
        let config_status = std::process::Command::new("git")
            .args(["config", "http.receivepack", "true"])
            .current_dir(&full_path)
            .status()?;
        if !config_status.success() {
            return Err(std::io::Error::other("git config http.receivepack failed"));
        }
        Ok(())
    }

    /// A first push rarely matches the default branch name from `init_bare_repo`, which leaves HEAD dangling
    /// and the repo looking empty (file browser, push pipelines). With exactly one branch, point HEAD at it.
    /// The wiki variant renames the branch instead.
    pub async fn heal_dangling_head(&self, disk_path: &str) -> std::io::Result<()> {
        let full_path = self.storage_root.join(disk_path);

        let symref = Command::new("git")
            .args(["symbolic-ref", "HEAD"])
            .current_dir(&full_path)
            .output()
            .await?;
        if !symref.status.success() {
            return Ok(());
        }
        let current_ref = String::from_utf8_lossy(&symref.stdout).trim().to_string();

        let resolves = Command::new("git")
            .args(["rev-parse", "--verify", "--quiet", &current_ref])
            .current_dir(&full_path)
            .status()
            .await?
            .success();
        if resolves {
            return Ok(());
        }

        let listing = Command::new("git")
            .args(["for-each-ref", "refs/heads/", "--format=%(refname)"])
            .current_dir(&full_path)
            .output()
            .await?;
        if !listing.status.success() {
            tracing::warn!(
                disk_path,
                "git for-each-ref failed while checking for a dangling repository HEAD"
            );
            return Ok(());
        }
        let listing = String::from_utf8_lossy(&listing.stdout);
        let branches: Vec<&str> = listing.lines().filter(|l| !l.is_empty()).collect();
        if branches.len() != 1 {
            return Ok(());
        }

        let retarget = Command::new("git")
            .args(["symbolic-ref", "HEAD", branches[0]])
            .current_dir(&full_path)
            .status()
            .await?;
        if !retarget.success() {
            tracing::warn!(
                disk_path,
                target = branches[0],
                "failed to repoint a dangling repository HEAD"
            );
        }
        Ok(())
    }

    /// A missing path is fine: this is best-effort cleanup after the database row is gone.
    pub fn remove_bare_repo(&self, disk_path: &str) -> std::io::Result<()> {
        let full_path = self.storage_root.join(disk_path);
        match std::fs::remove_dir_all(&full_path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Total size in bytes under `disk_path`, a plain walk like `du`.
    pub fn directory_size(&self, disk_path: &str) -> std::io::Result<u64> {
        fn walk(dir: &std::path::Path) -> std::io::Result<u64> {
            let mut total = 0u64;
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let metadata = entry.metadata()?;
                if metadata.is_dir() {
                    total += walk(&entry.path())?;
                } else {
                    total += metadata.len();
                }
            }
            Ok(total)
        }
        walk(&self.storage_root.join(disk_path))
    }

    pub async fn handle_smart_http(
        &self,
        disk_path: &str,
        req: SmartHttpRequest,
    ) -> Result<SmartHttpResponse, GitBackendError> {
        let full_path = self.storage_root.join(disk_path);
        let mut child = Command::new("git")
            .arg("http-backend")
            .env("GIT_PROJECT_ROOT", &self.storage_root)
            .env("GIT_HTTP_EXPORT_ALL", "1")
            .env("PATH_INFO", format!("/{disk_path}{}", req.path_info))
            .env("REQUEST_METHOD", &req.method)
            .env("QUERY_STRING", &req.query_string)
            .env("CONTENT_TYPE", &req.content_type)
            .current_dir(&full_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let mut stdin = child.stdin.take().expect("stdin piped");
        let body = req.body;
        let write_task = tokio::spawn(async move {
            let _ = stdin.write_all(&body).await;
        });

        let mut stderr = child.stderr.take().expect("stderr piped");
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            let _ = stderr.read_to_end(&mut buf).await;
            buf
        });

        let mut stdout = child.stdout.take().expect("stdout piped");
        let mut raw = Vec::new();
        // Long on purpose: this only stops a hung child (corrupt repo, disk trouble). Pushes accept 500 MiB
        // bodies and can take minutes, so a short timeout would kill exactly those.
        match tokio::time::timeout(
            std::time::Duration::from_secs(600),
            stdout.read_to_end(&mut raw),
        )
        .await
        {
            Ok(read_result) => {
                read_result?;
            }
            Err(_elapsed) => {
                // Kill the hung child, otherwise it keeps running with its file descriptors open.
                let _ = child.start_kill();
                return Err(GitBackendError::Timeout);
            }
        }
        let _ = write_task.await;
        let stderr_bytes = stderr_task.await.unwrap_or_default();
        let status = child.wait().await?;

        if !status.success() {
            tracing::warn!(
                disk_path,
                exit_status = %status,
                stderr = %String::from_utf8_lossy(&stderr_bytes),
                "git http-backend exited with non-success status"
            );
        }

        parse_cgi_response(&raw)
    }
}

impl ferrisgit_domain::storage_size::DirectorySizePort for GitBackend {
    fn directory_size(&self, disk_path: &str) -> std::io::Result<u64> {
        GitBackend::directory_size(self, disk_path)
    }
}

fn parse_cgi_response(raw: &[u8]) -> Result<SmartHttpResponse, GitBackendError> {
    let separator = b"\r\n\r\n";
    let split_at = raw
        .windows(4)
        .position(|w| w == separator)
        .ok_or(GitBackendError::MalformedCgiResponse)?;
    let (header_block, rest) = raw.split_at(split_at);
    let body = rest[4..].to_vec();
    let header_text =
        std::str::from_utf8(header_block).map_err(|_| GitBackendError::MalformedCgiResponse)?;

    let mut status = 200u16;
    let mut content_type = String::from("application/octet-stream");
    let mut headers = Vec::new();
    for line in header_text.lines() {
        if let Some((name, value)) = line.split_once(':') {
            let name = name.trim();
            let value = value.trim();
            match name.to_ascii_lowercase().as_str() {
                "status" => {
                    status = value
                        .split_whitespace()
                        .next()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(200)
                }
                "content-type" => {
                    content_type = value.to_string();
                    headers.push((name.to_string(), value.to_string()));
                }
                _ => headers.push((name.to_string(), value.to_string())),
            }
        }
    }
    Ok(SmartHttpResponse {
        status,
        content_type,
        headers,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_git::{git_stdout, git_stdout_untrimmed, git_succeeds, push_one_commit};

    #[tokio::test]
    async fn info_refs_against_a_freshly_initialized_bare_repo_advertises_upload_pack() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = GitBackend::new(tmp.path().to_path_buf());
        backend.init_bare_repo("owner/repo.git").unwrap();

        let response = backend
            .handle_smart_http(
                "owner/repo.git",
                SmartHttpRequest {
                    path_info: "/info/refs".to_string(),
                    method: "GET".to_string(),
                    query_string: "service=git-upload-pack".to_string(),
                    content_type: String::new(),
                    body: Vec::new(),
                },
            )
            .await
            .unwrap();

        assert_eq!(response.status, 200);
        assert!(
            response
                .content_type
                .contains("git-upload-pack-advertisement")
        );
        assert!(
            response
                .body
                .starts_with(b"001e# service=git-upload-pack\n")
        );
    }

    /// stderr used to be piped but never read, so a failing upload-pack that wrote a lot could block forever.
    #[tokio::test]
    async fn smart_http_drains_stderr_and_completes_when_git_reports_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = GitBackend::new(tmp.path().to_path_buf());
        backend.init_bare_repo("owner/repo.git").unwrap();

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            backend.handle_smart_http(
                "owner/repo.git",
                SmartHttpRequest {
                    path_info: "/git-upload-pack".to_string(),
                    method: "POST".to_string(),
                    query_string: String::new(),
                    content_type: "application/x-git-upload-pack-request".to_string(),
                    body: b"this is not a valid git pack protocol request".to_vec(),
                },
            ),
        )
        .await
        .expect("handle_smart_http must not hang even when git writes to stderr and fails");

        // Either outcome is fine, the point is not hanging.
        let _ = result;
    }

    /// parse_cgi_response used to drop Expires/Pragma/Cache-Control, which info/refs responses need for
    /// intermediate caches.
    #[tokio::test]
    async fn info_refs_response_forwards_cache_control_headers_from_git_http_backend() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = GitBackend::new(tmp.path().to_path_buf());
        backend.init_bare_repo("owner/repo.git").unwrap();

        let response = backend
            .handle_smart_http(
                "owner/repo.git",
                SmartHttpRequest {
                    path_info: "/info/refs".to_string(),
                    method: "GET".to_string(),
                    query_string: "service=git-upload-pack".to_string(),
                    content_type: String::new(),
                    body: Vec::new(),
                },
            )
            .await
            .unwrap();

        let header_names: Vec<&str> = response
            .headers
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        assert!(
            header_names
                .iter()
                .any(|n| n.eq_ignore_ascii_case("cache-control")),
            "headers were: {header_names:?}"
        );
    }

    /// A first push to a branch other than the initial HEAD target left HEAD dangling. The odd branch name
    /// keeps the test independent of the host's git defaults.
    #[tokio::test]
    async fn heal_dangling_head_repoints_head_to_the_sole_pushed_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = GitBackend::new(tmp.path().to_path_buf());
        backend.init_bare_repo("owner/repo.git").unwrap();
        let repo_path = tmp.path().join("owner/repo.git");

        push_one_commit(&repo_path, "pushed-branch", "file.txt", "hello");

        assert!(
            !git_succeeds(&repo_path, &["rev-parse", "--verify", "--quiet", "HEAD"]),
            "HEAD must be dangling before healing, or this test isn't exercising the bug"
        );

        backend.heal_dangling_head("owner/repo.git").await.unwrap();

        assert_eq!(
            git_stdout(&repo_path, &["symbolic-ref", "HEAD"]),
            "refs/heads/pushed-branch"
        );
        assert_eq!(
            git_stdout_untrimmed(&repo_path, &["cat-file", "-p", "HEAD:file.txt"]),
            "hello"
        );
    }

    #[tokio::test]
    async fn heal_dangling_head_is_a_no_op_when_head_already_resolves() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = GitBackend::new(tmp.path().to_path_buf());
        backend.init_bare_repo("owner/repo.git").unwrap();
        let repo_path = tmp.path().join("owner/repo.git");

        let head = git_stdout(&repo_path, &["symbolic-ref", "HEAD"]);
        let default_branch = head.trim_start_matches("refs/heads/");
        push_one_commit(&repo_path, default_branch, "file.txt", "hello");
        assert!(
            git_succeeds(&repo_path, &["rev-parse", "--verify", "--quiet", "HEAD"]),
            "HEAD must already resolve, or this test isn't exercising the no-op path"
        );

        backend.heal_dangling_head("owner/repo.git").await.unwrap();

        assert_eq!(
            git_stdout(&repo_path, &["symbolic-ref", "HEAD"]),
            format!("refs/heads/{default_branch}"),
            "healing an already-healthy HEAD must not move it"
        );
    }

    #[tokio::test]
    async fn heal_dangling_head_leaves_head_alone_when_the_repo_genuinely_has_no_commits_yet() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = GitBackend::new(tmp.path().to_path_buf());
        backend.init_bare_repo("owner/repo.git").unwrap();
        let repo_path = tmp.path().join("owner/repo.git");
        let before_target = git_stdout(&repo_path, &["symbolic-ref", "HEAD"]);

        backend.heal_dangling_head("owner/repo.git").await.unwrap();

        assert_eq!(
            git_stdout(&repo_path, &["symbolic-ref", "HEAD"]),
            before_target,
            "a genuinely empty repo has nothing to heal"
        );
    }

    #[tokio::test]
    async fn heal_dangling_head_leaves_head_alone_when_more_than_one_branch_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = GitBackend::new(tmp.path().to_path_buf());
        backend.init_bare_repo("owner/repo.git").unwrap();
        let repo_path = tmp.path().join("owner/repo.git");

        push_one_commit(&repo_path, "branch-a", "a.txt", "a");
        push_one_commit(&repo_path, "branch-b", "b.txt", "b");
        let before_target = git_stdout(&repo_path, &["symbolic-ref", "HEAD"]);

        backend.heal_dangling_head("owner/repo.git").await.unwrap();

        assert_eq!(
            git_stdout(&repo_path, &["symbolic-ref", "HEAD"]),
            before_target,
            "ambiguous which branch should be default — must leave HEAD alone"
        );
        for branch in ["branch-a", "branch-b"] {
            assert!(
                git_succeeds(
                    &repo_path,
                    &[
                        "rev-parse",
                        "--verify",
                        "--quiet",
                        &format!("refs/heads/{branch}"),
                    ]
                ),
                "{branch} must be untouched"
            );
        }
    }

    #[test]
    fn remove_bare_repo_deletes_the_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = GitBackend::new(tmp.path().to_path_buf());
        backend.init_bare_repo("owner/repo.git").unwrap();
        assert!(tmp.path().join("owner/repo.git").exists());

        backend.remove_bare_repo("owner/repo.git").unwrap();

        assert!(!tmp.path().join("owner/repo.git").exists());
    }

    #[test]
    fn remove_bare_repo_is_idempotent_for_a_path_that_never_existed() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = GitBackend::new(tmp.path().to_path_buf());

        backend.remove_bare_repo("owner/never-existed.git").unwrap();
    }

    #[test]
    fn directory_size_sums_every_file_recursively() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = GitBackend::new(tmp.path().to_path_buf());
        std::fs::create_dir_all(tmp.path().join("owner/repo.git/objects/pack")).unwrap();
        std::fs::write(
            tmp.path().join("owner/repo.git/HEAD"),
            "ref: refs/heads/main\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("owner/repo.git/objects/pack/pack-abc.pack"),
            vec![0u8; 1000],
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("owner/repo.git/objects/pack/pack-abc.idx"),
            vec![0u8; 234],
        )
        .unwrap();

        let size = backend.directory_size("owner/repo.git").unwrap();

        assert_eq!(size, "ref: refs/heads/main\n".len() as u64 + 1000 + 234);
    }

    #[test]
    fn directory_size_is_zero_for_an_empty_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = GitBackend::new(tmp.path().to_path_buf());
        std::fs::create_dir_all(tmp.path().join("owner/empty.git")).unwrap();

        assert_eq!(backend.directory_size("owner/empty.git").unwrap(), 0);
    }
}
