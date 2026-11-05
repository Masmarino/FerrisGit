use std::path::Path;

use crate::cli::{BodyArgs, Global};
use crate::client::Client;
use crate::config::{Store, TOKEN_ENV, env_host, normalize_instance, sends_token_in_clear};
use crate::error::{CliError, Result};
use crate::git;
use crate::model::Resolved;
use crate::output::{Output, read_stdin};
use crate::repo_ref::{RepoRef, parse_repo_arg};

pub struct Context {
    pub global: Global,
    pub out: Output,
    pub store: Store,
}

/// A repository the server has confirmed. `path` is its current one, which a rename can make differ from the request.
pub struct Repo {
    pub id: String,
    pub path: Vec<String>,
    /// Root first. Empty for a personal repository.
    pub groups: Vec<String>,
}

impl Repo {
    /// The start of a route of this repository.
    pub fn route(&self, rest: &str) -> String {
        format!("/repositories/{}{rest}", self.id)
    }
}

impl Context {
    /// An instance named on the command line wins, then `FERRISGIT_HOST`, then the Git remote, then the default instance.
    pub async fn instance(&self, hostname: Option<&str>) -> Result<String> {
        if let Some(repo) = &self.global.repo
            && let Some(instance) = parse_repo_arg(repo)?.instance
        {
            return Ok(instance);
        }
        if let Some(hostname) = hostname {
            return normalize_instance(hostname);
        }
        if let Some(instance) = env_host()? {
            return Ok(instance);
        }
        if let Some(instance) = git::repository_from_remotes()
            .await
            .and_then(|remote| remote.instance)
        {
            return self.refuse_unknown_cleartext(instance);
        }
        self.store.default_host().map(str::to_string).ok_or_else(|| {
            CliError::Auth(format!(
                "no instance: not signed in anywhere and no Git remote here. Run `fgit auth login --hostname <host>`, or \
                 set FERRISGIT_HOST and {TOKEN_ENV}"
            ))
        })
    }

    /// A remote is only someone's say-so: a plain HTTP one doesn't get a token unless someone chose that instance.
    fn refuse_unknown_cleartext(&self, instance: String) -> Result<String> {
        if sends_token_in_clear(&instance) && !self.store.knows(&instance) {
            return Err(CliError::Refused(format!(
                "the Git remote points at {instance} over plain HTTP, where a token could be read on the way. Use HTTPS, or \
                 name it yourself with --hostname or FERRISGIT_HOST"
            )));
        }
        Ok(instance)
    }

    pub async fn client_for(&self, instance: &str) -> Result<Client> {
        let Some((token, _)) = self.store.token_for(instance).await? else {
            return Err(CliError::Auth(format!(
                "not signed in to {instance}. Run `fgit auth login --hostname {instance}`, or set {TOKEN_ENV}"
            )));
        };
        if sends_token_in_clear(instance) {
            self.out.message(&format!(
                "warning: sending a token to {instance} over plain HTTP"
            ));
        }
        Client::new(instance, Some(token))
    }

    pub async fn client(&self, hostname: Option<&str>) -> Result<Client> {
        self.client_for(&self.instance(hostname).await?).await
    }

    /// Which repository this command is about, as written or from the Git remote. No server call.
    pub async fn repo_ref(&self, explicit: Option<&str>) -> Result<RepoRef> {
        if let Some(explicit) = explicit.or(self.global.repo.as_deref()) {
            return parse_repo_arg(explicit);
        }
        git::repository_from_remotes().await.ok_or_else(|| {
            CliError::Usage(
                "no repository: name one with --repo host/owner/name, or run this inside a clone"
                    .to_string(),
            )
        })
    }

    pub async fn repo(&self) -> Result<(Client, Repo)> {
        self.repo_named(None).await
    }

    pub async fn repo_named(&self, explicit: Option<&str>) -> Result<(Client, Repo)> {
        let reference = self.repo_ref(explicit).await?;
        // `--repo` may have said an instance that isn't the one a positional argument names.
        let instance = match (&reference.instance, explicit) {
            (Some(instance), Some(_)) => instance.clone(),
            _ => self.instance(None).await?,
        };
        let client = self.client_for(&instance).await?;
        let repo = resolve(&client, &reference).await?;
        Ok((client, repo))
    }
}

pub async fn resolve(client: &Client, reference: &RepoRef) -> Result<Repo> {
    let resolved: Resolved = client
        .get(&format!("/resolve/{}", reference.path_string()))
        .await?;
    match resolved {
        Resolved::PersonalRepository {
            repository_id,
            path,
        } => Ok(Repo {
            id: repository_id,
            path,
            groups: vec![],
        }),
        Resolved::GroupRepository {
            repository_id,
            path,
            chain,
        } => Ok(Repo {
            id: repository_id,
            path,
            groups: chain.into_iter().map(|group| group.id).collect(),
        }),
        Resolved::Group {} => Err(CliError::NotFound(format!(
            "{} is a group, not a repository",
            reference.path_string()
        ))),
    }
}

pub fn read_file_or_stdin(path: &Path) -> Result<String> {
    if path == Path::new("-") {
        return read_stdin();
    }
    std::fs::read_to_string(path)
        .map_err(|e| CliError::Other(format!("could not read {}: {e}", path.display())))
}

impl BodyArgs {
    pub fn read(&self) -> Result<Option<String>> {
        match (&self.body, &self.body_file) {
            (Some(body), _) => Ok(Some(body.clone())),
            (None, Some(file)) => read_file_or_stdin(file).map(Some),
            (None, None) => Ok(None),
        }
    }
}

/// For the rows of a listing. Ids are long, and a start of one is accepted wherever one is asked for.
pub fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

pub fn is_full_id(value: &str) -> bool {
    value.len() == 36
        && value.chars().enumerate().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

/// The one id in `ids` that starts with `prefix`.
pub fn unique_match<'a>(
    kind: &str,
    prefix: &str,
    mut ids: impl Iterator<Item = &'a str>,
) -> Result<String> {
    if prefix.len() < 4 {
        return Err(CliError::Usage(format!(
            "{prefix}: give at least 4 characters of the {kind} id"
        )));
    }
    let prefix = prefix.to_lowercase();
    let mut matching = ids.by_ref().filter(|id| id.starts_with(&prefix));
    let Some(first) = matching.next() else {
        return Err(CliError::NotFound(format!(
            "no {kind} starts with {prefix}"
        )));
    };
    if matching.next().is_some() {
        return Err(CliError::Usage(format!(
            "several {kind}s start with {prefix}: give more of the id"
        )));
    }
    Ok(first.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_id_is_recognised() {
        assert!(is_full_id("3f2a91c0-1234-4abc-8def-0123456789ab"));
        assert!(!is_full_id("3f2a91c0"));
        assert!(!is_full_id("zf2a91c0-1234-4abc-8def-0123456789ab"));
    }

    #[test]
    fn a_start_of_an_id_must_be_long_enough_and_unique() {
        let ids = ["3f2a91c0-aaaa", "3f2a0000-bbbb", "77770000-cccc"];
        let find = |prefix: &str| unique_match("merge request", prefix, ids.iter().copied());
        assert_eq!(find("7777").unwrap(), "77770000-cccc");
        assert_eq!(find("3F2A91").unwrap(), "3f2a91c0-aaaa");
        assert!(matches!(find("3f2a"), Err(CliError::Usage(_))));
        assert!(matches!(find("777"), Err(CliError::Usage(_))));
        assert!(matches!(find("aaaa"), Err(CliError::NotFound(_))));
    }
}
