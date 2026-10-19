mod common;

use common::git::{commit_file, git};
use common::http::{get_json, login, mint_api_token, post, post_json, put};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;
use std::path::Path;

/// Pushes `branch`, made of `commits` (file, message) on top of `base`, and opens a merge request from it into `main`.
async fn open_merge_request(
    client: &reqwest::Client,
    addr: SocketAddr,
    jwt: &str,
    repo_id: &str,
    work: &Path,
    branch: &str,
    commits: &[(&str, &str)],
) -> String {
    git(&["checkout", "-q", "-B", branch, "origin/main"], work).await;
    for (file, message) in commits {
        commit_file(work, file, &format!("{message}\n"), message).await;
    }
    git(&["push", "-q", "origin", branch], work).await;
    let mr: Value = post_json(
        client,
        addr,
        jwt,
        &format!("/repositories/{repo_id}/merge-requests"),
        &json!({ "sourceBranch": branch, "targetBranch": "main", "title": format!("Land {branch}"), "description": "" }),
    )
    .await;
    mr["id"].as_str().unwrap().to_string()
}

/// `main`'s commits, newest first, as `subject|parent count`.
async fn main_history(work: &Path) -> Vec<String> {
    git(&["fetch", "-q", "origin"], work).await;
    git(&["log", "--format=%s|%p", "origin/main"], work)
        .await
        .lines()
        .map(|line| {
            let (subject, parents) = line.split_once('|').unwrap();
            format!("{subject}|{}", parents.split_whitespace().count())
        })
        .collect()
}

#[sqlx::test]
async fn a_merge_request_lands_by_merge_commit_squash_or_rebase(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let jwt = login(&client, addr, "admin", "adminpassword123").await;
    let token = mint_api_token(&client, addr, &jwt, "dev").await;
    let repo: Value = post_json(
        &client,
        addr,
        &jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    let repo_id = repo["id"].as_str().unwrap().to_string();
    let parent = tempfile::tempdir().unwrap();
    let url = format!("http://admin:{token}@{addr}/admin/hello.git");
    git(&["clone", "-q", &url, "work"], parent.path()).await;
    let work = parent.path().join("work");
    commit_file(&work, "README.md", "hello\n", "init").await;
    git(&["push", "-q", "origin", "HEAD:main"], &work).await;
    git(&["fetch", "-q", "origin"], &work).await;

    let settings = get_json(
        &client,
        addr,
        &jwt,
        &format!("/repositories/{repo_id}/settings"),
    )
    .await;
    assert_eq!(settings["defaultMergeMethod"], "merge");

    // Squash: one commit with the merge request's title, on top of main.
    let squashed = open_merge_request(
        &client,
        addr,
        &jwt,
        &repo_id,
        &work,
        "squashed",
        &[("a.txt", "add a"), ("b.txt", "add b")],
    )
    .await;
    let merged: Value = post_json(
        &client,
        addr,
        &jwt,
        &format!("/merge-requests/{squashed}/merge"),
        &json!({ "method": "squash" }),
    )
    .await;
    assert_eq!(merged["outcome"], "merged", "{merged:#}");
    assert_eq!(main_history(&work).await, vec!["Land squashed|1", "init|0"]);

    // Rebase: each commit replayed on main, no merge commit.
    let rebased = open_merge_request(
        &client,
        addr,
        &jwt,
        &repo_id,
        &work,
        "rebased",
        &[("c.txt", "add c"), ("d.txt", "add d")],
    )
    .await;
    // main moves on, so the rebase really rewrites the commits.
    git(&["checkout", "-q", "-B", "hotfix", "origin/main"], &work).await;
    commit_file(&work, "hotfix.txt", "fix\n", "hotfix").await;
    git(&["push", "-q", "origin", "HEAD:main"], &work).await;
    let merged: Value = post_json(
        &client,
        addr,
        &jwt,
        &format!("/merge-requests/{rebased}/merge"),
        &json!({ "method": "rebase" }),
    )
    .await;
    assert_eq!(merged["outcome"], "merged", "{merged:#}");
    assert_eq!(
        main_history(&work).await,
        vec![
            "add d|1",
            "add c|1",
            "hotfix|1",
            "Land squashed|1",
            "init|0"
        ]
    );
    assert_eq!(
        merged["mergeCommitSha"].as_str().unwrap(),
        git(&["rev-parse", "origin/main"], &work).await
    );

    // The repository default applies to a merge that names no method.
    let res = put(
        &client,
        addr,
        &jwt,
        &format!("/repositories/{repo_id}/settings"),
        &json!({ "defaultMergeMethod": "squash" }),
    )
    .await;
    assert_eq!(res.status(), 200);
    assert_eq!(
        res.json::<Value>().await.unwrap()["defaultMergeMethod"],
        "squash"
    );
    let defaulted = open_merge_request(
        &client,
        addr,
        &jwt,
        &repo_id,
        &work,
        "defaulted",
        &[("e.txt", "add e"), ("f.txt", "add f")],
    )
    .await;
    let res = post(
        &client,
        addr,
        &jwt,
        &format!("/merge-requests/{defaulted}/merge"),
        &json!({ "method": "octopus" }),
    )
    .await;
    assert_eq!(res.status(), 400, "an unknown method is refused");
    let res = client
        .post(format!(
            "http://{addr}/api/merge-requests/{defaulted}/merge"
        ))
        .bearer_auth(&jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(main_history(&work).await[0], "Land defaulted|1");

    let timeline = get_json(
        &client,
        addr,
        &jwt,
        &format!("/merge-requests/{defaulted}/timeline"),
    )
    .await;
    let merged_event = timeline["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["kind"] == "merged")
        .expect("a merged event");
    assert_eq!(merged_event["payload"]["method"], "squash");

    let res = put(
        &client,
        addr,
        &jwt,
        &format!("/repositories/{repo_id}/settings"),
        &json!({ "defaultMergeMethod": "fast-forward" }),
    )
    .await;
    assert_eq!(res.status(), 400);
}
