use reqwest::Method;
use serde_json::{Map, Value};

use crate::cli::ApiArgs;
use crate::context::{Context, read_file_or_stdin};
use crate::error::{CliError, Result};

/// Only a path: a whole URL could carry the token to another host.
fn api_path(path: &str) -> Result<String> {
    if path.contains("://") || path.starts_with("//") {
        return Err(CliError::Usage(
            "give a path below /api, not a whole address. --hostname chooses the instance"
                .to_string(),
        ));
    }
    let path = path
        .strip_prefix("/api/")
        .map_or_else(|| path.trim_start_matches('/').to_string(), str::to_string);
    Ok(format!("/{path}"))
}

fn split_field(field: &str) -> Result<(&str, &str)> {
    field
        .split_once('=')
        .filter(|(key, _)| !key.is_empty())
        .ok_or_else(|| CliError::Usage(format!("expected key=value, got {field}")))
}

/// `--field` reads JSON and falls back on a string, `--raw-field` is always a string.
fn body_from_fields(raw: &[String], typed: &[String]) -> Result<Option<Value>> {
    if raw.is_empty() && typed.is_empty() {
        return Ok(None);
    }
    let mut body = Map::new();
    for field in raw {
        let (key, value) = split_field(field)?;
        body.insert(key.to_string(), Value::String(value.to_string()));
    }
    for field in typed {
        let (key, value) = split_field(field)?;
        let parsed =
            serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()));
        body.insert(key.to_string(), parsed);
    }
    Ok(Some(Value::Object(body)))
}

pub async fn run(context: &Context, args: ApiArgs) -> Result<()> {
    let path = api_path(&args.path)?;
    let client = context.client(args.hostname.as_deref()).await?;

    let fields = body_from_fields(&args.raw_fields, &args.fields)?;
    let input = args.input.as_deref().map(read_file_or_stdin).transpose()?;
    let has_body = fields.is_some() || input.is_some();
    let method = match &args.method {
        Some(method) => Method::from_bytes(method.to_uppercase().as_bytes())
            .map_err(|_| CliError::Usage(format!("not an HTTP method: {method}")))?,
        None if has_body => Method::POST,
        None => Method::GET,
    };

    let mut request = client.request(method, &path);
    if let Some(body) = &fields {
        request = request.json(body);
    } else if let Some(body) = input {
        request = request
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body);
    }
    let response = client.send(request).await?;
    let text = response
        .text()
        .await
        .map_err(|e| CliError::Server(format!("the answer stopped: {e}")))?;
    if text.is_empty() {
        return Ok(());
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(value) => context.out.print_json(&value),
        Err(_) if context.out.wants_json() => Err(CliError::Usage(
            "this route's answer isn't JSON, so --json and --jq don't apply".to_string(),
        )),
        Err(_) => {
            context.out.raw(&text);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_path_is_taken_with_or_without_its_prefix() {
        assert_eq!(api_path("/auth/me").unwrap(), "/auth/me");
        assert_eq!(api_path("auth/me").unwrap(), "/auth/me");
        assert_eq!(api_path("/api/auth/me").unwrap(), "/auth/me");
    }

    #[test]
    fn a_whole_address_is_refused_so_the_token_stays_home() {
        assert!(api_path("https://evil.example.com/x").is_err());
        assert!(api_path("//evil.example.com/x").is_err());
    }

    #[test]
    fn fields_are_strings_or_json_as_asked() {
        let body = body_from_fields(
            &["name=12".to_string()],
            &[
                "count=12".to_string(),
                "on=true".to_string(),
                "none=null".to_string(),
                "text=plain words".to_string(),
                "list=[\"a\"]".to_string(),
            ],
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            body,
            json!({
                "name": "12",
                "count": 12,
                "on": true,
                "none": null,
                "text": "plain words",
                "list": ["a"],
            })
        );
    }

    #[test]
    fn a_field_without_a_key_is_refused() {
        assert!(body_from_fields(&["novalue".to_string()], &[]).is_err());
        assert!(body_from_fields(&["=value".to_string()], &[]).is_err());
    }
}
