use serde_json::json;

use crate::cli::{IssueCommand, StateFilter};
use crate::client::Client;
use crate::context::{Context, Repo};
use crate::error::{CliError, Result};
use crate::model::{Collaborator, Comment, Issue, Label, Milestone, UserSearch};
use crate::output::{clean, clean_keeping_lines, required};

fn matches_state(issue: &Issue, state: StateFilter) -> bool {
    match state {
        StateFilter::All => true,
        StateFilter::Open => issue.status != "done",
        // An issue has no merged state, so that filter reads as closed too.
        StateFilter::Closed | StateFilter::Merged => issue.status == "done",
    }
}

fn username(user: &Option<crate::model::UserRef>) -> String {
    user.as_ref()
        .map_or_else(String::new, |u| u.username.clone())
}

pub async fn run(context: &Context, command: IssueCommand) -> Result<()> {
    let (client, repo) = context.repo().await?;
    let client = &client;
    let repo = &repo;
    match command {
        IssueCommand::List {
            state,
            label,
            assignee,
        } => {
            let issues: Vec<Issue> = client.get(&repo.route("/issues")).await?;
            let issues: Vec<Issue> = issues
                .into_iter()
                .filter(|issue| matches_state(issue, state))
                .filter(|issue| {
                    label
                        .iter()
                        .all(|wanted| issue.labels.iter().any(|l| &l.name == wanted))
                })
                .filter(|issue| {
                    assignee.as_ref().is_none_or(|wanted| {
                        issue
                            .assignee
                            .as_ref()
                            .is_some_and(|a| &a.username == wanted)
                    })
                })
                .collect();
            context.out.result(&issues, || {
                let rows: Vec<Vec<String>> = issues
                    .iter()
                    .map(|issue| {
                        vec![
                            format!("#{}", issue.number),
                            issue.status.clone(),
                            issue.kind.clone(),
                            username(&issue.assignee),
                            issue.title.clone(),
                        ]
                    })
                    .collect();
                context.out.table(&rows);
            })
        }
        IssueCommand::View { number, comments } => {
            let issue: Issue = get_issue(client, repo, number).await?;
            let listed: Vec<Comment> = if comments {
                client
                    .get(&repo.route(&format!("/issues/{number}/comments")))
                    .await?
            } else {
                vec![]
            };
            if context.out.wants_json() {
                let mut value =
                    serde_json::to_value(&issue).map_err(|e| CliError::Other(e.to_string()))?;
                if comments {
                    value["comments"] = serde_json::to_value(&listed)
                        .map_err(|e| CliError::Other(e.to_string()))?;
                }
                return context.out.print_json(&value);
            }
            print_issue(context, &issue);
            print_comments(context, &listed);
            Ok(())
        }
        IssueCommand::Create { title, body, kind } => {
            let title = required(title, "--title", "Title")?;
            let description = body.read()?.unwrap_or_default();
            let issue: Issue = client
                .post(
                    &repo.route("/issues"),
                    &json!({ "title": title, "description": description, "kind": kind.as_str() }),
                )
                .await?;
            finished(context, &issue, "Opened")
        }
        IssueCommand::Edit {
            number,
            title,
            body,
            kind,
            status,
        } => {
            let new_body = body.read()?;
            if title.is_none() && new_body.is_none() && kind.is_none() && status.is_none() {
                return Err(CliError::Usage(
                    "nothing to change: give --title, --body, --kind or --status".to_string(),
                ));
            }
            let mut issue = get_issue(client, repo, number).await?;
            if title.is_some() || new_body.is_some() || kind.is_some() {
                issue = replace_fields(
                    client,
                    repo,
                    &issue,
                    title.as_deref(),
                    new_body.as_deref(),
                    kind.map(|k| k.as_str()),
                    Some(issue.milestone_id.as_deref()),
                )
                .await?;
            }
            if let Some(status) = status {
                issue = client
                    .patch(
                        &repo.route(&format!("/issues/{number}/status")),
                        &json!({ "status": status.as_str() }),
                    )
                    .await?;
            }
            finished(context, &issue, "Updated")
        }
        IssueCommand::Comment { number, body } => {
            let text = required(body.read()?, "--body or --body-file", "Comment")?;
            let comment: Comment = client
                .post(
                    &repo.route(&format!("/issues/{number}/comments")),
                    &json!({ "body": text }),
                )
                .await?;
            context.out.message(&format!("Commented on #{number}."));
            context.out.result(&comment, || {})
        }
        IssueCommand::Close { number } => {
            let issue: Issue = client
                .post(&repo.route(&format!("/issues/{number}/close")), &json!({}))
                .await?;
            finished(context, &issue, "Closed")
        }
        IssueCommand::Reopen { number } => {
            let issue: Issue = client
                .post(&repo.route(&format!("/issues/{number}/reopen")), &json!({}))
                .await?;
            finished(context, &issue, "Reopened")
        }
        IssueCommand::Assign {
            number,
            username,
            unassign: _,
        } => {
            let assignee_id = match username {
                Some(username) => Some(user_id(client, repo, &username).await?),
                None => None,
            };
            let issue: Issue = client
                .post(
                    &repo.route(&format!("/issues/{number}/assign")),
                    &json!({ "assigneeId": assignee_id }),
                )
                .await?;
            finished(context, &issue, "Updated")
        }
        IssueCommand::Labels {
            number,
            add,
            remove,
        } => {
            if add.is_empty() && remove.is_empty() {
                return Err(CliError::Usage(
                    "nothing to change: give --add or --remove".to_string(),
                ));
            }
            let issue = get_issue(client, repo, number).await?;
            let known = known_labels(client, repo).await?;
            let mut ids: Vec<String> = issue
                .labels
                .iter()
                .filter(|label| !remove.contains(&label.name))
                .map(|label| label.id.clone())
                .collect();
            for name in &add {
                let label = known
                    .iter()
                    .find(|label| &label.name == name)
                    .ok_or_else(|| CliError::NotFound(format!("no label named {name}")))?;
                if !ids.contains(&label.id) {
                    ids.push(label.id.clone());
                }
            }
            let labels: Vec<Label> = client
                .put(
                    &repo.route(&format!("/issues/{number}/labels")),
                    &json!({ "labelIds": ids }),
                )
                .await?;
            context
                .out
                .message(&format!("Updated the labels of #{number}."));
            context.out.result(&labels, || {
                let names: Vec<&str> = labels.iter().map(|l| l.name.as_str()).collect();
                context.out.line(&clean(&names.join(", ")));
            })
        }
        IssueCommand::Milestone {
            number,
            title,
            clear: _,
        } => {
            let milestone_id = match title {
                Some(title) => Some(milestone_id(client, repo, &title).await?),
                None => None,
            };
            let issue = get_issue(client, repo, number).await?;
            let issue = replace_fields(
                client,
                repo,
                &issue,
                None,
                None,
                None,
                Some(milestone_id.as_deref()),
            )
            .await?;
            finished(context, &issue, "Updated")
        }
    }
}

fn finished(context: &Context, issue: &Issue, verb: &str) -> Result<()> {
    context.out.message(&format!("{verb} #{}.", issue.number));
    context.out.result(issue, || {})
}

async fn get_issue(client: &Client, repo: &Repo, number: i64) -> Result<Issue> {
    client.get(&repo.route(&format!("/issues/{number}"))).await
}

/// The route replaces every field, a missing one being a 400 and a null clearing the milestone, so what isn't changing has
/// to be sent back as it was.
async fn replace_fields(
    client: &Client,
    repo: &Repo,
    issue: &Issue,
    title: Option<&str>,
    description: Option<&str>,
    kind: Option<&str>,
    milestone_id: Option<Option<&str>>,
) -> Result<Issue> {
    client
        .patch(
            &repo.route(&format!("/issues/{}", issue.number)),
            &json!({
                "title": title.unwrap_or(&issue.title),
                "description": description.unwrap_or(&issue.description),
                "kind": kind.unwrap_or(&issue.kind),
                "milestoneId": milestone_id.unwrap_or(issue.milestone_id.as_deref()),
            }),
        )
        .await
}

/// An issue can take labels from its repository and from the groups above it.
async fn known_labels(client: &Client, repo: &Repo) -> Result<Vec<Label>> {
    let mut labels: Vec<Label> = client.get(&repo.route("/labels")).await?;
    for group in &repo.groups {
        let inherited: Vec<Label> = client.get(&format!("/groups/{group}/labels")).await?;
        labels.extend(inherited);
    }
    Ok(labels)
}

async fn milestone_id(client: &Client, repo: &Repo, title: &str) -> Result<String> {
    let mut milestones: Vec<Milestone> = client.get(&repo.route("/milestones")).await?;
    for group in &repo.groups {
        let inherited: Vec<Milestone> = client.get(&format!("/groups/{group}/milestones")).await?;
        milestones.extend(inherited);
    }
    milestones
        .into_iter()
        .find(|milestone| milestone.title == title)
        .map(|milestone| milestone.id)
        .ok_or_else(|| CliError::NotFound(format!("no milestone titled {title}")))
}

/// A username to its id. Collaborators first, since that only takes a scope this command already needs to be useful. Group
/// members aren't among them, so the search covers those, when the token's scopes allow it.
async fn user_id(client: &Client, repo: &Repo, username: &str) -> Result<String> {
    if let Ok(collaborators) = client
        .get::<Vec<Collaborator>>(&repo.route("/collaborators"))
        .await
        && let Some(found) = collaborators.iter().find(|c| c.username == username)
    {
        return Ok(found.user_id.clone());
    }
    let encoded: String = url::form_urlencoded::byte_serialize(username.as_bytes()).collect();
    let results: UserSearch = client.get(&format!("/search?q={encoded}")).await?;
    results
        .users
        .into_iter()
        .find(|user| user.username == username)
        .map(|user| user.id)
        .ok_or_else(|| CliError::NotFound(format!("no user named {username}")))
}

fn print_issue(context: &Context, issue: &Issue) {
    let labels: Vec<&str> = issue.labels.iter().map(|l| l.name.as_str()).collect();
    context.out.line(
        &context
            .out
            .paint("1", &format!("#{} {}", issue.number, clean(&issue.title))),
    );
    context.out.table(&[
        vec!["status".to_string(), issue.status.clone()],
        vec!["kind".to_string(), issue.kind.clone()],
        vec!["author".to_string(), username(&issue.author)],
        vec!["assignee".to_string(), username(&issue.assignee)],
        vec!["labels".to_string(), labels.join(", ")],
        vec!["comments".to_string(), issue.comment_count.to_string()],
    ]);
    if !issue.description.is_empty() {
        context.out.line("");
        context.out.line(&clean_keeping_lines(&issue.description));
    }
}

fn print_comments(context: &Context, comments: &[Comment]) {
    for comment in comments {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue_with_status(status: &str) -> Issue {
        Issue {
            id: "i".to_string(),
            number: 1,
            title: "t".to_string(),
            description: String::new(),
            status: status.to_string(),
            kind: "bug".to_string(),
            milestone_id: None,
            author: None,
            assignee: None,
            labels: vec![],
            comment_count: 0,
            created_at: String::new(),
            closed_at: None,
        }
    }

    #[test]
    fn open_is_every_status_but_done() {
        for status in ["todo", "in_progress", "in_review"] {
            let issue = issue_with_status(status);
            assert!(matches_state(&issue, StateFilter::Open), "{status}");
            assert!(!matches_state(&issue, StateFilter::Closed), "{status}");
        }
        let done = issue_with_status("done");
        assert!(!matches_state(&done, StateFilter::Open));
        assert!(matches_state(&done, StateFilter::Closed));
        assert!(matches_state(&done, StateFilter::All));
    }
}
