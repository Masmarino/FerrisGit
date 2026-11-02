use serde_json::{Map, Value, json};

use crate::cli::{MrCommand, MrRef, StateFilter};
use crate::client::Client;
use crate::context::{Context, Repo, is_full_id, short_id, unique_match};
use crate::error::{CliError, Result};
use crate::git;
use crate::model::{Branch, Comment, DiffFile, MergeOutcome, MergeRequest, Review, UserRef};
use crate::output::{clean, clean_keeping_lines, confirm, required};

fn matches_state(mr: &MergeRequest, state: StateFilter) -> bool {
    match state {
        StateFilter::All => true,
        StateFilter::Open => mr.status == "open",
        StateFilter::Merged => mr.status == "merged",
        StateFilter::Closed => mr.status == "closed",
    }
}

fn username(user: &Option<UserRef>) -> String {
    user.as_ref()
        .map_or_else(String::new, |u| u.username.clone())
}

/// The id, from a whole one (no repository needed), a start of one, or the open merge request of the current branch.
pub async fn resolve_id(context: &Context, reference: Option<&str>) -> Result<(Client, String)> {
    if let Some(id) = reference
        && is_full_id(id)
    {
        return Ok((context.client(None).await?, id.to_lowercase()));
    }
    let (client, repo) = context.repo().await?;
    let id = resolve_in_repo(&client, &repo, reference).await?;
    Ok((client, id))
}

async fn resolve_in_repo(client: &Client, repo: &Repo, reference: Option<&str>) -> Result<String> {
    let all: Vec<MergeRequest> = client.get(&repo.route("/merge-requests")).await?;
    if let Some(prefix) = reference {
        return unique_match("merge request", prefix, all.iter().map(|mr| mr.id.as_str()));
    }
    let branch = git::current_branch().await.map_err(|_| {
        CliError::Usage("which merge request? Give its id, or run this on its branch".to_string())
    })?;
    let mut open = all
        .iter()
        .filter(|mr| mr.status == "open" && mr.source_branch == branch);
    let Some(first) = open.next() else {
        return Err(CliError::NotFound(format!(
            "no open merge request from the branch {branch}"
        )));
    };
    if open.next().is_some() {
        return Err(CliError::Usage(format!(
            "several merge requests are open from {branch}: give the id of one"
        )));
    }
    Ok(first.id.clone())
}

pub async fn run(context: &Context, command: MrCommand) -> Result<()> {
    match command {
        MrCommand::List { state } => {
            let (client, repo) = context.repo().await?;
            let all: Vec<MergeRequest> = client.get(&repo.route("/merge-requests")).await?;
            let listed: Vec<MergeRequest> = all
                .into_iter()
                .filter(|mr| matches_state(mr, state))
                .collect();
            context.out.result(&listed, || {
                let rows: Vec<Vec<String>> = listed
                    .iter()
                    .map(|mr| {
                        vec![
                            short_id(&mr.id),
                            mr.status.clone(),
                            format!("{} → {}", mr.source_branch, mr.target_branch),
                            mr.title.clone(),
                        ]
                    })
                    .collect();
                context.out.table(&rows);
            })
        }
        MrCommand::View { mr, comments } => view(context, mr, comments).await,
        MrCommand::Create {
            title,
            body,
            source,
            target,
            push,
        } => {
            let title = required(title, "--title", "Title")?;
            let description = body.read()?.unwrap_or_default();
            let (client, repo) = context.repo().await?;
            let source = match source {
                Some(source) => source,
                None => git::current_branch().await?,
            };
            if push {
                push_branch(&client, &repo, &source).await?;
            }
            let branches: Vec<Branch> = client.get(&repo.route("/branches")).await?;
            if !branches.iter().any(|branch| branch.name == source) {
                return Err(CliError::Refused(format!(
                    "the branch {source} isn't on the server. Push it first, or pass --push"
                )));
            }
            let target = match target {
                Some(target) => target,
                None => branches
                    .iter()
                    .find(|branch| branch.is_default)
                    .map(|branch| branch.name.clone())
                    .ok_or_else(|| {
                        CliError::Usage(
                            "the repository has no default branch: name one with --target"
                                .to_string(),
                        )
                    })?,
            };
            let created: MergeRequest = client
                .post(
                    &repo.route("/merge-requests"),
                    &json!({
                        "sourceBranch": source,
                        "targetBranch": target,
                        "title": title,
                        "description": description,
                    }),
                )
                .await?;
            context.out.message(&format!(
                "Opened merge request {} ({source} → {target}).",
                short_id(&created.id)
            ));
            context
                .out
                .result(&created, || context.out.line(&created.id))
        }
        MrCommand::Checkout { id } => checkout(context, &id).await,
        MrCommand::Diff { mr } => {
            let (client, id) = resolve_id(context, mr.id.as_deref()).await?;
            let files: Vec<DiffFile> = client.get(&format!("/merge-requests/{id}/diff")).await?;
            context.out.result(&files, || print_diff(context, &files))
        }
        MrCommand::Comment { mr, body } => {
            let text = required(body.read()?, "--body or --body-file", "Comment")?;
            let (client, id) = resolve_id(context, mr.id.as_deref()).await?;
            let comment: Comment = client
                .post(
                    &format!("/merge-requests/{id}/comments"),
                    &json!({ "body": text }),
                )
                .await?;
            context
                .out
                .message(&format!("Commented on merge request {}.", short_id(&id)));
            context.out.result(&comment, || {})
        }
        MrCommand::Review {
            mr,
            approve,
            request_changes: _,
            body,
        } => {
            let text = body.read()?;
            let (client, id) = resolve_id(context, mr.id.as_deref()).await?;
            if let Some(text) = text {
                let _: Comment = client
                    .post(
                        &format!("/merge-requests/{id}/comments"),
                        &json!({ "body": text }),
                    )
                    .await?;
            }
            let decision = if approve {
                "approved"
            } else {
                "changes_requested"
            };
            let review: Review = client
                .post(
                    &format!("/merge-requests/{id}/reviews"),
                    &json!({ "decision": decision }),
                )
                .await?;
            context.out.message(&format!(
                "Reviewed merge request {}: {decision}.",
                short_id(&id)
            ));
            context.out.result(&review, || {})
        }
        MrCommand::Merge { mr, method } => {
            let (client, id) = resolve_id(context, mr.id.as_deref()).await?;
            confirm(
                context.global.yes,
                &format!("Merge merge request {}?", short_id(&id)),
            )?;
            let mut body = Map::new();
            if let Some(method) = method {
                body.insert("method".to_string(), json!(method.as_str()));
            }
            let outcome: MergeOutcome = client
                .post(&format!("/merge-requests/{id}/merge"), &Value::Object(body))
                .await?;
            let Some(merged) = outcome
                .merge_request
                .filter(|_| outcome.outcome == "merged")
            else {
                return Err(CliError::Refused(format!(
                    "merge request {} can't be merged: its branches conflict",
                    short_id(&id)
                )));
            };
            context
                .out
                .message(&format!("Merged merge request {}.", short_id(&id)));
            context.out.result(&merged, || {})
        }
        MrCommand::Close { mr } => {
            let (client, id) = resolve_id(context, mr.id.as_deref()).await?;
            confirm(
                context.global.yes,
                &format!("Close merge request {}?", short_id(&id)),
            )?;
            client
                .post_empty(&format!("/merge-requests/{id}/close"))
                .await?;
            context
                .out
                .message(&format!("Closed merge request {}.", short_id(&id)));
            let closed: MergeRequest = client.get(&format!("/merge-requests/{id}")).await?;
            context.out.result(&closed, || {})
        }
    }
}

async fn view(context: &Context, mr: MrRef, comments: bool) -> Result<()> {
    let (client, id) = resolve_id(context, mr.id.as_deref()).await?;
    let found: MergeRequest = client.get(&format!("/merge-requests/{id}")).await?;
    let listed: Vec<Comment> = if comments {
        client
            .get(&format!("/merge-requests/{id}/comments"))
            .await?
    } else {
        vec![]
    };
    if context.out.wants_json() {
        let mut value = serde_json::to_value(&found).map_err(|e| CliError::Other(e.to_string()))?;
        if comments {
            value["comments"] =
                serde_json::to_value(&listed).map_err(|e| CliError::Other(e.to_string()))?;
        }
        return context.out.print_json(&value);
    }
    context
        .out
        .line(&context.out.paint("1", &clean(&found.title)));
    let pipeline = found.pipeline.as_ref().map_or_else(
        || "none".to_string(),
        |p| format!("{} ({})", p.status, short_id(&p.id)),
    );
    context.out.table(&[
        vec!["id".to_string(), found.id.clone()],
        vec!["status".to_string(), found.status.clone()],
        vec![
            "branches".to_string(),
            format!("{} → {}", found.source_branch, found.target_branch),
        ],
        vec!["author".to_string(), username(&found.author)],
        vec!["pipeline".to_string(), pipeline],
        vec!["comments".to_string(), found.comment_count.to_string()],
    ]);
    if !found.description.is_empty() {
        context.out.line("");
        context.out.line(&clean_keeping_lines(&found.description));
    }
    for comment in &listed {
        context.out.line("");
        context.out.line(&context.out.paint(
            "2",
            &format!(
                "{} on {}",
                clean(&username(&comment.author)),
                comment.created_at
            ),
        ));
        context.out.line(&clean_keeping_lines(&comment.body));
    }
    Ok(())
}

/// Through `origin` when that's the instance being talked to, for the upstream it sets. Otherwise straight to the address.
async fn push_branch(client: &Client, repo: &Repo, branch: &str) -> Result<()> {
    let instance = client.instance();
    let mut args = git::credential_options(instance)?;
    args.push("push".to_string());
    if git::origin_instance().await.as_deref() == Some(instance) {
        args.extend(["-u".to_string(), "origin".to_string()]);
        args.push(format!("{branch}:{branch}"));
    } else {
        args.push(git::clone_url(instance, &repo.path));
        args.push(format!("{branch}:refs/heads/{branch}"));
    }
    git::run_visible(&args).await
}

async fn checkout(context: &Context, id: &str) -> Result<()> {
    let (client, repo) = context.repo().await?;
    let id = if is_full_id(id) {
        id.to_lowercase()
    } else {
        resolve_in_repo(&client, &repo, Some(id)).await?
    };
    let found: MergeRequest = client.get(&format!("/merge-requests/{id}")).await?;
    let branch = found.source_branch;
    let instance = client.instance();

    let mut fetch = git::credential_options(instance)?;
    fetch.extend([
        "fetch".to_string(),
        git::clone_url(instance, &repo.path),
        format!("+refs/heads/{branch}:refs/remotes/origin/{branch}"),
    ]);
    git::run_visible(&fetch).await?;
    git::run_visible(&["switch".to_string(), "--".to_string(), branch.clone()]).await?;
    context.out.message(&format!("Now on {branch}."));
    Ok(())
}

fn print_diff(context: &Context, files: &[DiffFile]) {
    for file in files {
        let path = clean(&file.path);
        context.out.line(
            &context
                .out
                .paint("1", &format!("diff --git a/{path} b/{path}")),
        );
        if file.change == "binary" {
            context.out.line("Binary file changed");
            continue;
        }
        for hunk in &file.hunks {
            let old_start = hunk.rows.iter().find_map(|row| row.old_line).unwrap_or(0);
            let new_start = hunk.rows.iter().find_map(|row| row.new_line).unwrap_or(0);
            context.out.line(
                &context
                    .out
                    .paint("36", &format!("@@ -{old_start} +{new_start} @@")),
            );
            for row in &hunk.rows {
                for line in unified_lines(row) {
                    context.out.line(&match line.chars().next() {
                        Some('-') => context.out.paint("31", &line),
                        Some('+') => context.out.paint("32", &line),
                        _ => line,
                    });
                }
            }
        }
    }
}

/// The server's rows are side by side: a modified one is two lines of a unified diff.
fn unified_lines(row: &crate::model::DiffRow) -> Vec<String> {
    let old = clean(row.old_content.as_deref().unwrap_or(""));
    let new = clean(row.new_content.as_deref().unwrap_or(""));
    match row.kind.as_str() {
        "removed" => vec![format!("-{old}")],
        "added" => vec![format!("+{new}")],
        "modified" => vec![format!("-{old}"), format!("+{new}")],
        _ => vec![format!(" {new}")],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DiffRow;

    fn row(kind: &str, old: Option<&str>, new: Option<&str>) -> DiffRow {
        DiffRow {
            old_line: Some(1),
            old_content: old.map(str::to_string),
            new_line: Some(1),
            new_content: new.map(str::to_string),
            kind: kind.to_string(),
        }
    }

    #[test]
    fn a_side_by_side_row_reads_as_unified_lines() {
        assert_eq!(unified_lines(&row("context", Some("a"), Some("a"))), [" a"]);
        assert_eq!(unified_lines(&row("removed", Some("a"), None)), ["-a"]);
        assert_eq!(unified_lines(&row("added", None, Some("b"))), ["+b"]);
        assert_eq!(
            unified_lines(&row("modified", Some("a"), Some("b"))),
            ["-a", "+b"]
        );
    }
}
