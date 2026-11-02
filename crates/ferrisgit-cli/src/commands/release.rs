use std::path::Path;

use futures_util::StreamExt;
use reqwest::multipart::{Form, Part};
use serde_json::json;
use tokio::io::AsyncWriteExt;

use crate::cli::ReleaseCommand;
use crate::client::{Client, TRANSFER_TIMEOUT};
use crate::context::{Context, Repo, read_file_or_stdin};
use crate::error::{CliError, Result};
use crate::model::{Branch, Release, ReleaseAsset, ReleaseSummary};
use crate::output::{clean, clean_keeping_lines, required};

/// A tag can't hold a `/`, the server refuses it, but it still goes into a URL.
fn encoded(tag: &str) -> String {
    url::form_urlencoded::byte_serialize(tag.as_bytes()).collect()
}

fn is_commit_id(value: &str) -> bool {
    value.len() == 40 && value.chars().all(|c| c.is_ascii_hexdigit())
}

pub async fn run(context: &Context, command: ReleaseCommand) -> Result<()> {
    let (client, repo) = context.repo().await?;
    let client = &client;
    let repo = &repo;
    match command {
        ReleaseCommand::List => {
            let releases: Vec<ReleaseSummary> = client.get(&repo.route("/releases")).await?;
            context.out.result(&releases, || {
                let rows: Vec<Vec<String>> = releases
                    .iter()
                    .map(|r| {
                        let state = match (r.draft, r.prerelease) {
                            (true, _) => "draft",
                            (_, true) => "pre-release",
                            _ => "published",
                        };
                        vec![
                            r.tag_name.clone(),
                            state.to_string(),
                            r.asset_count.to_string(),
                            r.title.clone(),
                        ]
                    })
                    .collect();
                context.out.table(&rows);
            })
        }
        ReleaseCommand::View { tag } => {
            let release = get(client, repo, &tag).await?;
            context
                .out
                .result(&release, || print_release(context, &release))
        }
        ReleaseCommand::Create {
            tag,
            title,
            notes,
            notes_file,
            target,
            draft,
            prerelease,
            asset,
        } => {
            let title = required(title, "--title", "Title")?;
            let notes = match (notes, notes_file) {
                (Some(notes), _) => notes,
                (None, Some(file)) => read_file_or_stdin(&file)?,
                (None, None) => String::new(),
            };
            // Before anything is created, so a typo in a file name doesn't leave half a release behind.
            for path in &asset {
                if !path.is_file() {
                    return Err(CliError::Usage(format!("no such file: {}", path.display())));
                }
            }
            let target_commit_sha = target_commit(client, repo, target.as_deref()).await?;
            let _: ReleaseSummary = client
                .post(
                    &repo.route("/releases"),
                    &json!({
                        "tagName": tag,
                        "targetCommitSha": target_commit_sha,
                        "title": title,
                        "notes": notes,
                        "draft": draft,
                        "prerelease": prerelease,
                    }),
                )
                .await?;
            for path in &asset {
                upload(context, client, repo, &tag, path).await?;
            }
            context.out.message(&format!("Created release {tag}."));
            let release = get(client, repo, &tag).await?;
            context.out.result(&release, || {})
        }
        ReleaseCommand::Upload { tag, files } => {
            let mut uploaded = Vec::with_capacity(files.len());
            for path in &files {
                uploaded.push(upload(context, client, repo, &tag, path).await?);
            }
            context.out.result(&uploaded, || {})
        }
        ReleaseCommand::Download {
            tag,
            asset,
            dir,
            clobber,
        } => {
            let release = get(client, repo, &tag).await?;
            for wanted in &asset {
                if !release.assets.iter().any(|a| &a.filename == wanted) {
                    return Err(CliError::NotFound(format!(
                        "release {tag} has no asset named {wanted}"
                    )));
                }
            }
            let selected: Vec<&ReleaseAsset> = release
                .assets
                .iter()
                .filter(|a| asset.is_empty() || asset.contains(&a.filename))
                .collect();
            if selected.is_empty() {
                return Err(CliError::NotFound(format!("release {tag} has no asset")));
            }
            tokio::fs::create_dir_all(&dir)
                .await
                .map_err(|e| CliError::Other(format!("could not create {}: {e}", dir.display())))?;
            let mut saved = Vec::with_capacity(selected.len());
            for asset in selected {
                let path = download(client, repo, &tag, asset, &dir, clobber).await?;
                context.out.message(&format!("Saved {}.", path.display()));
                saved.push(path.display().to_string());
            }
            context.out.result(&saved, || {})
        }
    }
}

async fn get(client: &Client, repo: &Repo, tag: &str) -> Result<Release> {
    client
        .get(&repo.route(&format!("/releases/{}", encoded(tag))))
        .await
}

async fn target_commit(client: &Client, repo: &Repo, target: Option<&str>) -> Result<String> {
    if let Some(target) = target
        && is_commit_id(target)
    {
        return Ok(target.to_lowercase());
    }
    let branches: Vec<Branch> = client.get(&repo.route("/branches")).await?;
    let found = match target {
        Some(target) => branches.iter().find(|b| b.name == target),
        None => branches.iter().find(|b| b.is_default),
    };
    found.map(|branch| branch.tip_sha.clone()).ok_or_else(|| {
        CliError::NotFound(match target {
            Some(target) => format!("no branch named {target}: give a branch or a full commit id"),
            None => "the repository has no default branch: use --target".to_string(),
        })
    })
}

async fn upload(
    context: &Context,
    client: &Client,
    repo: &Repo,
    tag: &str,
    path: &Path,
) -> Result<ReleaseAsset> {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| CliError::Usage(format!("not a file name: {}", path.display())))?
        .to_string();
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| CliError::Other(format!("could not read {}: {e}", path.display())))?;
    let form = Form::new().part("file", Part::bytes(bytes).file_name(filename.clone()));
    let request = client
        .request(
            reqwest::Method::POST,
            &repo.route(&format!("/releases/{}/assets", encoded(tag))),
        )
        .multipart(form)
        .timeout(TRANSFER_TIMEOUT);
    let response = client.send(request).await?;
    let asset: ReleaseAsset = response
        .json()
        .await
        .map_err(|e| CliError::Server(format!("unexpected answer: {e}")))?;
    context.out.message(&format!("Uploaded {filename}."));
    Ok(asset)
}

/// The name comes from the server, and only its last component is used: it mustn't choose where the file lands.
fn local_name(filename: &str) -> Result<&str> {
    Path::new(filename)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.contains(['/', '\\']))
        .ok_or_else(|| CliError::Refused(format!("unusable asset name: {}", clean(filename))))
}

async fn download(
    client: &Client,
    repo: &Repo,
    tag: &str,
    asset: &ReleaseAsset,
    dir: &Path,
    clobber: bool,
) -> Result<std::path::PathBuf> {
    let path = dir.join(local_name(&asset.filename)?);
    if path.exists() && !clobber {
        return Err(CliError::Usage(format!(
            "{} already exists: pass --clobber to replace it",
            path.display()
        )));
    }
    let failed =
        |e: std::io::Error| CliError::Other(format!("could not write {}: {e}", path.display()));
    let request = client
        .request(
            reqwest::Method::GET,
            &repo.route(&format!("/releases/{}/assets/{}", encoded(tag), asset.id)),
        )
        .timeout(TRANSFER_TIMEOUT);
    let response = client.send(request).await?;

    // Beside the target, so that an interrupted transfer never leaves a truncated file under the real name.
    let partial = path.with_extension("fgit-partial");
    let mut file = tokio::fs::File::create(&partial).await.map_err(failed)?;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(e) => {
                let _ = tokio::fs::remove_file(&partial).await;
                return Err(CliError::Server(format!("the download stopped: {e}")));
            }
        };
        file.write_all(&chunk).await.map_err(failed)?;
    }
    file.flush().await.map_err(failed)?;
    drop(file);
    tokio::fs::rename(&partial, &path).await.map_err(failed)?;
    Ok(path)
}

fn print_release(context: &Context, release: &Release) {
    context.out.line(&context.out.paint(
        "1",
        &format!("{} {}", clean(&release.tag_name), clean(&release.title)),
    ));
    context.out.table(&[
        vec!["commit".to_string(), release.target_commit_sha.clone()],
        vec!["draft".to_string(), release.draft.to_string()],
        vec!["pre-release".to_string(), release.prerelease.to_string()],
    ]);
    if !release.notes.is_empty() {
        context.out.line("");
        context.out.line(&clean_keeping_lines(&release.notes));
    }
    if !release.assets.is_empty() {
        context.out.line("");
        let rows: Vec<Vec<String>> = release
            .assets
            .iter()
            .map(|a| vec![a.filename.clone(), format!("{} bytes", a.size_bytes)])
            .collect();
        context.out.table(&rows);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_asset_name_can_not_choose_where_the_file_lands() {
        assert_eq!(local_name("app.tar.gz").unwrap(), "app.tar.gz");
        assert_eq!(local_name("../../etc/passwd").unwrap(), "passwd");
        assert!(local_name("..").is_err());
        assert!(local_name("").is_err());
        assert!(local_name(r"..\..\evil.exe").is_err() || cfg!(windows));
    }

    #[test]
    fn a_full_commit_id_is_used_as_is() {
        assert!(is_commit_id(&"a".repeat(40)));
        assert!(!is_commit_id("main"));
        assert!(!is_commit_id(&"g".repeat(40)));
    }
}
