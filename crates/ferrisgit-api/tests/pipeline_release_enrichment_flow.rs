mod common;

use common::git::git;

use common::http::post_json;

use common::http::login;
use ferrisgit_api::state::AppState;
use ferrisgit_domain::pipeline::NewPipeline;
use ferrisgit_domain::settings::ExecutionEngine;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;
use std::time::Duration;
use uuid::Uuid;

async fn spawn_server(pool: PgPool) -> (SocketAddr, AppState) {
    let app = common::spawn_app(pool).await;
    (app.addr, app.state)
}

struct Fixture {
    addr: SocketAddr,
    state: AppState,
    client: reqwest::Client,
    jwt: String,
    plain_token: String,
    repo_id: String,
    repository_id: Uuid,
    admin_id: Uuid,
}

async fn fixture(pool: PgPool) -> Fixture {
    let (addr, state) = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let jwt = login(&client, addr, "admin", "adminpassword123").await;
    let token_res: Value =
        post_json(&client, addr, &jwt, "/tokens", &json!({ "name": "ci" })).await;
    let plain_token = token_res["token"].as_str().unwrap().to_string();
    let repo_res: Value = post_json(
        &client,
        addr,
        &jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    let repo_id = repo_res["id"].as_str().unwrap().to_string();
    let repository_id: Uuid = repo_id.parse().unwrap();
    let admin_id = state
        .users
        .find_by_username("admin")
        .await
        .unwrap()
        .unwrap()
        .id;
    Fixture {
        addr,
        state,
        client,
        jwt,
        plain_token,
        repo_id,
        repository_id,
        admin_id,
    }
}

async fn push_commit(f: &Fixture, files: &[(&str, &str)], message: &str) -> String {
    let clone_parent = tempfile::tempdir().unwrap();
    let clone_url = format!("http://admin:{}@{}/admin/hello.git", f.plain_token, f.addr);
    git(&["clone", "-q", &clone_url, "repo"], clone_parent.path()).await;
    let repo_path = clone_parent.path().join("repo");
    for (name, content) in files {
        std::fs::write(repo_path.join(name), content).unwrap();
    }
    git(&["add", "."], &repo_path).await;
    git(
        &[
            "-c",
            "user.email=t@t.com",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            message,
        ],
        &repo_path,
    )
    .await;
    git(&["push", "-q", "origin", "HEAD:main"], &repo_path).await;
    git(&["rev-parse", "HEAD"], &repo_path).await
}

async fn wait_for_first_pipeline(f: &Fixture) -> Value {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let res: Value = f
            .client
            .get(format!(
                "http://{}/api/repositories/{}/pipelines",
                f.addr, f.repo_id
            ))
            .bearer_auth(&f.jwt)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if res.as_array().is_some_and(|a| !a.is_empty()) {
            return res;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for a pipeline to be created after the push"
        );
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

async fn get_json(f: &Fixture, path: &str) -> Value {
    let res = f
        .client
        .get(format!("http://{}{path}", f.addr))
        .bearer_auth(&f.jwt)
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success(), "GET {path} -> {}", res.status());
    res.json().await.unwrap()
}

const CI_FILE: &str = "stages: [build]\njobs:\n  hello:\n    stage: build\n    image: alpine:3.20\n    script:\n      - echo hi\n";

#[sqlx::test]
async fn pipelines_carry_the_trigger_user_and_the_first_line_of_the_commit_message(pool: PgPool) {
    let f = fixture(pool).await;
    let sha = push_commit(
        &f,
        &[(".ferrisgit-ci.yml", CI_FILE)],
        "add pipeline\n\nA longer body\nthat must not leak",
    )
    .await;

    let list = wait_for_first_pipeline(&f).await;
    let pipeline = &list.as_array().unwrap()[0];
    assert_eq!(pipeline["commitSha"], sha, "existing fields stay unchanged");
    assert!(pipeline["status"].is_string());
    assert!(pipeline["createdAt"].is_string());
    assert_eq!(pipeline["triggeredBy"]["username"], "admin");
    assert_eq!(pipeline["triggeredBy"]["id"], f.admin_id.to_string());
    assert_eq!(pipeline["commitMessage"], "add pipeline");

    let detail = get_json(
        &f,
        &format!("/api/pipelines/{}", pipeline["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(detail["commitSha"], sha);
    assert_eq!(detail["triggeredBy"]["username"], "admin");
    assert_eq!(detail["commitMessage"], "add pipeline");
    assert!(detail["jobs"].is_array(), "the detail keeps its jobs");
}

#[sqlx::test]
async fn a_pipeline_whose_commit_cannot_be_resolved_has_a_null_commit_message_and_never_fails_the_request(
    pool: PgPool,
) {
    let f = fixture(pool).await;
    push_commit(&f, &[("README.md", "hello\n")], "root").await;
    let other_user = f
        .state
        .users
        .find_by_username("admin")
        .await
        .unwrap()
        .unwrap()
        .id;
    let bogus = f
        .state
        .pipelines
        .create(NewPipeline {
            repository_id: f.repository_id,
            commit_sha: "0123456789abcdef0123456789abcdef01234567".to_string(),
            execution_engine: ExecutionEngine::DockerRunners,
            triggered_by: other_user,
        })
        .await
        .unwrap();
    let not_hex = f
        .state
        .pipelines
        .create(NewPipeline {
            repository_id: f.repository_id,
            commit_sha: "abc123".to_string(),
            execution_engine: ExecutionEngine::DockerRunners,
            triggered_by: other_user,
        })
        .await
        .unwrap();

    let list = get_json(&f, &format!("/api/repositories/{}/pipelines", f.repo_id)).await;
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 2);
    for p in list {
        assert!(
            p["commitMessage"].is_null(),
            "unresolvable commit -> null, got {p}"
        );
        assert_eq!(p["triggeredBy"]["username"], "admin");
    }

    for id in [bogus.id, not_hex.id] {
        let detail = get_json(&f, &format!("/api/pipelines/{id}")).await;
        assert!(detail["commitMessage"].is_null());
        assert_eq!(detail["triggeredBy"]["username"], "admin");
    }
}

#[sqlx::test]
async fn the_list_resolves_commit_messages_for_the_50_newest_pipelines_only(pool: PgPool) {
    let f = fixture(pool).await;
    let sha = push_commit(&f, &[("README.md", "hello\n")], "the one commit").await;
    // 51 pipelines on the same real commit: the oldest one falls outside the 50 newest.
    let mut ids = Vec::new();
    for _ in 0..51 {
        let p = f
            .state
            .pipelines
            .create(NewPipeline {
                repository_id: f.repository_id,
                commit_sha: sha.clone(),
                execution_engine: ExecutionEngine::DockerRunners,
                triggered_by: f.admin_id,
            })
            .await
            .unwrap();
        ids.push(p.id.to_string());
    }

    let list = get_json(&f, &format!("/api/repositories/{}/pipelines", f.repo_id)).await;
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 51);
    let resolved = list
        .iter()
        .filter(|p| p["commitMessage"] == "the one commit")
        .count();
    let unresolved = list.iter().filter(|p| p["commitMessage"].is_null()).count();
    assert_eq!(resolved, 50);
    assert_eq!(unresolved, 1);
    assert!(list[50]["commitMessage"].is_null());
    assert_eq!(list[50]["id"], ids[0]);

    let detail = get_json(&f, &format!("/api/pipelines/{}", ids[0])).await;
    assert_eq!(detail["commitMessage"], "the one commit");
}

#[sqlx::test]
async fn releases_carry_their_author_an_excerpt_of_the_notes_and_an_asset_count(pool: PgPool) {
    let f = fixture(pool).await;
    let sha = push_commit(&f, &[("README.md", "hello\n")], "root").await;
    let base = format!("http://{}/api/repositories/{}/releases", f.addr, f.repo_id);

    f.client
        .post(format!("http://{}/api/admin/users", f.addr))
        .bearer_auth(&f.jwt)
        .json(&json!({ "username": "maintainer-mia", "email": "mia@example.com", "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    f.client
        .post(format!(
            "http://{}/api/repositories/{}/collaborators",
            f.addr, f.repo_id
        ))
        .bearer_auth(&f.jwt)
        .json(&json!({ "username": "maintainer-mia", "role": "maintainer" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let mia_jwt = login(&f.client, f.addr, "maintainer-mia", "password12345").await;

    let long_notes = format!("  {}  ", "n".repeat(300));
    let created: Value = f
        .client
        .post(&base)
        .bearer_auth(&mia_jwt)
        .json(&json!({ "tagName": "v1.0.0", "targetCommitSha": sha, "title": "First", "notes": long_notes, "draft": false }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        created["author"]["username"], "maintainer-mia",
        "the create response is enriched too"
    );
    assert_eq!(created["assetCount"], 0);

    for name in ["a.tar.gz", "b.tar.gz"] {
        let part = reqwest::multipart::Part::bytes(b"bytes".to_vec())
            .file_name(name)
            .mime_str("application/gzip")
            .unwrap();
        let uploaded: Value = f
            .client
            .post(format!("{base}/v1.0.0/assets"))
            .bearer_auth(&mia_jwt)
            .multipart(reqwest::multipart::Form::new().part("file", part))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(
            uploaded["uploader"]["username"], "maintainer-mia",
            "the upload response is enriched too"
        );
    }

    let tag2: Value = f.client.post(&base).bearer_auth(&f.jwt).json(&json!({ "tagName": "v2.0.0", "targetCommitSha": sha, "title": "Second", "notes": "", "draft": false })).send().await.unwrap().json().await.unwrap();
    assert_eq!(tag2["notesExcerpt"], "");

    let list = get_json(&f, &format!("/api/repositories/{}/releases", f.repo_id)).await;
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 2);
    let first = list.iter().find(|r| r["tagName"] == "v1.0.0").unwrap();
    assert_eq!(first["title"], "First", "existing fields stay unchanged");
    assert_eq!(first["draft"], false);
    assert!(first["authorId"].is_string());
    assert_eq!(first["author"]["username"], "maintainer-mia");
    assert_eq!(first["assetCount"], 2);
    let excerpt = first["notesExcerpt"].as_str().unwrap();
    assert_eq!(excerpt.chars().count(), 240);
    assert!(
        excerpt.chars().all(|c| c == 'n'),
        "the notes are trimmed before being cut"
    );

    let second = list.iter().find(|r| r["tagName"] == "v2.0.0").unwrap();
    assert_eq!(second["author"]["username"], "admin");
    assert_eq!(second["notesExcerpt"], "");
    assert_eq!(second["assetCount"], 0);

    let detail = get_json(
        &f,
        &format!("/api/repositories/{}/releases/v1.0.0", f.repo_id),
    )
    .await;
    assert_eq!(detail["author"]["username"], "maintainer-mia");
    assert_eq!(
        detail["notes"], long_notes,
        "the full notes stay untouched on the detail"
    );
    assert_eq!(detail["targetCommitSha"], sha);
    let assets = detail["assets"].as_array().unwrap();
    assert_eq!(assets.len(), 2);
    for asset in assets {
        assert_eq!(asset["uploader"]["username"], "maintainer-mia");
        assert!(
            asset["uploadedBy"].is_string(),
            "existing fields stay unchanged"
        );
    }

    let patched: Value = f
        .client
        .patch(format!("{base}/v1.0.0"))
        .bearer_auth(&f.jwt)
        .json(&json!({ "title": "Renamed" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(patched["author"]["username"], "maintainer-mia");
    assert_eq!(patched["assets"].as_array().unwrap().len(), 2);
    assert_eq!(
        patched["assets"][0]["uploader"]["username"],
        "maintainer-mia"
    );
}

#[sqlx::test]
async fn a_short_notes_excerpt_is_the_trimmed_notes_and_never_cuts_a_multibyte_character(
    pool: PgPool,
) {
    let f = fixture(pool).await;
    let sha = push_commit(&f, &[("README.md", "hello\n")], "root").await;
    let base = format!("http://{}/api/repositories/{}/releases", f.addr, f.repo_id);

    f.client.post(&base).bearer_auth(&f.jwt).json(&json!({ "tagName": "v1", "targetCommitSha": sha, "title": "Short", "notes": "  # Hello\n\nworld  ", "draft": false })).send().await.unwrap().error_for_status().unwrap();
    // 3-byte characters: a naive byte slice at 240 would split one.
    let accents = "é".repeat(300);
    f.client.post(&base).bearer_auth(&f.jwt).json(&json!({ "tagName": "v2", "targetCommitSha": sha, "title": "Accents", "notes": accents, "draft": false })).send().await.unwrap().error_for_status().unwrap();

    let list = get_json(&f, &format!("/api/repositories/{}/releases", f.repo_id)).await;
    let list = list.as_array().unwrap();
    let short = list.iter().find(|r| r["tagName"] == "v1").unwrap();
    assert_eq!(
        short["notesExcerpt"], "# Hello\n\nworld",
        "no markdown stripping, only trimming"
    );
    let accented = list.iter().find(|r| r["tagName"] == "v2").unwrap();
    assert_eq!(
        accented["notesExcerpt"].as_str().unwrap().chars().count(),
        240
    );
}
