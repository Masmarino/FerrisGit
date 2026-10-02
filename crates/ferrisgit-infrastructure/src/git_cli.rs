//! Shared plumbing for the adapters that drive the `git` binary.

use std::path::Path;
use std::process::Stdio;

use ferrisgit_domain::error::DomainError;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::error::infra;

pub(crate) struct GitOutput {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

impl GitOutput {
    pub fn stdout_trimmed(&self) -> String {
        String::from_utf8_lossy(&self.stdout).trim().to_string()
    }

    /// Strips one trailing newline rather than calling `.trim()`. A root file name ending in whitespace
    /// (possible through a raw `git push`) would otherwise get renamed on the next `save_page`.
    pub fn stdout_without_final_newline(&self) -> String {
        let text = String::from_utf8_lossy(&self.stdout);
        text.strip_suffix('\n').unwrap_or(&text).to_string()
    }
}

/// Runs `git` in `repo_path` with extra environment variables and optional stdin. A failing exit
/// status is reported in the output, not as an error: only a failure to run `git` at all is one.
pub(crate) async fn run(
    repo_path: &Path,
    args: &[&str],
    env: &[(&str, &str)],
    stdin: Option<&[u8]>,
) -> Result<GitOutput, DomainError> {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(repo_path)
        .envs(env.iter().copied());
    let output = match stdin {
        None => command.output().await.map_err(infra)?,
        Some(data) => {
            let mut child = command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(infra)?;
            let mut pipe = child.stdin.take().expect("stdin was piped");
            pipe.write_all(data).await.map_err(infra)?;
            // Closing the pipe lets the child see EOF.
            drop(pipe);
            child.wait_with_output().await.map_err(infra)?
        }
    };
    Ok(GitOutput {
        success: output.status.success(),
        code: output.status.code(),
        stdout: output.stdout,
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    })
}

/// Accepts a plausible short or full git object id (7 to 40 lowercase hex characters), so nothing
/// else can become a positional argument: given `--help`, `git ls-tree` exits 0 and prints help text.
pub(crate) fn is_plausible_commit_sha(value: &str) -> bool {
    (7..=40).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_plausible_commit_sha_accepts_short_and_full_lowercase_hex_and_rejects_everything_else() {
        assert!(is_plausible_commit_sha("abc1234"));
        assert!(is_plausible_commit_sha(&"a".repeat(40)));
        assert!(!is_plausible_commit_sha("abc123"), "too short");
        assert!(!is_plausible_commit_sha(&"a".repeat(41)), "too long");
        assert!(
            !is_plausible_commit_sha("ABC1234"),
            "uppercase hex must be rejected"
        );
        assert!(
            !is_plausible_commit_sha("main"),
            "a branch name is not a sha"
        );
        assert!(
            !is_plausible_commit_sha("--upload-pack=x"),
            "must not look like a flag"
        );
    }
}
