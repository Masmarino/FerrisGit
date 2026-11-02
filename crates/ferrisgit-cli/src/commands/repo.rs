use serde_json::{Map, Value, json};

use crate::cli::RepoCommand;
use crate::config::sends_token_in_clear;
use crate::context::{Context, resolve};
use crate::error::{CliError, Result};
use crate::git;
use crate::model::Repository;
use crate::output::required;

pub async fn run(context: &Context, command: RepoCommand) -> Result<()> {
    match command {
        RepoCommand::List { starred, hostname } => {
            let client = context.client(hostname.as_deref()).await?;
            let suffix = if starred { "?starred=true" } else { "" };
            let repositories: Vec<Repository> =
                client.get(&format!("/repositories{suffix}")).await?;
            context.out.result(&repositories, || {
                let rows: Vec<Vec<String>> = repositories
                    .iter()
                    .map(|r| {
                        vec![
                            r.path.join("/"),
                            r.visibility.clone(),
                            r.role.clone().unwrap_or_default(),
                            r.description.clone(),
                        ]
                    })
                    .collect();
                context.out.table(&rows);
            })
        }
        RepoCommand::View { repository } => {
            let (client, repo) = context.repo_named(repository.as_deref()).await?;
            let repository: Repository = client
                .get(&format!("/repositories/by-id/{}", repo.id))
                .await?;
            context
                .out
                .result(&repository, || print_repository(context, &repository))
        }
        RepoCommand::Create {
            name,
            description,
            public,
            group,
            hostname,
        } => {
            let name = required(name, "a repository name", "Repository name")?;
            let client = context.client(hostname.as_deref()).await?;
            let mut body = Map::new();
            body.insert("name".to_string(), json!(name));
            body.insert("description".to_string(), json!(description));
            body.insert(
                "visibility".to_string(),
                json!(if public { "public" } else { "private" }),
            );
            if let Some(group) = group {
                body.insert("groupPath".to_string(), json!(group));
            }
            let repository: Repository = client.post("/repositories", &Value::Object(body)).await?;
            context.out.message(&format!(
                "Created {}/{}",
                client.instance(),
                repository.path.join("/")
            ));
            context.out.result(&repository, || {
                context
                    .out
                    .line(&git::clone_url(client.instance(), &repository.path));
            })
        }
        RepoCommand::Clone {
            repository,
            directory,
        } => clone(context, &repository, directory).await,
    }
}

fn print_repository(context: &Context, repository: &Repository) {
    context.out.table(&[
        vec!["path".to_string(), repository.path.join("/")],
        vec!["visibility".to_string(), repository.visibility.clone()],
        vec![
            "your role".to_string(),
            repository
                .role
                .clone()
                .unwrap_or_else(|| "none".to_string()),
        ],
        vec!["description".to_string(), repository.description.clone()],
        vec!["created".to_string(), repository.created_at.clone()],
    ]);
}

async fn clone(
    context: &Context,
    repository: &str,
    directory: Option<std::path::PathBuf>,
) -> Result<()> {
    let reference = context.repo_ref(Some(repository)).await?;
    let instance = match &reference.instance {
        Some(instance) => instance.clone(),
        None => context.instance(None).await?,
    };
    let client = context.client_for(&instance).await?;
    let repo = resolve(&client, &reference).await?;

    let url = git::clone_url(&instance, &repo.path);
    let target = directory.unwrap_or_else(|| {
        std::path::PathBuf::from(repo.path.last().map_or("repository", String::as_str))
    });
    let target_text = target.to_str().ok_or_else(|| {
        CliError::Usage(format!("not a usable directory name: {}", target.display()))
    })?;

    let mut args = git::credential_options(&instance)?;
    args.extend([
        "clone".to_string(),
        "--".to_string(),
        url,
        target_text.to_string(),
    ]);
    git::run_visible(&args).await?;

    git::configure_helper(&["-C", target_text], "--local", &instance).await?;
    if sends_token_in_clear(&instance) {
        context
            .out
            .message("warning: this clone's token travels over plain HTTP");
    }
    context.out.message(&format!("Cloned into {target_text}."));
    Ok(())
}
