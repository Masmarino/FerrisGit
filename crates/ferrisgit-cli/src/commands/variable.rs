use serde_json::json;

use crate::cli::VariableCommand;
use crate::context::{Context, read_file_or_stdin};
use crate::error::{CliError, Result};
use crate::model::CiVariable;
use crate::output::{confirm, read_stdin, stdin_is_terminal};

/// `variable` and `secret` are one list on the server, told apart by `masked`: a secret is a variable whose value is masked
/// in job logs. Each command only sees its own, so `secret delete` can't take out a plain variable by mistake.
pub async fn run(context: &Context, command: VariableCommand, masked: bool) -> Result<()> {
    let noun = if masked { "secret" } else { "variable" };
    let (client, repo) = context.repo().await?;
    let route = repo.route("/ci-variables");
    match command {
        VariableCommand::List => {
            let all: Vec<CiVariable> = client.get(&route).await?;
            let listed: Vec<CiVariable> = all.into_iter().filter(|v| v.masked == masked).collect();
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
            let value = read_value(masked, value, value_file.as_deref())?;
            let stored: CiVariable = client
                .post(
                    &route,
                    &json!({ "key": key, "value": value, "masked": masked }),
                )
                .await?;
            context.out.message(&format!("Set the {noun} {key}."));
            context.out.result(&stored, || {})
        }
        VariableCommand::Delete { key } => {
            let all: Vec<CiVariable> = client.get(&route).await?;
            let found = all
                .iter()
                .find(|v| v.key == key && v.masked == masked)
                .ok_or_else(|| CliError::NotFound(format!("no {noun} named {key}")))?;
            confirm(context.global.yes, &format!("Delete the {noun} {key}?"))?;
            client.delete(&format!("{route}/{}", found.id)).await?;
            context.out.message(&format!("Deleted the {noun} {key}."));
            Ok(())
        }
    }
}

/// Exactly what's in the file or on stdin, only without a final line break, which is almost never part of the value.
fn trim_final_line_break(text: String) -> String {
    let trimmed = text.strip_suffix('\n').unwrap_or(&text);
    trimmed.strip_suffix('\r').unwrap_or(trimmed).to_string()
}

fn read_value(
    masked: bool,
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
    if masked {
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
