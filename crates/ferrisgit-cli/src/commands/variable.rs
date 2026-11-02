use serde_json::json;

use crate::cli::VariableCommand;
use crate::client::Client;
use crate::context::{Context, Repo, read_file_or_stdin};
use crate::error::{CliError, Result};
use crate::model::{CiSecret, CiVariable};
use crate::output::{confirm, read_stdin, stdin_is_terminal};

/// The plain ones. The server's list holds secrets too, and refuses to make one plain by accident.
pub async fn run_variable(context: &Context, command: VariableCommand) -> Result<()> {
    let (client, repo) = context.repo().await?;
    let route = repo.route("/ci-variables");
    match command {
        VariableCommand::List => {
            let all: Vec<CiVariable> = client.get(&route).await?;
            let listed: Vec<CiVariable> = all.into_iter().filter(|v| !v.masked).collect();
            context.out.result(&listed, || {
                let rows: Vec<Vec<String>> = listed.iter().map(|v| vec![v.key.clone()]).collect();
                context.out.table(&rows);
            })
        }
        VariableCommand::Set {
            key,
            value,
            value_file,
        } => {
            let value = read_value(false, value, value_file.as_deref())?;
            let stored: CiVariable = client
                .post(
                    &route,
                    &json!({ "key": key, "value": value, "masked": false }),
                )
                .await?;
            context.out.message(&format!("Set the variable {key}."));
            context.out.result(&stored, || {})
        }
        VariableCommand::Delete { key } => {
            confirm(context.global.yes, &format!("Delete the variable {key}?"))?;
            let all: Vec<CiVariable> = client.get(&route).await?;
            let found = all
                .iter()
                .find(|v| v.key == key && !v.masked)
                .ok_or_else(|| CliError::NotFound(format!("no variable named {key}")))?;
            client.delete(&format!("{route}/{}", found.id)).await?;
            context.out.message(&format!("Deleted the variable {key}."));
            Ok(())
        }
    }
}

pub async fn run_secret(context: &Context, command: VariableCommand) -> Result<()> {
    let (client, repo) = context.repo().await?;
    match command {
        VariableCommand::List => {
            let secrets: Vec<CiSecret> = client.get(&repo.route("/ci-secrets")).await?;
            context.out.result(&secrets, || {
                let rows: Vec<Vec<String>> = secrets.iter().map(|s| vec![s.key.clone()]).collect();
                context.out.table(&rows);
            })
        }
        VariableCommand::Set {
            key,
            value,
            value_file,
        } => {
            let value = read_value(true, value, value_file.as_deref())?;
            let stored = put_secret(&client, &repo, &key, &value).await?;
            context.out.message(&format!("Set the secret {key}."));
            context.out.result(&stored, || {})
        }
        VariableCommand::Delete { key } => {
            confirm(context.global.yes, &format!("Delete the secret {key}?"))?;
            client.delete(&secret_route(&repo, &key)).await?;
            context.out.message(&format!("Deleted the secret {key}."));
            Ok(())
        }
    }
}

fn secret_route(repo: &Repo, key: &str) -> String {
    let encoded: String = url::form_urlencoded::byte_serialize(key.as_bytes()).collect();
    repo.route(&format!("/ci-secrets/{encoded}"))
}

async fn put_secret(client: &Client, repo: &Repo, key: &str, value: &str) -> Result<CiSecret> {
    client
        .put(&secret_route(repo, key), &json!({ "value": value }))
        .await
}

/// Exactly what's in the file or on stdin, only without a final line break, which is almost never part of the value.
fn trim_final_line_break(text: String) -> String {
    let trimmed = text.strip_suffix('\n').unwrap_or(&text);
    trimmed.strip_suffix('\r').unwrap_or(trimmed).to_string()
}

fn read_value(
    hidden: bool,
    value: Option<String>,
    file: Option<&std::path::Path>,
) -> Result<String> {
    if let Some(value) = value {
        return Ok(value);
    }
    if let Some(file) = file {
        return read_file_or_stdin(file).map(trim_final_line_break);
    }
    if !stdin_is_terminal() {
        return read_stdin().map(trim_final_line_break);
    }
    if hidden {
        rpassword::prompt_password("Value: ")
            .map_err(|e| CliError::Other(format!("could not read the value: {e}")))
    } else {
        crate::output::required(None, "--value", "Value")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_final_line_break_is_dropped() {
        assert_eq!(trim_final_line_break("a\n".to_string()), "a");
        assert_eq!(trim_final_line_break("a\r\n".to_string()), "a");
        assert_eq!(trim_final_line_break("a\nb\n\n".to_string()), "a\nb\n");
        assert_eq!(trim_final_line_break("a".to_string()), "a");
    }
}
