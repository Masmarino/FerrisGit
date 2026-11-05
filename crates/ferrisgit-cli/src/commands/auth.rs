use std::io::Write;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::json;

use crate::cli::{AuthCommand, HostArgs, LoginArgs, LogoutArgs};
use crate::client::{Client, error_for, error_message};
use crate::config::{
    HOST_ENV, TOKEN_ENV, TokenSource, env_host, normalize_instance, sends_token_in_clear,
};
use crate::context::Context;
use crate::error::{CliError, Result};
use crate::git;
use crate::model::{CreatedToken, DeviceStart, Me};
use crate::output::{clean, read_stdin, stdin_is_terminal};

/// What a person signing in needs day to day. `admin` has to be asked for by name.
const DEFAULT_SCOPES: [&str; 5] = [
    "repositories:write",
    "issues:write",
    "merge-requests:write",
    "pipelines:write",
    "releases:write",
];

pub async fn run(context: &mut Context, command: AuthCommand) -> Result<()> {
    match command {
        AuthCommand::Login(args) => login(context, args).await,
        AuthCommand::Logout(args) => logout(context, args).await,
        AuthCommand::Status => status(context).await,
        AuthCommand::Token(args) => token(context, args).await,
        AuthCommand::SetupGit(args) => setup_git(context, args).await,
        AuthCommand::GitCredential { action } => git_credential(context, &action).await,
    }
}

/// Signing in doesn't need a Git remote to be on the right track, but is glad to take one.
async fn login_instance(context: &Context, hostname: Option<&str>) -> Result<String> {
    match context.instance(hostname).await {
        Ok(instance) => Ok(instance),
        Err(CliError::Auth(_)) => Err(CliError::Usage(
            "which instance? Name it with --hostname <host>".to_string(),
        )),
        Err(e) => Err(e),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SignedIn {
    host: String,
    username: String,
    source: TokenSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    scopes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<String>,
}

pub async fn login(context: &mut Context, args: LoginArgs) -> Result<()> {
    let instance = login_instance(context, args.hostname.as_deref()).await?;
    if sends_token_in_clear(&instance) {
        context.out.message(&format!(
            "warning: {instance} is plain HTTP, so the token can be read on the way"
        ));
    }
    if std::env::var_os(TOKEN_ENV).is_some() {
        context.out.message(&format!(
            "warning: {TOKEN_ENV} is set and takes precedence over what you are storing"
        ));
    }

    let (token, scopes, expires_at) = if args.with_token {
        (read_token_from_stdin()?, None, None)
    } else {
        if !args.web && !stdin_is_terminal() {
            return Err(CliError::Usage(format!(
                "no terminal to sign in from: use --with-token, set {TOKEN_ENV}, or pass --web to wait for someone to \
                 confirm in a browser"
            )));
        }
        let created = device_login(context, &instance, &args).await?;
        (
            created.token,
            Some(created.scopes),
            Some(created.expires_at),
        )
    };

    // Also what tells a bad token from --with-token before it gets stored.
    let client = Client::new(&instance, Some(token.clone()))?;
    let me: Me = client.get("/auth/me").await?;
    let source = context
        .store
        .sign_in(&instance, &me.username, &token, args.insecure_storage)
        .await?;

    context.out.message(&format!(
        "Signed in to {instance} as {}.",
        clean(&me.username)
    ));
    let signed_in = SignedIn {
        host: instance,
        username: me.username,
        source,
        scopes,
        expires_at,
    };
    context.out.result(&signed_in, || {})
}

fn read_token_from_stdin() -> Result<String> {
    if stdin_is_terminal() {
        return rpassword::prompt_password("Token: ")
            .map(|token| token.trim().to_string())
            .map_err(|e| CliError::Other(format!("could not read the token: {e}")));
    }
    let token = read_stdin()?.trim().to_string();
    if token.is_empty() {
        return Err(CliError::Usage(
            "--with-token reads a token from stdin, and stdin was empty".to_string(),
        ));
    }
    Ok(token)
}

fn client_name() -> String {
    let host = ["HOSTNAME", "COMPUTERNAME"]
        .iter()
        .find_map(|name| std::env::var(name).ok())
        .map(|host| clean(&host))
        .filter(|host| !host.trim().is_empty());
    let name = match host {
        Some(host) => format!("fgit on {host}"),
        None => "fgit".to_string(),
    };
    name.chars().take(100).collect()
}

async fn device_login(context: &Context, instance: &str, args: &LoginArgs) -> Result<CreatedToken> {
    let client = Client::new(instance, None)?;
    let scopes: Vec<String> = if args.scopes.is_empty() {
        DEFAULT_SCOPES.iter().map(ToString::to_string).collect()
    } else {
        args.scopes.clone()
    };
    let started: DeviceStart = client
        .post(
            "/auth/device/start",
            &json!({
                "clientName": client_name(),
                "scopes": scopes,
                "expiresInDays": args.expires_in_days,
            }),
        )
        .await?;

    context.out.message(&format!(
        "Confirm this code in your browser: {}\n  {}",
        clean(&started.user_code),
        clean(&started.verification_uri_complete)
    ));
    if !args.no_browser && stdin_is_terminal() {
        // Only an address on the instance being signed in to. A failure isn't one: the address is printed above.
        let same_instance = started
            .verification_uri_complete
            .starts_with(&format!("{instance}/"));
        if same_instance && open::that(&started.verification_uri_complete).is_err() {
            context
                .out
                .message("Could not open a browser: open the address above yourself.");
        }
    }
    context.out.message("Waiting for your confirmation…");
    let _ = std::io::stderr().flush();

    poll_device(&client, &started).await
}

async fn poll_device(client: &Client, started: &DeviceStart) -> Result<CreatedToken> {
    let deadline = Instant::now() + Duration::from_secs(started.expires_in);
    let mut interval = Duration::from_secs(started.interval.max(1));
    loop {
        tokio::time::sleep(interval).await;
        if Instant::now() > deadline {
            return Err(expired());
        }
        let response = client
            .request(reqwest::Method::POST, "/auth/device/token")
            .json(&json!({ "deviceCode": started.device_code }))
            .timeout(Duration::from_secs(30))
            .send()
            .await;
        let response = match response {
            Ok(response) => response,
            // The wait is long enough to outlast a network blip.
            Err(_) => continue,
        };
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if status.is_success() {
            return serde_json::from_str(&body)
                .map_err(|e| CliError::Server(format!("unexpected answer: {e}")));
        }
        let message = error_message(&body);
        match message.as_str() {
            "authorization_pending" => {}
            "slow_down" => interval += Duration::from_secs(5),
            "access_denied" => {
                return Err(CliError::Auth("the sign-in was refused".to_string()));
            }
            "expired_token" => return Err(expired()),
            _ if status.is_server_error() => {}
            _ => return Err(error_for(status, message)),
        }
    }
}

fn expired() -> CliError {
    CliError::Auth(
        "the code expired before it was confirmed. Run `fgit auth login` again".to_string(),
    )
}

pub async fn logout(context: &mut Context, args: LogoutArgs) -> Result<()> {
    let instance = match args.hostname.as_deref() {
        Some(hostname) => normalize_instance(hostname)?,
        None => context.instance(None).await?,
    };
    if !context.store.sign_out(&instance).await? {
        return Err(CliError::NotFound(format!("not signed in to {instance}")));
    }
    context.out.message(&format!(
        "Forgot {instance} on this machine. The token keeps working until it expires: revoke it in Mon compte to end it now."
    ));
    if std::env::var_os(TOKEN_ENV).is_some() {
        context.out.message(&format!("{TOKEN_ENV} is still set."));
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HostStatus {
    host: String,
    username: Option<String>,
    source: TokenSource,
    valid: bool,
    /// Why it isn't valid. Empty otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    is_default: bool,
}

async fn status(context: &Context) -> Result<()> {
    let mut instances: Vec<String> = context
        .store
        .hosts()
        .map(|(host, _)| host.clone())
        .collect();
    if std::env::var_os(TOKEN_ENV).is_some() {
        let from_env = match env_host()? {
            Some(host) => Some(host),
            None => git::repository_from_remotes()
                .await
                .and_then(|r| r.instance),
        };
        if let Some(host) = from_env
            && !instances.contains(&host)
        {
            instances.push(host);
        }
    }

    let mut statuses = Vec::new();
    for host in instances {
        let Some((token, source)) = context.store.token_for(&host).await? else {
            continue;
        };
        let me: Result<Me> = async { Client::new(&host, Some(token))?.get("/auth/me").await }.await;
        let is_default = context.store.default_host() == Some(host.as_str());
        statuses.push(match me {
            Ok(me) => HostStatus {
                host,
                username: Some(me.username),
                source,
                valid: true,
                error: None,
                is_default,
            },
            Err(e) => HostStatus {
                host,
                username: None,
                source,
                valid: false,
                error: Some(e.to_string()),
                is_default,
            },
        });
    }

    if statuses.is_empty() && !context.out.wants_json() {
        return Err(CliError::Auth(format!(
            "not signed in anywhere. Run `fgit auth login`, or set {TOKEN_ENV} and {HOST_ENV}"
        )));
    }
    let all_valid = statuses.iter().all(|s| s.valid);
    context.out.result(&statuses, || {
        for s in &statuses {
            let default = if s.is_default { " (default)" } else { "" };
            match (&s.username, &s.error) {
                (Some(username), _) => context.out.line(&format!(
                    "{}{default}: signed in as {} ({:?})",
                    s.host,
                    clean(username),
                    s.source
                )),
                (None, error) => context.out.line(&format!(
                    "{}{default}: {}",
                    s.host,
                    error.as_deref().unwrap_or("invalid token")
                )),
            }
        }
    })?;
    if all_valid {
        Ok(())
    } else {
        Err(CliError::Auth(
            "at least one token doesn't work".to_string(),
        ))
    }
}

async fn token(context: &Context, args: HostArgs) -> Result<()> {
    let instance = context.instance(args.hostname.as_deref()).await?;
    let (token, _) = context
        .store
        .token_for(&instance)
        .await?
        .ok_or_else(|| CliError::Auth(format!("not signed in to {instance}")))?;
    context.out.line(&token);
    Ok(())
}

async fn setup_git(context: &Context, args: HostArgs) -> Result<()> {
    let instances: Vec<String> = match args.hostname.as_deref() {
        Some(hostname) => vec![normalize_instance(hostname)?],
        None => context
            .store
            .hosts()
            .map(|(host, _)| host.clone())
            .collect(),
    };
    if instances.is_empty() {
        return Err(CliError::Auth(
            "not signed in anywhere. Run `fgit auth login` first".to_string(),
        ));
    }
    for instance in &instances {
        git::configure_helper(&[], "--global", instance).await?;
        context.out.message(&format!(
            "Git now asks fgit for its credentials on {instance}."
        ));
    }
    Ok(())
}

/// What Git says it wants: only the instance matters here, the rest of its input is ignored. It may also be a request for
/// another host, if a helper configured without a scope calls here: that gets nothing.
fn instance_from_credential_request(input: &str) -> Option<String> {
    let mut protocol = None;
    let mut host = None;
    for line in input.lines() {
        match line.split_once('=') {
            Some(("protocol", value)) => protocol = Some(value),
            Some(("host", value)) => host = Some(value),
            _ => {}
        }
    }
    normalize_instance(&format!("{}://{}", protocol?, host?)).ok()
}

async fn git_credential(context: &Context, action: &str) -> Result<()> {
    // Git also says `store` and `erase`, about credentials it thinks it holds. There's nothing to do for those.
    if action != "get" {
        let _ = read_stdin();
        return Ok(());
    }
    let Some(instance) = instance_from_credential_request(&read_stdin()?) else {
        return Ok(());
    };
    // Answering nothing lets Git go on to whatever other helper it has.
    if let Ok(Some((token, _))) = context.store.token_for(&instance).await {
        context
            .out
            .raw(&format!("username=fgit\npassword={token}\n"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_credential_request_names_its_instance() {
        assert_eq!(
            instance_from_credential_request("protocol=https\nhost=git.example.com\n\n").as_deref(),
            Some("https://git.example.com")
        );
        assert_eq!(
            instance_from_credential_request("protocol=http\nhost=localhost:8080\npath=a/b.git\n")
                .as_deref(),
            Some("http://localhost:8080")
        );
    }

    #[test]
    fn a_request_without_a_host_names_nothing() {
        assert_eq!(instance_from_credential_request("protocol=https\n"), None);
        assert_eq!(
            instance_from_credential_request("protocol=ssh\nhost=git.example.com\n"),
            None
        );
    }

    #[test]
    fn the_default_scopes_leave_out_admin() {
        assert!(
            !DEFAULT_SCOPES
                .iter()
                .any(|scope| scope.starts_with("admin"))
        );
    }
}
