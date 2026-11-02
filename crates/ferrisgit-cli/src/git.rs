use std::process::Stdio;

use tokio::process::Command;

use crate::error::{CliError, Result};
use crate::output::stdin_is_terminal;
use crate::repo_ref::{RepoRef, parse_remote_url};

fn git_command() -> Command {
    let mut command = Command::new("git");
    if !stdin_is_terminal() {
        // Otherwise a missing credential hangs a script on a prompt nobody can see.
        command.env("GIT_TERMINAL_PROMPT", "0");
    }
    command
}

fn cannot_run(e: std::io::Error) -> CliError {
    CliError::Other(format!("could not run git: {e}"))
}

/// For reading: `None` when git says no, which outside a repository is normal.
async fn read(args: &[&str]) -> Option<String> {
    let output = git_command()
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub async fn current_branch() -> Result<String> {
    read(&["symbolic-ref", "--quiet", "--short", "HEAD"])
        .await
        .filter(|branch| !branch.is_empty())
        .ok_or_else(|| {
            CliError::Usage(
                "no current branch: not on a branch of a Git repository. Name one with --source"
                    .to_string(),
            )
        })
}

/// `origin` if it points at an instance, or else the only remote that does. With several and no `origin`, guessing could
/// send work to the wrong place, so it takes `--repo`.
pub async fn repository_from_remotes() -> Option<RepoRef> {
    if let Some(origin) = read(&["remote", "get-url", "origin"])
        .await
        .and_then(|url| parse_remote_url(&url))
    {
        return Some(origin);
    }
    let remotes = read(&["remote"]).await?;
    let mut found = Vec::new();
    for remote in remotes.lines() {
        if let Some(parsed) = read(&["remote", "get-url", remote])
            .await
            .and_then(|url| parse_remote_url(&url))
        {
            found.push(parsed);
        }
    }
    (found.len() == 1).then(|| found.remove(0))
}

/// The instance `origin` points at, to push through the remote when it's the one being talked to.
pub async fn origin_instance() -> Option<String> {
    read(&["remote", "get-url", "origin"])
        .await
        .and_then(|url| parse_remote_url(&url))
        .and_then(|remote| remote.instance)
}

/// Git runs a helper through a shell, which wants forward slashes on Windows too.
fn credential_helper() -> Result<String> {
    let exe = std::env::current_exe()
        .map_err(|e| CliError::Other(format!("could not find the fgit executable: {e}")))?;
    let exe = exe.to_string_lossy().replace('\\', "/");
    Ok(format!("!\"{exe}\" auth git-credential"))
}

/// `-c` options that make this one git command ask fgit for the credentials of `instance`, and only of that instance. The
/// empty value first clears helpers from other configurations, or one of them might answer first with an old password.
pub fn credential_options(instance: &str) -> Result<Vec<String>> {
    let key = format!("credential.{instance}.helper");
    Ok(vec![
        "-c".to_string(),
        format!("{key}="),
        "-c".to_string(),
        format!("{key}={}", credential_helper()?),
    ])
}

/// Git's output goes straight to stderr, where the person can watch it. Stdout stays for results.
pub async fn run_visible(args: &[String]) -> Result<()> {
    let status = git_command()
        .args(args)
        .stdout(std::io::stderr())
        .status()
        .await
        .map_err(cannot_run)?;
    if status.success() {
        Ok(())
    } else {
        // Only the subcommand: the rest of the line holds configuration, not what the person asked for.
        let name = args
            .iter()
            .find(|a| {
                matches!(
                    a.as_str(),
                    "clone" | "push" | "fetch" | "checkout" | "switch" | "config"
                )
            })
            .map_or("git", String::as_str);
        Err(CliError::Other(format!("git {name} failed")))
    }
}

/// Makes `git push` and `git pull` work later in a clone, with nothing to store but this reference to fgit.
pub async fn configure_helper(before: &[&str], scope: &str, instance: &str) -> Result<()> {
    let key = format!("credential.{instance}.helper");
    let config = |rest: [String; 3]| -> Vec<String> {
        before
            .iter()
            .map(|s| s.to_string())
            .chain(["config".to_string(), scope.to_string()])
            .chain(rest)
            .collect()
    };
    run_visible(&config([
        "--replace-all".to_string(),
        key.clone(),
        String::new(),
    ]))
    .await?;
    run_visible(&config(["--add".to_string(), key, credential_helper()?])).await
}

pub fn clone_url(instance: &str, path: &[String]) -> String {
    format!("{instance}/{}.git", path.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_helper_is_scoped_to_one_instance_and_clears_others_first() {
        let options = credential_options("https://git.example.com").unwrap();
        assert_eq!(options[0], "-c");
        assert_eq!(options[1], "credential.https://git.example.com.helper=");
        assert!(options[3].starts_with("credential.https://git.example.com.helper=!\""));
        assert!(options[3].ends_with(" auth git-credential"));
        assert!(!options[3].contains('\\'));
    }

    #[test]
    fn a_clone_url_follows_the_whole_path() {
        let path = vec!["acme".to_string(), "team".to_string(), "hello".to_string()];
        assert_eq!(
            clone_url("https://git.example.com", &path),
            "https://git.example.com/acme/team/hello.git"
        );
    }
}
