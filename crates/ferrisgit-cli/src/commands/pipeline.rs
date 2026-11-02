use std::collections::HashMap;
use std::time::Duration;

use crate::cli::PipelineCommand;
use crate::client::Client;
use crate::commands::mr;
use crate::context::{Context, is_full_id, short_id, unique_match};
use crate::error::{CliError, Result};
use crate::model::{MergeRequest, Pipeline, PipelineDetail, is_finished};
use crate::output::{clean, confirm};

/// What the web interface polls at. Each poll carries all the logs, so more often wouldn't be kind to the server.
const POLL_INTERVAL: Duration = Duration::from_secs(3);

pub async fn run(context: &Context, command: PipelineCommand) -> Result<()> {
    match command {
        PipelineCommand::List { limit } => {
            let (client, repo) = context.repo().await?;
            let mut pipelines: Vec<Pipeline> = client.get(&repo.route("/pipelines")).await?;
            pipelines.truncate(limit);
            context.out.result(&pipelines, || {
                let rows: Vec<Vec<String>> = pipelines
                    .iter()
                    .map(|p| {
                        vec![
                            short_id(&p.id),
                            p.status.clone(),
                            p.commit_sha.chars().take(8).collect(),
                            p.created_at.clone(),
                            p.commit_message
                                .as_deref()
                                .and_then(|m| m.lines().next())
                                .unwrap_or("")
                                .to_string(),
                        ]
                    })
                    .collect();
                context.out.table(&rows);
            })
        }
        PipelineCommand::View { id, mr, watch } => {
            let (client, id) = resolve(context, id.as_deref(), mr.as_deref()).await?;
            let detail = if watch {
                wait_until_finished(context, &client, &id).await?
            } else {
                get(&client, &id).await?
            };
            context
                .out
                .result(&detail, || print_detail(context, &detail))?;
            if watch { outcome(&detail) } else { Ok(()) }
        }
        PipelineCommand::Logs {
            id,
            mr,
            job,
            follow,
        } => {
            let (client, id) = resolve(context, id.as_deref(), mr.as_deref()).await?;
            logs(context, &client, &id, job.as_deref(), follow).await
        }
        PipelineCommand::Cancel { id } => {
            let (client, id) = resolve(context, Some(&id), None).await?;
            confirm(
                context.global.yes,
                &format!("Cancel pipeline {}?", short_id(&id)),
            )?;
            client
                .post_empty(&format!("/pipelines/{id}/cancel"))
                .await?;
            context
                .out
                .message(&format!("Cancelled pipeline {}.", short_id(&id)));
            let detail = get(&client, &id).await?;
            context.out.result(&detail.pipeline, || {})
        }
    }
}

async fn get(client: &Client, id: &str) -> Result<PipelineDetail> {
    client.get(&format!("/pipelines/{id}")).await
}

/// From a merge request, an id (whole or a unique start), or the newest pipeline of the repository.
async fn resolve(
    context: &Context,
    id: Option<&str>,
    merge_request: Option<&str>,
) -> Result<(Client, String)> {
    if let Some(reference) = merge_request {
        let (client, mr_id) = mr::resolve_id(context, Some(reference)).await?;
        let found: MergeRequest = client.get(&format!("/merge-requests/{mr_id}")).await?;
        let pipeline = found.pipeline.ok_or_else(|| {
            CliError::NotFound(format!(
                "merge request {} has no pipeline",
                short_id(&mr_id)
            ))
        })?;
        return Ok((client, pipeline.id));
    }
    if let Some(id) = id
        && is_full_id(id)
    {
        return Ok((context.client(None).await?, id.to_lowercase()));
    }
    let (client, repo) = context.repo().await?;
    let pipelines: Vec<Pipeline> = client.get(&repo.route("/pipelines")).await?;
    let id = match id {
        Some(prefix) => unique_match("pipeline", prefix, pipelines.iter().map(|p| p.id.as_str()))?,
        None => pipelines
            .first()
            .map(|p| p.id.clone())
            .ok_or_else(|| CliError::NotFound("this repository has no pipeline".to_string()))?,
    };
    Ok((client, id))
}

async fn wait_until_finished(
    context: &Context,
    client: &Client,
    id: &str,
) -> Result<PipelineDetail> {
    let mut last_status = String::new();
    loop {
        let detail = get(client, id).await?;
        if detail.pipeline.status != last_status {
            context.out.message(&format!(
                "pipeline {}: {}",
                short_id(id),
                detail.pipeline.status
            ));
            last_status.clone_from(&detail.pipeline.status);
        }
        if is_finished(&detail.pipeline.status) {
            return Ok(detail);
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// The result was printed: what's left is telling the script whether it can go on.
fn outcome(detail: &PipelineDetail) -> Result<()> {
    if detail.pipeline.status == "success" {
        return Ok(());
    }
    let why = detail
        .pipeline
        .error
        .as_deref()
        .map_or(String::new(), |error| format!(": {}", clean(error)));
    Err(CliError::Other(format!(
        "pipeline {} ended as {}{why}",
        short_id(&detail.pipeline.id),
        detail.pipeline.status
    )))
}

/// What's new in a log since last time. Logs only grow, except when retention empties one, which starts it over. Byte offsets
/// can fall inside a character only if the log changed under us, and then nothing printable is lost by waiting.
fn new_part<'a>(log: &'a str, seen: &mut usize) -> &'a str {
    if log.len() < *seen {
        *seen = 0;
    }
    match log.get(*seen..) {
        Some(part) => {
            *seen = log.len();
            part
        }
        None => "",
    }
}

async fn logs(
    context: &Context,
    client: &Client,
    id: &str,
    job: Option<&str>,
    follow: bool,
) -> Result<()> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    loop {
        let detail = get(client, id).await?;
        let selected: Vec<_> = detail
            .jobs
            .iter()
            .filter(|j| job.is_none_or(|wanted| j.name == wanted))
            .collect();
        if let Some(job) = job
            && selected.is_empty()
        {
            let finished = is_finished(&detail.pipeline.status);
            if finished || !follow {
                return Err(CliError::NotFound(format!(
                    "no job named {job} in pipeline {}",
                    short_id(id)
                )));
            }
        }

        if context.out.wants_json() {
            // There's no stream to speak of in JSON: the logs of the finished state, once.
            if !follow || is_finished(&detail.pipeline.status) {
                let jobs: Vec<_> = selected
                    .iter()
                    .map(|j| serde_json::json!({ "name": j.name, "status": j.status, "logs": j.logs }))
                    .collect();
                context.out.print_json(&serde_json::Value::Array(jobs))?;
                return if follow { outcome(&detail) } else { Ok(()) };
            }
        } else {
            let several = selected.len() > 1;
            for job in &selected {
                let offset = seen.entry(job.id.clone()).or_insert(0);
                let part = new_part(&job.logs, offset);
                if part.is_empty() {
                    continue;
                }
                let text = context.out.terminal_text(part);
                if several {
                    for line in text.lines() {
                        context.out.line(&format!("[{}] {line}", clean(&job.name)));
                    }
                } else {
                    context.out.raw(&text);
                }
            }
            if !follow {
                return Ok(());
            }
            if is_finished(&detail.pipeline.status) {
                return outcome(&detail);
            }
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

fn print_detail(context: &Context, detail: &PipelineDetail) {
    let pipeline = &detail.pipeline;
    context.out.table(&[
        vec!["id".to_string(), pipeline.id.clone()],
        vec!["status".to_string(), pipeline.status.clone()],
        vec!["commit".to_string(), pipeline.commit_sha.clone()],
        vec!["created".to_string(), pipeline.created_at.clone()],
    ]);
    if let Some(error) = &pipeline.error {
        context.out.line(&format!("error: {}", clean(error)));
    }
    if !detail.jobs.is_empty() {
        context.out.line("");
        let rows: Vec<Vec<String>> = detail
            .jobs
            .iter()
            .map(|job| vec![job.stage.clone(), job.name.clone(), job.status.clone()])
            .collect();
        context.out.table(&rows);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_what_is_new_in_a_log_prints() {
        let mut seen = 0;
        assert_eq!(new_part("one\n", &mut seen), "one\n");
        assert_eq!(new_part("one\n", &mut seen), "");
        assert_eq!(new_part("one\ntwo\n", &mut seen), "two\n");
    }

    #[test]
    fn a_log_emptied_by_retention_starts_over() {
        let mut seen = 8;
        assert_eq!(new_part("", &mut seen), "");
        assert_eq!(new_part("again\n", &mut seen), "again\n");
    }

    #[test]
    fn an_offset_inside_a_character_waits_instead_of_panicking() {
        let mut seen = 1;
        assert_eq!(new_part("é", &mut seen), "");
        assert_eq!(seen, 1);
    }
}
