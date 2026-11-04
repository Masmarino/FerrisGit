mod common;

use common::git::{commit_file, git};
use common::http::{get_json, login, mint_api_token, post, post_empty, post_json, put};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

/// One job for each thing a rule can name, `unconditional` for none, and `shipped` for what `needs` does.
const PIPELINE_FILE: &str = r#"
stages: [check, ship]
jobs:
  unconditional:
    stage: check
    image: alpine
    script: ["true"]
  on-main:
    stage: check
    image: alpine
    script: ["true"]
    rules:
      branches: [main]
  on-release:
    stage: check
    image: alpine
    script: ["true"]
    rules:
      branches: ["release/*"]
  on-tag:
    stage: check
    image: alpine
    script: ["true"]
    rules:
      tags: ["v*"]
  on-merge-request:
    stage: check
    image: alpine
    script: ["true"]
    rules:
      merge_requests: true
  on-source-change:
    stage: check
    image: alpine
    script: ["true"]
    rules:
      changes: ["src/**"]
  shipped:
    stage: ship
    image: alpine
    script: ["true"]
    needs: [on-main]
"#;

/// A repository and a clone of it, with what it takes to push and to read the pipelines.
struct World {
    addr: SocketAddr,
    client: reqwest::Client,
    jwt: String,
    repo_id: String,
    work: PathBuf,
    _parent: tempfile::TempDir,
    _app: common::App,
}

impl World {
    async fn start(pool: PgPool) -> Self {
        let app = common::spawn_app(pool).await;
        let addr = app.addr;
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
        Self {
            addr,
            client,
            jwt,
            repo_id,
            work,
            _parent: parent,
            _app: app,
        }
    }

    async fn commit(&self, path: &str, text: &str) -> String {
        let file = self.work.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        commit_file(&self.work, path, text, path).await;
        self.tip().await
    }

    async fn tip(&self) -> String {
        git(&["rev-parse", "HEAD"], &self.work).await
    }

    fn dir(&self) -> &Path {
        &self.work
    }

    async fn pipelines(&self) -> Vec<Value> {
        get_json(
            &self.client,
            self.addr,
            &self.jwt,
            &format!("/repositories/{}/pipelines", self.repo_id),
        )
        .await
        .as_array()
        .unwrap()
        .clone()
    }

    /// Runs `step` and returns the pipelines it created, oldest first.
    async fn created_by(&self, step: impl std::future::Future<Output = ()>) -> Vec<Value> {
        let before: Vec<Value> = self
            .pipelines()
            .await
            .iter()
            .map(|p| p["id"].clone())
            .collect();
        step.await;
        let mut created: Vec<Value> = self
            .pipelines()
            .await
            .into_iter()
            .filter(|p| !before.contains(&p["id"]))
            .collect();
        created.reverse();
        created
    }

    /// The names of the jobs of a pipeline, sorted.
    async fn job_names(&self, pipeline: &Value) -> Vec<String> {
        let detail = get_json(
            &self.client,
            self.addr,
            &self.jwt,
            &format!("/pipelines/{}", pipeline["id"].as_str().unwrap()),
        )
        .await;
        let mut names: Vec<String> = detail["jobs"]
            .as_array()
            .unwrap_or_else(|| panic!("no jobs in {detail:#}"))
            .iter()
            .map(|job| job["name"].as_str().unwrap().to_string())
            .collect();
        names.sort();
        names
    }

    async fn post_release(&self, tag: &str, commit: &str, draft: bool) -> reqwest::Response {
        post(
            &self.client,
            self.addr,
            &self.jwt,
            &format!("/repositories/{}/releases", self.repo_id),
            &json!({ "tagName": tag, "targetCommitSha": commit, "title": tag, "draft": draft }),
        )
        .await
    }

    async fn open_merge_request(&self, source: &str) -> Value {
        post_json(
            &self.client,
            self.addr,
            &self.jwt,
            &format!("/repositories/{}/merge-requests", self.repo_id),
            &json!({ "sourceBranch": source, "targetBranch": "main", "title": source, "description": "" }),
        )
        .await
    }
}

#[sqlx::test]
async fn each_kind_of_pipeline_gets_the_jobs_its_rules_select_on_the_commit_it_is_about(
    pool: PgPool,
) {
    let world = World::start(pool).await;
    let dir = world.dir();

    // The first push of a branch: nothing to compare with, so `changes` is met.
    let first = world.commit(".ferrisgit-ci.yml", PIPELINE_FILE).await;
    let created = world
        .created_by(async {
            git(&["push", "-q", "origin", "HEAD:main"], dir).await;
        })
        .await;
    assert_eq!(created.len(), 1, "{created:#?}");
    assert_eq!(created[0]["commitSha"], first.as_str());
    assert_eq!(
        world.job_names(&created[0]).await,
        vec!["on-main", "on-source-change", "shipped", "unconditional"]
    );

    // A push that changes no source file.
    let second = world.commit("README.md", "hello\n").await;
    let created = world
        .created_by(async {
            git(&["push", "-q", "origin", "HEAD:main"], dir).await;
        })
        .await;
    assert_eq!(created[0]["commitSha"], second.as_str());
    assert_eq!(
        world.job_names(&created[0]).await,
        vec!["on-main", "shipped", "unconditional"]
    );

    // A push that does, and only what it changes counts: the commit before it touched another file.
    world.commit("src/lib.rs", "// one\n").await;
    let third = world.commit("README.md", "hello again\n").await;
    let created = world
        .created_by(async {
            git(&["push", "-q", "origin", "HEAD:main"], dir).await;
        })
        .await;
    assert_eq!(created[0]["commitSha"], third.as_str());
    assert_eq!(
        world.job_names(&created[0]).await,
        vec!["on-main", "on-source-change", "shipped", "unconditional"]
    );

    // A work branch runs on its own commit, not on the default branch. `shipped` needs `on-main`, left out here.
    git(&["checkout", "-q", "-b", "feature/x"], dir).await;
    let feature_tip = world.commit("notes.txt", "notes\n").await;
    let created = world
        .created_by(async {
            git(&["push", "-q", "origin", "HEAD:refs/heads/feature/x"], dir).await;
        })
        .await;
    assert_eq!(created.len(), 1, "{created:#?}");
    assert_eq!(created[0]["commitSha"], feature_tip.as_str());
    assert_eq!(created[0]["mergeRequestId"], Value::Null);
    assert_eq!(
        world.job_names(&created[0]).await,
        vec!["on-source-change", "unconditional"],
        "a new branch has nothing to compare with"
    );

    // A pattern over a branch with a slash in its name, and an update of it this time.
    git(&["checkout", "-q", "-b", "release/1.0"], dir).await;
    git(
        &["push", "-q", "origin", "HEAD:refs/heads/release/1.0"],
        dir,
    )
    .await;
    world.commit("notes.txt", "more notes\n").await;
    let created = world
        .created_by(async {
            git(
                &["push", "-q", "origin", "HEAD:refs/heads/release/1.0"],
                dir,
            )
            .await;
        })
        .await;
    assert_eq!(
        world.job_names(&created[0]).await,
        vec!["on-release", "unconditional"]
    );

    // An annotated tag runs on its commit, and only matching tags select `on-tag`.
    let tagged = world.tip().await;
    git(
        &[
            "-c",
            "user.email=t@t.com",
            "-c",
            "user.name=t",
            "tag",
            "-a",
            "v1.0.0",
            "-m",
            "one",
        ],
        dir,
    )
    .await;
    let created = world
        .created_by(async {
            git(&["push", "-q", "origin", "v1.0.0"], dir).await;
        })
        .await;
    assert_eq!(created.len(), 1, "{created:#?}");
    assert_eq!(created[0]["commitSha"], tagged.as_str());
    assert_eq!(
        world.job_names(&created[0]).await,
        vec!["on-source-change", "on-tag", "unconditional"]
    );
    git(&["tag", "nightly"], dir).await;
    let created = world
        .created_by(async {
            git(&["push", "-q", "origin", "nightly"], dir).await;
        })
        .await;
    assert_eq!(
        world.job_names(&created[0]).await,
        vec!["on-source-change", "unconditional"]
    );

    // Deleting refs runs nothing.
    let created = world
        .created_by(async {
            git(
                &["push", "-q", "origin", "--delete", "nightly", "feature/x"],
                dir,
            )
            .await;
        })
        .await;
    assert!(created.is_empty(), "{created:#?}");

    // One push of two refs is two pipelines.
    git(&["checkout", "-q", "-b", "two"], dir).await;
    git(&["tag", "v2"], dir).await;
    let created = world
        .created_by(async {
            git(&["push", "-q", "origin", "two", "v2"], dir).await;
        })
        .await;
    assert_eq!(created.len(), 2, "{created:#?}");
}

#[sqlx::test]
async fn a_release_that_creates_its_tag_runs_the_pipeline_of_that_tag(pool: PgPool) {
    let world = World::start(pool).await;
    let dir = world.dir();
    let tagged = world.commit(".ferrisgit-ci.yml", PIPELINE_FILE).await;
    let tip = world.commit("README.md", "hello\n").await;
    git(&["push", "-q", "origin", "HEAD:main"], dir).await;

    // On the commit of the tag, not the tip of the default branch. The same jobs as a pushed tag.
    let created = world
        .created_by(async {
            let response = world.post_release("v1.0.0", &tagged, false).await;
            assert_eq!(response.status(), 200);
        })
        .await;
    assert_eq!(created.len(), 1, "{created:#?}");
    assert_eq!(created[0]["commitSha"], tagged.as_str());
    assert_eq!(created[0]["mergeRequestId"], Value::Null);
    assert_eq!(
        world.job_names(&created[0]).await,
        vec!["on-source-change", "on-tag", "unconditional"]
    );

    // A draft's tag is a tag like any other. The rules see its name.
    let created = world
        .created_by(async {
            let response = world.post_release("nightly", &tip, true).await;
            assert_eq!(response.status(), 200);
        })
        .await;
    assert_eq!(created.len(), 1, "{created:#?}");
    assert_eq!(
        world.job_names(&created[0]).await,
        vec!["on-source-change", "unconditional"]
    );

    // A short sha is enough for git, and a pipeline still gets the commit in full.
    let created = world
        .created_by(async {
            let response = world.post_release("v1.1.0", &tip[..7], false).await;
            assert_eq!(response.status(), 200);
        })
        .await;
    assert_eq!(created.len(), 1, "{created:#?}");
    assert_eq!(created[0]["commitSha"], tip.as_str());

    // A tag that was pushed has had its pipeline: its release adds none.
    git(&["tag", "v2.0.0"], dir).await;
    git(&["push", "-q", "origin", "v2.0.0"], dir).await;
    let created = world
        .created_by(async {
            let response = world.post_release("v2.0.0", &tip, false).await;
            assert_eq!(response.status(), 200);
        })
        .await;
    assert!(created.is_empty(), "{created:#?}");

    // No tag created, no pipeline.
    let created = world
        .created_by(async {
            let response = world.post_release("v1.0.0", &tip, false).await;
            assert_eq!(response.status(), 400);
        })
        .await;
    assert!(created.is_empty(), "{created:#?}");

    // The tag of a release was deleted by a push. A second release is refused, but it did bring the tag back, and a retry
    // would now find it there and run nothing.
    git(&["push", "-q", "origin", "--delete", "v1.0.0"], dir).await;
    let created = world
        .created_by(async {
            let response = world.post_release("v1.0.0", &tagged, false).await;
            assert_eq!(response.status(), 409);
        })
        .await;
    assert_eq!(created.len(), 1, "{created:#?}");
}

#[sqlx::test]
async fn a_merge_request_pipeline_and_the_pipeline_after_the_merge_follow_the_rules(pool: PgPool) {
    let world = World::start(pool).await;
    let dir = world.dir();
    world.commit(".ferrisgit-ci.yml", PIPELINE_FILE).await;
    git(&["push", "-q", "origin", "HEAD:main"], dir).await;

    git(&["checkout", "-q", "-b", "code"], dir).await;
    world.commit("src/lib.rs", "// code\n").await;
    git(&["push", "-q", "origin", "code"], dir).await;
    let mr = world.open_merge_request("code").await;
    assert_eq!(
        world.job_names(&mr["pipeline"]).await,
        vec!["on-merge-request", "on-source-change", "unconditional"],
        "`changes` compares the merge with the tip of the target"
    );

    // Only what the source branch brings counts, not what the target did on its own.
    git(&["checkout", "-q", "main"], dir).await;
    world.commit("src/main_only.rs", "// main\n").await;
    git(&["push", "-q", "origin", "HEAD:main"], dir).await;
    git(&["checkout", "-q", "-b", "docs"], dir).await;
    git(&["reset", "-q", "--hard", "HEAD~1"], dir).await;
    world.commit("docs.txt", "docs\n").await;
    git(&["push", "-q", "origin", "docs"], dir).await;
    let docs_mr = world.open_merge_request("docs").await;
    assert_eq!(
        world.job_names(&docs_mr["pipeline"]).await,
        vec!["on-merge-request", "unconditional"]
    );

    // The pipeline after the merge is one of the target branch, and compares it with where it was.
    let mr_id = mr["id"].as_str().unwrap();
    let created = world
        .created_by(async {
            let merged: Value = post_empty(
                &world.client,
                world.addr,
                &world.jwt,
                &format!("/merge-requests/{mr_id}/merge"),
            )
            .await
            .json()
            .await
            .unwrap();
            assert_eq!(merged["outcome"], "merged", "{merged:#}");
        })
        .await;
    assert_eq!(created.len(), 1, "{created:#?}");
    assert_eq!(
        world.job_names(&created[0]).await,
        vec!["on-main", "on-source-change", "shipped", "unconditional"]
    );

    let docs_id = docs_mr["id"].as_str().unwrap();
    let created = world
        .created_by(async {
            let merged: Value = post_empty(
                &world.client,
                world.addr,
                &world.jwt,
                &format!("/merge-requests/{docs_id}/merge"),
            )
            .await
            .json()
            .await
            .unwrap();
            assert_eq!(merged["outcome"], "merged", "{merged:#}");
        })
        .await;
    assert_eq!(
        world.job_names(&created[0]).await,
        vec!["on-main", "shipped", "unconditional"]
    );
}

#[sqlx::test]
async fn a_push_of_many_refs_creates_a_limited_number_of_pipelines(pool: PgPool) {
    let world = World::start(pool).await;
    let dir = world.dir();
    world
        .commit(".ferrisgit-ci.yml", "stages: [test]\njobs:\n  unit:\n    stage: test\n    image: alpine\n    script: [\"true\"]\n")
        .await;
    git(&["push", "-q", "origin", "HEAD:main"], dir).await;
    for number in 0..25 {
        git(&["tag", &format!("v{number}")], dir).await;
    }

    let created = world
        .created_by(async {
            git(&["push", "-q", "origin", "--tags"], dir).await;
        })
        .await;

    assert_eq!(created.len(), 20);
}

const ONLY_ON_SOURCE_CHANGES: &str = "stages: [test]\njobs:\n  unit:\n    stage: test\n    image: alpine\n    script: [\"true\"]\n    rules:\n      changes: [\"src/**\"]\n";

#[sqlx::test]
async fn when_no_job_remains_a_push_creates_nothing_and_a_merge_request_is_not_blocked(
    pool: PgPool,
) {
    let world = World::start(pool).await;
    let dir = world.dir();
    let res = put(
        &world.client,
        world.addr,
        &world.jwt,
        &format!("/repositories/{}/settings", world.repo_id),
        &json!({ "pipelineMustSucceed": true }),
    )
    .await;
    assert_eq!(res.status(), 200);

    world
        .commit(".ferrisgit-ci.yml", ONLY_ON_SOURCE_CHANGES)
        .await;
    git(&["push", "-q", "origin", "HEAD:main"], dir).await;
    world.commit("README.md", "hello\n").await;
    let created = world
        .created_by(async {
            git(&["push", "-q", "origin", "HEAD:main"], dir).await;
        })
        .await;
    assert!(created.is_empty(), "{created:#?}");

    git(&["checkout", "-q", "-b", "readme"], dir).await;
    world.commit("README.md", "hello again\n").await;
    git(&["push", "-q", "origin", "readme"], dir).await;
    let mr = world.open_merge_request("readme").await;
    assert_eq!(mr["pipeline"]["status"], "success", "{mr:#}");
    assert!(world.job_names(&mr["pipeline"]).await.is_empty());
    let finished = get_json(
        &world.client,
        world.addr,
        &world.jwt,
        &format!("/pipelines/{}", mr["pipeline"]["id"].as_str().unwrap()),
    )
    .await;
    assert_ne!(finished["finishedAt"], Value::Null, "{finished:#}");

    let merged: Value = post_empty(
        &world.client,
        world.addr,
        &world.jwt,
        &format!("/merge-requests/{}/merge", mr["id"].as_str().unwrap()),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(merged["outcome"], "merged", "{merged:#}");
}

#[sqlx::test]
async fn wrong_rules_give_a_failed_pipeline_carrying_the_reason(pool: PgPool) {
    let world = World::start(pool).await;
    let dir = world.dir();

    for (rules, expected) in [
        ("{branch: [main]}", "unknown field `branch`"),
        (
            "{tags: []}",
            "job 'unit' has invalid `rules`: `tags` is empty",
        ),
    ] {
        world
            .commit(
                ".ferrisgit-ci.yml",
                &format!(
                    "stages: [test]\njobs:\n  unit:\n    stage: test\n    image: alpine\n    script: [\"true\"]\n    rules: {rules}\n"
                ),
            )
            .await;
        let created = world
            .created_by(async {
                git(&["push", "-q", "origin", "HEAD:main"], dir).await;
            })
            .await;
        assert_eq!(created[0]["status"], "failed", "{created:#?}");
        let error = created[0]["error"].as_str().unwrap();
        assert!(error.contains(expected), "{error}");
    }
}

#[sqlx::test]
async fn the_editor_routes_keep_the_rules_and_report_what_is_wrong_with_them(pool: PgPool) {
    let world = World::start(pool).await;
    let yaml = "stages: [test]\njobs:\n  unit:\n    stage: test\n    image: alpine\n    script: [\"true\"]\n    rules:\n      branches: [main]\n      merge_requests: true\n      changes: [\"src/**\"]\n";

    let parsed: Value = post_json(
        &world.client,
        world.addr,
        &world.jwt,
        "/pipeline-definitions/parse",
        &json!({ "yaml": yaml }),
    )
    .await;
    assert_eq!(parsed["problems"], json!([]), "{parsed:#}");
    assert_eq!(parsed["ignoredFields"], json!([]), "{parsed:#}");
    assert_eq!(
        parsed["definition"]["jobs"]["unit"]["rules"],
        json!({ "branches": ["main"], "mergeRequests": true, "changes": ["src/**"] })
    );

    let rendered: Value = post_json(
        &world.client,
        world.addr,
        &world.jwt,
        "/pipeline-definitions/render",
        &json!({ "definition": parsed["definition"] }),
    )
    .await;
    assert_eq!(rendered["problems"], json!([]), "{rendered:#}");
    let again: Value = post_json(
        &world.client,
        world.addr,
        &world.jwt,
        "/pipeline-definitions/parse",
        &json!({ "yaml": rendered["yaml"] }),
    )
    .await;
    assert_eq!(again["definition"], parsed["definition"], "{rendered:#}");
    assert!(
        rendered["yaml"]
            .as_str()
            .unwrap()
            .contains("merge_requests: true"),
        "{rendered:#}"
    );

    let job_without_rules = "stages: [test]\njobs:\n  unit:\n    stage: test\n    image: alpine\n    script: [\"true\"]\n";
    let parsed: Value = post_json(
        &world.client,
        world.addr,
        &world.jwt,
        "/pipeline-definitions/parse",
        &json!({ "yaml": job_without_rules }),
    )
    .await;
    assert!(
        parsed["definition"]["jobs"]["unit"].get("rules").is_none(),
        "{parsed:#}"
    );

    let wrong: Value = post_json(
        &world.client,
        world.addr,
        &world.jwt,
        "/pipeline-definitions/render",
        &json!({ "definition": { "stages": ["test"], "jobs": { "unit": { "stage": "test", "rules": { "tags": [] } } } } }),
    )
    .await;
    let problems = wrong["problems"].as_array().unwrap();
    assert_eq!(problems[0]["code"], "invalid_rules", "{wrong:#}");
    assert_eq!(problems[0]["job"], "unit");
    assert!(
        problems[0]["message"]
            .as_str()
            .unwrap()
            .contains("`tags` is empty"),
        "{wrong:#}"
    );

    let res = post(
        &world.client,
        world.addr,
        &world.jwt,
        "/pipeline-definitions/render",
        &json!({ "definition": { "stages": ["test"], "jobs": { "unit": { "stage": "test", "rules": { "bogus": [] } } } } }),
    )
    .await;
    assert_eq!(res.status(), 422);

    let unknown: Value = post_json(
        &world.client,
        world.addr,
        &world.jwt,
        "/pipeline-definitions/parse",
        &json!({ "yaml": "stages: [test]\njobs:\n  unit:\n    stage: test\n    image: alpine\n    script: [\"true\"]\n    rules:\n      bogus: true\n" }),
    )
    .await;
    assert_eq!(unknown["definition"], Value::Null, "{unknown:#}");
    assert_eq!(unknown["problems"][0]["code"], "invalid_yaml");
}
