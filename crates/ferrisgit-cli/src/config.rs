use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{CliError, Result};

pub const TOKEN_ENV: &str = "FERRISGIT_TOKEN";
pub const HOST_ENV: &str = "FERRISGIT_HOST";
const CONFIG_DIR_ENV: &str = "FERRISGIT_CONFIG_DIR";
const KEYCHAIN_SERVICE: &str = "fgit";

/// `git.example.com`, `https://git.example.com/` and the like, as `scheme://host[:port]`. A bare local name is plain
/// HTTP, because that's what a development instance speaks. Anything else defaults to HTTPS.
pub fn normalize_instance(input: &str) -> Result<String> {
    let invalid = || CliError::Usage(format!("not an instance address: {input}"));
    let input = input.trim();
    let with_scheme = if input.contains("://") {
        input.to_string()
    } else if is_loopback_name(input.split(':').next().unwrap_or("")) {
        format!("http://{input}")
    } else {
        format!("https://{input}")
    };
    let url = Url::parse(&with_scheme).map_err(|_| invalid())?;
    let host = url.host_str().ok_or_else(invalid)?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || !url.path().trim_end_matches('/').is_empty()
        || url.query().is_some()
    {
        return Err(invalid());
    }
    Ok(match url.port() {
        Some(port) => format!("{}://{host}:{port}", url.scheme()),
        None => format!("{}://{host}", url.scheme()),
    })
}

fn is_loopback_name(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]") || host.ends_with(".localhost")
}

/// A token sent over plain HTTP to anything but this machine can be read on the way.
pub fn sends_token_in_clear(instance: &str) -> bool {
    let Ok(url) = Url::parse(instance) else {
        return true;
    };
    url.scheme() != "https" && !url.host_str().is_some_and(is_loopback_name)
}

/// The instance `FERRISGIT_HOST` names, if it is set.
pub fn env_host() -> Result<Option<String>> {
    match std::env::var(HOST_ENV) {
        Ok(value) if !value.trim().is_empty() => normalize_instance(&value).map(Some),
        _ => Ok(None),
    }
}

fn env_token() -> Option<String> {
    std::env::var(TOKEN_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TokenSource {
    Environment,
    Keychain,
    File,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostsFile {
    default_host: Option<String>,
    #[serde(default)]
    hosts: BTreeMap<String, HostEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostEntry {
    pub username: Option<String>,
    /// Only with `--insecure-storage`. Otherwise the token is in the system keychain and this stays empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<String>,
}

/// Which instances are signed in. Tokens live in the system keychain, the file only says what's there.
pub struct Store {
    path: PathBuf,
    file: HostsFile,
}

impl Store {
    pub fn load() -> Result<Self> {
        let dir = match std::env::var_os(CONFIG_DIR_ENV) {
            Some(dir) => PathBuf::from(dir),
            None => dirs::config_dir()
                .ok_or_else(|| CliError::Other("no configuration directory here".to_string()))?
                .join("fgit"),
        };
        Self::load_from(dir.join("hosts.json"))
    }

    fn load_from(path: PathBuf) -> Result<Self> {
        let file = match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| CliError::Other(format!("could not read {}: {e}", path.display())))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => HostsFile::default(),
            Err(e) => {
                return Err(CliError::Other(format!(
                    "could not read {}: {e}",
                    path.display()
                )));
            }
        };
        Ok(Self { path, file })
    }

    fn save(&self) -> Result<()> {
        let failed = |e: std::io::Error| {
            CliError::Other(format!("could not write {}: {e}", self.path.display()))
        };
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(failed)?;
        }
        let text =
            serde_json::to_string_pretty(&self.file).map_err(|e| CliError::Other(e.to_string()))?;
        write_private(&self.path, &text).map_err(failed)
    }

    pub fn default_host(&self) -> Option<&str> {
        self.file.default_host.as_deref()
    }

    pub fn hosts(&self) -> impl Iterator<Item = (&String, &HostEntry)> {
        self.file.hosts.iter()
    }

    pub fn knows(&self, instance: &str) -> bool {
        self.file.hosts.contains_key(instance)
    }

    /// A token from the environment wins, but only for the instance `FERRISGIT_HOST` names when it names one: a repository
    /// whose remote points elsewhere must not receive it.
    pub async fn token_for(&self, instance: &str) -> Result<Option<(String, TokenSource)>> {
        if let Some(token) = env_token() {
            match env_host()? {
                Some(host) if host != instance => {}
                _ => return Ok(Some((token, TokenSource::Environment))),
            }
        }
        let Some(entry) = self.file.hosts.get(instance) else {
            return Ok(None);
        };
        if let Some(token) = &entry.token {
            return Ok(Some((token.clone(), TokenSource::File)));
        }
        Ok(keychain_get(instance)
            .await?
            .map(|token| (token, TokenSource::Keychain)))
    }

    /// Stores in the keychain unless `insecure` says otherwise, and never falls back to the file on its own.
    pub async fn sign_in(
        &mut self,
        instance: &str,
        username: &str,
        token: &str,
        insecure: bool,
    ) -> Result<TokenSource> {
        let source = if insecure {
            TokenSource::File
        } else {
            keychain_set(instance, token).await?;
            TokenSource::Keychain
        };
        self.file.hosts.insert(
            instance.to_string(),
            HostEntry {
                username: Some(username.to_string()),
                token: insecure.then(|| token.to_string()),
            },
        );
        if self.file.default_host.is_none() || self.file.hosts.len() == 1 {
            self.file.default_host = Some(instance.to_string());
        }
        self.save()?;
        Ok(source)
    }

    pub async fn sign_out(&mut self, instance: &str) -> Result<bool> {
        let Some(entry) = self.file.hosts.remove(instance) else {
            return Ok(false);
        };
        if entry.token.is_none() {
            keychain_delete(instance).await?;
        }
        if self.file.default_host.as_deref() == Some(instance) {
            self.file.default_host = self.file.hosts.keys().next().cloned();
        }
        self.save()?;
        Ok(true)
    }
}

#[cfg(unix)]
fn write_private(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(text.as_bytes())
}

/// On Windows the file inherits the profile directory's permissions, which already keep other users out.
#[cfg(not(unix))]
fn write_private(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    std::fs::write(path, text)
}

fn keychain_failed(e: impl std::fmt::Display) -> CliError {
    CliError::Other(format!(
        "the system keychain can't be used here: {e}. Use {TOKEN_ENV}, or sign in with --insecure-storage to keep the \
         token in a plain file"
    ))
}

/// The library's wording for a machine with no keychain service reads like something fgit forgot to set up.
fn keychain_error(e: keyring::Error) -> CliError {
    match e {
        keyring::Error::NoDefaultStore => keychain_failed("no keychain service answered"),
        other => keychain_failed(other),
    }
}

/// The keychain API blocks, and on Linux it runs its own async runtime, so it needs a thread of its own.
async fn on_keychain_thread<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(keychain_failed)
}

async fn keychain_get(instance: &str) -> Result<Option<String>> {
    let instance = instance.to_string();
    on_keychain_thread(move || {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &instance).map_err(keychain_error)?;
        match entry.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(keychain_error(e)),
        }
    })
    .await?
}

async fn keychain_set(instance: &str, token: &str) -> Result<()> {
    let (instance, token) = (instance.to_string(), token.to_string());
    on_keychain_thread(move || {
        keyring::Entry::new(KEYCHAIN_SERVICE, &instance)
            .and_then(|entry| entry.set_password(&token))
            .map_err(keychain_error)
    })
    .await?
}

async fn keychain_delete(instance: &str) -> Result<()> {
    let instance = instance.to_string();
    on_keychain_thread(move || {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &instance).map_err(keychain_error)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(keychain_error(e)),
        }
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_instance_reads_the_same_however_it_was_written() {
        for input in [
            "git.example.com",
            "https://git.example.com",
            "https://git.example.com/",
            " git.example.com ",
        ] {
            assert_eq!(
                normalize_instance(input).unwrap(),
                "https://git.example.com"
            );
        }
        assert_eq!(
            normalize_instance("git.example.com:8443").unwrap(),
            "https://git.example.com:8443"
        );
    }

    #[test]
    fn a_local_name_defaults_to_http() {
        assert_eq!(
            normalize_instance("localhost:8080").unwrap(),
            "http://localhost:8080"
        );
        assert_eq!(
            normalize_instance("127.0.0.1:8080").unwrap(),
            "http://127.0.0.1:8080"
        );
    }

    #[test]
    fn an_address_with_more_than_an_instance_is_refused() {
        for input in [
            "ftp://git.example.com",
            "https://alice:secret@git.example.com",
            "https://git.example.com/some/path",
            "https://git.example.com/?a=b",
            "",
        ] {
            assert!(normalize_instance(input).is_err(), "{input}");
        }
    }

    #[test]
    fn only_plain_http_to_another_machine_counts_as_in_the_clear() {
        assert!(sends_token_in_clear("http://git.example.com"));
        assert!(!sends_token_in_clear("https://git.example.com"));
        assert!(!sends_token_in_clear("http://localhost:8080"));
        assert!(!sends_token_in_clear("http://127.0.0.1:8080"));
    }

    #[tokio::test]
    async fn a_token_in_the_file_reads_back_and_signing_out_removes_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("hosts.json");
        let mut store = Store::load_from(path.clone()).unwrap();

        store
            .sign_in("https://a.example.com", "alice", "fga_a", true)
            .await
            .unwrap();
        store
            .sign_in("https://b.example.com", "alice2", "fga_b", true)
            .await
            .unwrap();

        let mut reloaded = Store::load_from(path).unwrap();
        assert_eq!(reloaded.default_host(), Some("https://a.example.com"));
        assert_eq!(reloaded.hosts().count(), 2);
        let entry = &reloaded.file.hosts["https://b.example.com"];
        assert_eq!(entry.token.as_deref(), Some("fga_b"));

        assert!(reloaded.sign_out("https://a.example.com").await.unwrap());
        assert_eq!(reloaded.default_host(), Some("https://b.example.com"));
        assert!(!reloaded.sign_out("https://a.example.com").await.unwrap());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_hosts_file_is_only_readable_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hosts.json");
        let mut store = Store::load_from(path.clone()).unwrap();

        store
            .sign_in("https://a.example.com", "alice", "fga_a", true)
            .await
            .unwrap();

        let mode = std::fs::metadata(path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
