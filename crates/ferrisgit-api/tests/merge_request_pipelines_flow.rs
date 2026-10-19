mod common;

use common::git::{commit_file, git};
use common::http::{get_json, login, mint_api_token, post, post_empty, post_json, put};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::path::Path;
use std::process::Command;

const PIPELINE_FILE: &str =
    "stages: [test]\njobs:\n  unit:\n    stage: test\n    image: alpine\n    script: [\"true\"]\n";

/// Runs `git <args>` in `cwd` and returns whether it succeeded, with its stdout and stderr.
async fn try_git(args: &[&str], cwd: &Path) -> (bool, String) {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let cwd = cwd.to_path_buf();
    let output = tokio::task::spawn_blocking(move || {
        Command::new("git").args(&args).current_dir(&cwd).output()
    })
    .await
    .unwrap()
    .unwrap();
    (
        output.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    )
}

#[sqlx::test]
async fn a_merge_request_runs_a_pipeline_on_the_merge_of_its_branches(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (addr, pool) = (app.addr, app.pool.clone());
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
    commit_file(&work, ".ferrisgit-ci.yml", PIPELINE_FILE, "ci").await;
    git(&["push", "-q", "origin", "HEAD:main"], &work).await;
    git(&["checkout", "-q", "-b", "feature"], &work).await;
    commit_file(&work, "feature.txt", "one\n", "feature one").await;
    git(&["push", "-q", "origin", "feature"], &work).await;
    let first_tip = git(&["rev-parse", "HEAD"], &work).await;

    let mr: Value = post_json(
        &client,
        addr,
        &jwt,
        &format!("/repositories/{repo_id}/merge-requests"),
        &json!({ "sourceBranch": "feature", "targetBranch": "main", "title": "Feature", "description": "" }),
    )
    .await;
    let mr_id = mr["id"].as_str().unwrap().to_string();
    let first = mr["pipeline"].clone();
    assert_eq!(first["status"], "pending", "{mr:#}");
    assert_eq!(first["sourceSha"], first_tip.as_str());
    let merge_sha = first["commitSha"].as_str().unwrap().to_string();
    assert_ne!(
        merge_sha, first_tip,
        "the pipeline runs on the merge, not the source tip"
    );

    // The merge commit is a real merge of main and feature, kept under a ref nobody sees.
    let (fetched, output) = try_git(&["fetch", "-q", "origin", &merge_sha], &work).await;
    assert!(
        fetched,
        "a clone fetches the merge commit by its id: {output}"
    );
    let parents = git(&["log", "--pretty=%P", "-1", &merge_sha], &work).await;
    let main_tip = git(&["rev-parse", "origin/main"], &work).await;
    assert_eq!(parents, format!("{main_tip} {first_tip}"));
    let (_, refs) = try_git(&["ls-remote", "origin"], &work).await;
    assert!(!refs.contains("merge-requests"), "{refs}");
    let (pushed, output) = try_git(
        &[
            "push",
            "origin",
            &format!("HEAD:refs/merge-requests/{mr_id}/merge"),
        ],
        &work,
    )
    .await;
    assert!(
        !pushed,
        "the merge request refs are not pushed to: {output}"
    );

    let pipelines = get_json(
        &client,
        addr,
        &jwt,
        &format!("/repositories/{repo_id}/pipelines"),
    )
    .await;
    let listed = pipelines
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == first["id"])
        .expect("the merge request pipeline is listed with the others");
    assert_eq!(listed["mergeRequestId"], mr_id.as_str());
    assert_eq!(listed["sourceSha"], first_tip.as_str());

    // A new commit on the source branch starts a new pipeline and cancels the one it supersedes.
    commit_file(&work, "feature.txt", "two\n", "feature two").await;
    git(&["push", "-q", "origin", "feature"], &work).await;
    let second_tip = git(&["rev-parse", "HEAD"], &work).await;
    let detail = get_json(&client, addr, &jwt, &format!("/merge-requests/{mr_id}")).await;
    assert_eq!(detail["pipeline"]["sourceSha"], second_tip.as_str());
    assert_ne!(detail["pipeline"]["id"], first["id"]);
    let superseded = get_json(
        &client,
        addr,
        &jwt,
        &format!("/pipelines/{}", first["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(superseded["status"], "canceled", "{superseded:#}");

    let list = get_json(
        &client,
        addr,
        &jwt,
        &format!("/repositories/{repo_id}/merge-requests"),
    )
    .await;
    assert_eq!(list[0]["pipeline"]["id"], detail["pipeline"]["id"]);

    let timeline = get_json(
        &client,
        addr,
        &jwt,
        &format!("/merge-requests/{mr_id}/timeline"),
    )
    .await;
    let pipeline_items: Vec<&Value> = timeline["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["type"] == "pipeline")
        .collect();
    assert_eq!(pipeline_items.len(), 2, "{timeline:#}");
    assert_eq!(pipeline_items[0]["status"], "canceled");
    assert_eq!(pipeline_items[1]["status"], "pending");
    assert_eq!(pipeline_items[1]["triggeredBy"]["username"], "admin");

    // With pipelines required, the merge waits for the latest one to succeed.
    let res = put(
        &client,
        addr,
        &jwt,
        &format!("/repositories/{repo_id}/settings"),
        &json!({ "pipelineMustSucceed": true }),
    )
    .await;
    assert_eq!(res.status(), 200);
    assert_eq!(
        res.json::<Value>().await.unwrap()["pipelineMustSucceed"],
        true
    );
    let res = post_empty(
        &client,
        addr,
        &jwt,
        &format!("/merge-requests/{mr_id}/merge"),
    )
    .await;
    assert_eq!(res.status(), 400);
    let error = res.json::<Value>().await.unwrap();
    assert!(
        error["error"].as_str().unwrap().contains("not finished"),
        "{error:#}"
    );

    let latest_id: uuid::Uuid = detail["pipeline"]["id"].as_str().unwrap().parse().unwrap();
    sqlx::query("UPDATE pipelines SET status = 'success', finished_at = now() WHERE id = $1")
        .bind(latest_id)
        .execute(&pool)
        .await
        .unwrap();
    let merged: Value = post_empty(
        &client,
        addr,
        &jwt,
        &format!("/merge-requests/{mr_id}/merge"),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(merged["outcome"], "merged", "{merged:#}");
}

#[sqlx::test]
async fn a_push_that_leaves_the_merge_request_failing_blocks_a_required_merge(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    let (addr, pool) = (app.addr, app.pool.clone());
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
    let res = put(
        &client,
        addr,
        &jwt,
        &format!("/repositories/{repo_id}/settings"),
        &json!({ "pipelineMustSucceed": true }),
    )
    .await;
    assert_eq!(res.status(), 200);

    let parent = tempfile::tempdir().unwrap();
    let url = format!("http://admin:{token}@{addr}/admin/hello.git");
    git(&["clone", "-q", &url, "work"], parent.path()).await;
    let work = parent.path().join("work");
    // No pipeline file: no pipeline ever runs, so the merge can't happen while one is required.
    commit_file(&work, "README.md", "hello\n", "init").await;
    git(&["push", "-q", "origin", "HEAD:main"], &work).await;
    git(&["checkout", "-q", "-b", "feature"], &work).await;
    commit_file(&work, "feature.txt", "one\n", "feature").await;
    git(&["push", "-q", "origin", "feature"], &work).await;
    let mr: Value = post_json(
        &client,
        addr,
        &jwt,
        &format!("/repositories/{repo_id}/merge-requests"),
        &json!({ "sourceBranch": "feature", "targetBranch": "main", "title": "Feature", "description": "" }),
    )
    .await;
    assert_eq!(mr["pipeline"], Value::Null);
    let mr_id = mr["id"].as_str().unwrap();

    let res = post(
        &client,
        addr,
        &jwt,
        &format!("/merge-requests/{mr_id}/merge"),
        &json!({}),
    )
    .await;
    assert_eq!(res.status(), 400);
    let error = res.json::<Value>().await.unwrap();
    assert!(
        error["error"]
            .as_str()
            .unwrap()
            .contains("check 'pipeline' has not run"),
        "{error:#}"
    );
    let open: i64 = sqlx::query_scalar("SELECT count(*) FROM merge_requests WHERE status = 'open'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(open, 1);
}
