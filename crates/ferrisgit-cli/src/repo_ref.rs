use url::Url;

use crate::config::normalize_instance;
use crate::error::{CliError, Result};

/// A repository as a person writes it. `instance` is `None` for the short form, whose instance comes from elsewhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRef {
    pub instance: Option<String>,
    pub path: Vec<String>,
}

impl RepoRef {
    pub fn path_string(&self) -> String {
        self.path.join("/")
    }
}

/// Repository and group names only take letters, digits, `-` and `_`, so a dot, a colon or `localhost` can only be an instance.
fn looks_like_instance(segment: &str) -> bool {
    segment.contains('.') || segment.contains(':') || segment == "localhost"
}

fn clean_path(path: &str) -> Vec<String> {
    path.trim_matches('/')
        .trim_end_matches(".git")
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect()
}

/// `--repo`: `host/owner/name`, `https://host/owner/name`, or `owner/name` on the current instance. Groups nest, so the path
/// can be longer.
pub fn parse_repo_arg(input: &str) -> Result<RepoRef> {
    let invalid = || {
        CliError::Usage(format!(
            "not a repository: {input}. Expected host/owner/name or owner/name"
        ))
    };
    let (instance, path) = if input.contains("://") {
        let url = Url::parse(input).map_err(|_| invalid())?;
        let origin = url.origin().ascii_serialization();
        (Some(normalize_instance(&origin)?), clean_path(url.path()))
    } else {
        let trimmed = input.trim_matches('/');
        match trimmed.split_once('/') {
            Some((first, rest)) if looks_like_instance(first) => {
                (Some(normalize_instance(first)?), clean_path(rest))
            }
            _ => (None, clean_path(trimmed)),
        }
    };
    if path.len() < 2 {
        return Err(invalid());
    }
    Ok(RepoRef { instance, path })
}

/// A Git remote's URL, HTTP or scp-like. Credentials in the URL are dropped: a token left there isn't for the API.
pub fn parse_remote_url(remote: &str) -> Option<RepoRef> {
    let remote = remote.trim();
    let (instance, path) = if remote.contains("://") {
        let url = Url::parse(remote).ok()?;
        let origin = match url.scheme() {
            "http" | "https" => url.origin().ascii_serialization(),
            // No SSH on the server, so the instance speaks HTTPS even if a remote says otherwise.
            "ssh" => format!("https://{}", url.host_str()?),
            _ => return None,
        };
        (normalize_instance(&origin).ok()?, clean_path(url.path()))
    } else {
        let (host, path) = remote.split_once(':')?;
        let host = host.rsplit('@').next()?;
        if host.is_empty() || path.starts_with('\\') || host.len() == 1 {
            // `C:\path` is a local directory, not a host.
            return None;
        }
        (normalize_instance(host).ok()?, clean_path(path))
    };
    (path.len() >= 2).then_some(RepoRef {
        instance: Some(instance),
        path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn repo_names_its_instance_or_leaves_it_open() {
        assert_eq!(
            parse_repo_arg("git.example.com/acme/hello").unwrap(),
            RepoRef {
                instance: Some("https://git.example.com".to_string()),
                path: path(&["acme", "hello"]),
            }
        );
        assert_eq!(
            parse_repo_arg("acme/hello").unwrap(),
            RepoRef {
                instance: None,
                path: path(&["acme", "hello"]),
            }
        );
    }

    #[test]
    fn nested_groups_keep_their_whole_path() {
        assert_eq!(
            parse_repo_arg("git.example.com/acme/team/sub/hello")
                .unwrap()
                .path,
            path(&["acme", "team", "sub", "hello"])
        );
    }

    #[test]
    fn a_local_instance_and_a_full_url_work_too() {
        let local = parse_repo_arg("localhost:8080/acme/hello").unwrap();
        assert_eq!(local.instance.as_deref(), Some("http://localhost:8080"));
        let full = parse_repo_arg("http://git.example.com/acme/hello.git").unwrap();
        assert_eq!(full.instance.as_deref(), Some("http://git.example.com"));
        assert_eq!(full.path, path(&["acme", "hello"]));
    }

    #[test]
    fn a_path_without_a_name_is_refused() {
        for input in ["hello", "git.example.com/hello", ""] {
            assert!(parse_repo_arg(input).is_err(), "{input}");
        }
    }

    #[test]
    fn a_remote_gives_its_instance_and_path() {
        let remote = parse_remote_url("https://git.example.com/acme/team/hello.git").unwrap();
        assert_eq!(remote.instance.as_deref(), Some("https://git.example.com"));
        assert_eq!(remote.path, path(&["acme", "team", "hello"]));
    }

    #[test]
    fn credentials_in_a_remote_stay_out_of_the_instance() {
        let remote =
            parse_remote_url("https://alice:fg_secret@git.example.com/acme/hello.git").unwrap();
        assert_eq!(remote.instance.as_deref(), Some("https://git.example.com"));
    }

    #[test]
    fn a_port_and_plain_http_survive() {
        let remote = parse_remote_url("http://localhost:8080/acme/hello.git").unwrap();
        assert_eq!(remote.instance.as_deref(), Some("http://localhost:8080"));
    }

    #[test]
    fn an_scp_like_remote_maps_to_https() {
        let remote = parse_remote_url("git@git.example.com:acme/hello.git").unwrap();
        assert_eq!(remote.instance.as_deref(), Some("https://git.example.com"));
        assert_eq!(remote.path, path(&["acme", "hello"]));
    }

    #[test]
    fn a_local_path_is_not_a_remote_on_an_instance() {
        assert!(parse_remote_url(r"C:\repos\hello").is_none());
        assert!(parse_remote_url("/srv/git/hello.git").is_none());
        assert!(parse_remote_url("file:///srv/git/acme/hello.git").is_none());
    }
}
