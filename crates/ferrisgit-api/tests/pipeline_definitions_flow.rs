// The two routes the pipeline builder talks to: read a pipeline file with the server's own parser, and write a drawn
// definition back as YAML. Both are pure, so what matters is the shape of the answers and what they report.

mod common;

use common::http::login;

use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;

const FILE: &str = "stages: [build, test]\njobs:\n  compile:\n    stage: build\n    image: rust:1\n    script:\n      - cargo build\n  check:\n    stage: test\n    image: rust:1\n    script:\n      - cargo test\n    needs: [compile]\n    variables:\n      RUST_LOG: debug\n    cache: [cargo-registry]\n";

struct Api {
    addr: SocketAddr,
    client: reqwest::Client,
    session: String,
}

impl Api {
    async fn start(pool: PgPool) -> Self {
        let addr = common::spawn_app(pool).await.addr;
        let client = reqwest::Client::new();
        let session = login(&client, addr, "admin", "adminpassword123").await;
        Self {
            addr,
            client,
            session,
        }
    }

    async fn post(&self, path: &str, body: &Value) -> reqwest::Response {
        self.client
            .post(format!(
                "http://{}/api/pipeline-definitions/{path}",
                self.addr
            ))
            .bearer_auth(&self.session)
            .json(body)
            .send()
            .await
            .unwrap()
    }

    async fn parse(&self, yaml: &str) -> Value {
        let res = self.post("parse", &json!({ "yaml": yaml })).await;
        assert_eq!(res.status(), 200);
        res.json().await.unwrap()
    }

    async fn render(&self, definition: &Value) -> Value {
        let res = self
            .post("render", &json!({ "definition": definition }))
            .await;
        assert_eq!(res.status(), 200);
        res.json().await.unwrap()
    }
}

#[sqlx::test]
async fn both_routes_refuse_anonymous_callers(pool: PgPool) {
    let api = Api::start(pool).await;

    for (path, body) in [
        ("parse", json!({ "yaml": FILE })),
        (
            "render",
            json!({ "definition": { "stages": [], "jobs": {} } }),
        ),
    ] {
        let res = api
            .client
            .post(format!(
                "http://{}/api/pipeline-definitions/{path}",
                api.addr
            ))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 401, "{path}");
    }
}

#[sqlx::test]
async fn a_valid_file_is_read_into_stages_and_jobs_with_every_field(pool: PgPool) {
    let api = Api::start(pool).await;

    let body = api.parse(FILE).await;

    assert_eq!(body["problems"], json!([]));
    assert_eq!(body["warnings"], json!([]));
    assert_eq!(body["ignoredFields"], json!([]));
    assert_eq!(body["hasComments"], json!(false));
    assert_eq!(
        body["definition"],
        json!({
            "stages": ["build", "test"],
            "jobs": {
                "check": {
                    "stage": "test", "image": "rust:1", "script": ["cargo test"],
                    "variables": { "RUST_LOG": "debug" }, "needs": ["compile"],
                    "tags": [], "cache": ["cargo-registry"]
                },
                "compile": {
                    "stage": "build", "image": "rust:1", "script": ["cargo build"],
                    "variables": {}, "needs": [], "tags": [], "cache": []
                }
            }
        })
    );
}

#[sqlx::test]
async fn a_file_with_several_mistakes_still_opens_and_lists_them_all(pool: PgPool) {
    let api = Api::start(pool).await;
    let yaml = "stages: [build]\njobs:\n  a:\n    stage: nowhere\n    image: rust:1\n    script: [x]\n  b:\n    stage: build\n    image: rust:1\n    script: [x]\n    needs: [ghost]\n    cache: [Bad Key]\n";

    let body = api.parse(yaml).await;

    assert!(body["definition"].is_object(), "the builder can open it");
    let codes: Vec<&str> = body["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["code"].as_str().unwrap())
        .collect();
    assert_eq!(
        codes,
        ["unknown_stage", "unknown_dependency", "invalid_cache_key"]
    );
    assert_eq!(body["problems"][0]["job"], "a");
    assert_eq!(body["problems"][0]["stage"], "nowhere");
    assert_eq!(body["problems"][1]["dependency"], "ghost");
    assert_eq!(body["problems"][2]["key"], "Bad Key");
    assert!(
        body["problems"][0]["message"]
            .as_str()
            .unwrap()
            .contains("not declared in `stages`")
    );
}

#[sqlx::test]
async fn a_dependency_cycle_is_reported_with_its_jobs(pool: PgPool) {
    let api = Api::start(pool).await;
    let yaml = "stages: [build]\njobs:\n  a:\n    stage: build\n    image: i\n    script: [x]\n    needs: [b]\n  b:\n    stage: build\n    image: i\n    script: [x]\n    needs: [a]\n";

    let body = api.parse(yaml).await;

    assert_eq!(body["problems"][0]["code"], "needs_cycle");
    assert_eq!(body["problems"][0]["jobs"], json!(["a", "b"]));
}

#[sqlx::test]
async fn malformed_yaml_gives_no_definition_and_an_invalid_yaml_problem(pool: PgPool) {
    let api = Api::start(pool).await;

    let body = api.parse("stages: [build\njobs:").await;

    assert!(body["definition"].is_null());
    assert_eq!(body["problems"][0]["code"], "invalid_yaml");
}

#[sqlx::test]
async fn what_a_rewrite_would_lose_is_reported_before_anyone_rewrites(pool: PgPool) {
    let api = Api::start(pool).await;
    let yaml = "# build everything\nstages: [build]\ninclude: other.yml\njobs:\n  a:\n    stage: build\n    image: i\n    script: [x]\n    when: manual\n";

    let body = api.parse(yaml).await;

    assert_eq!(body["hasComments"], json!(true));
    assert_eq!(body["ignoredFields"], json!(["include", "jobs.a.when"]));
}

#[sqlx::test]
async fn a_drawn_definition_is_written_as_yaml_grouped_by_stage(pool: PgPool) {
    let api = Api::start(pool).await;
    let definition = json!({
        "stages": ["build", "test"],
        "jobs": {
            "unit": { "stage": "test", "image": "rust:1", "script": ["cargo test"], "needs": ["compile"] },
            "compile": { "stage": "build", "image": "rust:1", "script": ["cargo build"] }
        }
    });

    let body = api.render(&definition).await;

    assert_eq!(body["problems"], json!([]));
    assert_eq!(
        body["yaml"],
        "stages:\n- build\n- test\njobs:\n  compile:\n    stage: build\n    image: rust:1\n    script:\n    - cargo build\n  unit:\n    stage: test\n    image: rust:1\n    script:\n    - cargo test\n    needs:\n    - compile\n"
    );
}

#[sqlx::test]
async fn a_draft_is_rendered_with_its_problems_and_warnings_instead_of_being_refused(pool: PgPool) {
    let api = Api::start(pool).await;
    let draft = json!({
        "stages": ["build", "build"],
        "jobs": { "new-job": { "stage": "later" } }
    });

    let body = api.render(&draft).await;

    assert!(body["yaml"].as_str().unwrap().contains("new-job"));
    assert_eq!(body["problems"][0]["code"], "unknown_stage");
    let warnings: Vec<&str> = body["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["code"].as_str().unwrap())
        .collect();
    assert_eq!(warnings, ["duplicate_stage", "empty_image", "empty_script"]);
}

#[sqlx::test]
async fn reading_what_was_written_gives_the_same_definition_back(pool: PgPool) {
    let api = Api::start(pool).await;
    let first = api.parse(FILE).await;

    let rendered = api.render(&first["definition"]).await;
    let second = api.parse(rendered["yaml"].as_str().unwrap()).await;

    assert_eq!(second["definition"], first["definition"]);
    assert_eq!(second["problems"], json!([]));
}

#[sqlx::test]
async fn a_body_over_256_kib_is_refused(pool: PgPool) {
    let api = Api::start(pool).await;
    let huge = format!("stages: [a]\n# {}\n", "x".repeat(300 * 1024));

    let res = api.post("parse", &json!({ "yaml": huge })).await;

    assert_eq!(res.status(), 413);
}

// Saving: the editor's file goes to a new branch with a merge request, never to the default branch.

mod saving {
    use super::*;
    use crate::common::USER_PASSWORD;
    use crate::common::git::{commit_file, git, push_files};
    use crate::common::http::{add_collaborator, create_user, get_json, post, put};

    const OLD: &str = "stages: [build]\njobs:\n  a:\n    stage: build\n    image: rust:1\n    script: [cargo build]\n";
    const NEW: &str = "stages: [build]\njobs:\n  a:\n    stage: build\n    image: rust:1\n    script: [cargo build, cargo test]\n";

    struct Repo {
        addr: SocketAddr,
        client: reqwest::Client,
        admin: String,
        id: String,
        clone_url: String,
    }

    impl Repo {
        /// A private repository owned by the admin, with `files` pushed to `main` unless there are none.
        async fn with(pool: PgPool, files: &[(&str, &[u8])]) -> Self {
            let addr = common::spawn_app(pool).await.addr;
            let client = reqwest::Client::new();
            let admin = login(&client, addr, "admin", "adminpassword123").await;
            let token = common::http::mint_api_token(&client, addr, &admin, "git").await;
            let repo: Value = common::http::post_json(
                &client,
                addr,
                &admin,
                "/repositories",
                &json!({ "name": "hello", "visibility": "private" }),
            )
            .await;
            let clone_url = format!("http://admin:{token}@{addr}/admin/hello.git");
            if !files.is_empty() {
                assert!(push_files(&clone_url, "main", "root", files).await);
            }
            Self {
                addr,
                client,
                admin,
                id: repo["id"].as_str().unwrap().to_string(),
                clone_url,
            }
        }

        fn path(&self, suffix: &str) -> String {
            format!("/repositories/{}/pipeline-definition{suffix}", self.id)
        }

        async fn file_as(&self, token: &str) -> reqwest::Response {
            common::http::get(&self.client, self.addr, token, &self.path("")).await
        }

        async fn file(&self) -> Value {
            let res = self.file_as(&self.admin).await;
            assert_eq!(res.status(), 200);
            res.json().await.unwrap()
        }

        async fn propose_as(&self, token: &str, body: Value) -> reqwest::Response {
            post(
                &self.client,
                self.addr,
                token,
                &self.path("/proposal"),
                &body,
            )
            .await
        }
    }

    #[sqlx::test]
    async fn an_empty_repository_has_no_file_and_no_branch_to_start_from(pool: PgPool) {
        let repo = Repo::with(pool, &[]).await;

        let file = repo.file().await;

        assert_eq!(file["path"], ".ferrisgit-ci.yml");
        assert!(file["branch"].is_null() && file["baseSha"].is_null() && file["yaml"].is_null());
    }

    #[sqlx::test]
    async fn the_editor_starts_from_the_default_branchs_file_at_the_configured_path(pool: PgPool) {
        let repo = Repo::with(
            pool,
            &[("ci/pipeline.yml", OLD.as_bytes()), ("README.md", b"hi\n")],
        )
        .await;
        put(
            &repo.client,
            repo.addr,
            &repo.admin,
            &format!("/repositories/{}/settings", repo.id),
            &json!({ "pipelineFilePath": "ci/pipeline.yml" }),
        )
        .await
        .error_for_status()
        .unwrap();

        let file = repo.file().await;

        assert_eq!(file["path"], "ci/pipeline.yml");
        assert_eq!(file["branch"], "main");
        assert_eq!(file["yaml"], OLD);
        assert_eq!(file["baseSha"].as_str().unwrap().len(), 40);
    }

    #[sqlx::test]
    async fn a_proposal_lands_on_a_new_branch_with_a_merge_request_and_main_is_untouched(
        pool: PgPool,
    ) {
        let repo = Repo::with(pool, &[(".ferrisgit-ci.yml", OLD.as_bytes())]).await;
        let before = repo.file().await;

        let res = repo
            .propose_as(
                &repo.admin,
                json!({ "yaml": NEW, "baseSha": before["baseSha"], "title": "Run the tests", "description": "Adds `cargo test`." }),
            )
            .await;

        assert_eq!(res.status(), 200);
        let proposed: Value = res.json().await.unwrap();
        let branch = proposed["branch"].as_str().unwrap();
        assert!(branch.starts_with("pipeline-editor/"), "{branch}");

        // The default branch still has the old file.
        assert_eq!(repo.file().await["yaml"], OLD);
        // The branch holds the new one, on top of main's tip.
        let dir = tempfile::tempdir().unwrap();
        git(&["clone", "-q", &repo.clone_url, "work"], dir.path()).await;
        let work = dir.path().join("work");
        let on_branch = git(
            &["show", &format!("origin/{branch}:.ferrisgit-ci.yml")],
            &work,
        )
        .await;
        assert_eq!(on_branch.trim_end(), NEW.trim_end());
        let parent = git(&["rev-parse", &format!("origin/{branch}^")], &work).await;
        assert_eq!(parent, before["baseSha"].as_str().unwrap());
        let author = git(
            &["log", "-1", "--format=%an", &format!("origin/{branch}")],
            &work,
        )
        .await;
        assert_eq!(author, "admin");

        // And a merge request waits for review.
        let mrs: Value = get_json(
            &repo.client,
            repo.addr,
            &repo.admin,
            &format!("/repositories/{}/merge-requests", repo.id),
        )
        .await;
        let mr = &mrs.as_array().unwrap()[0];
        assert_eq!(mr["id"], proposed["mergeRequestId"]);
        assert_eq!(mr["sourceBranch"], branch);
        assert_eq!(mr["targetBranch"], "main");
        assert_eq!(mr["title"], "Run the tests");
        assert_eq!(mr["status"], "open");
    }

    #[sqlx::test]
    async fn a_pipeline_file_can_be_proposed_to_a_repository_that_has_none(pool: PgPool) {
        let repo = Repo::with(pool, &[("README.md", b"hi\n")]).await;
        let base = repo.file().await["baseSha"].clone();

        let res = repo
            .propose_as(&repo.admin, json!({ "yaml": NEW, "baseSha": base }))
            .await;

        assert_eq!(res.status(), 200);
    }

    #[sqlx::test]
    async fn what_the_engine_would_refuse_or_what_changes_nothing_is_a_400(pool: PgPool) {
        let repo = Repo::with(pool, &[(".ferrisgit-ci.yml", OLD.as_bytes())]).await;
        let base = repo.file().await["baseSha"].clone();
        let unknown_stage = NEW.replace("stage: build", "stage: deploy");

        for yaml in [unknown_stage.as_str(), "jobs: [", OLD] {
            let res = repo
                .propose_as(&repo.admin, json!({ "yaml": yaml, "baseSha": base }))
                .await;
            assert_eq!(res.status(), 400, "{yaml:?}");
        }
    }

    #[sqlx::test]
    async fn a_file_that_changed_since_the_editor_opened_is_a_409(pool: PgPool) {
        let repo = Repo::with(pool, &[(".ferrisgit-ci.yml", OLD.as_bytes())]).await;
        let opened = repo.file().await["baseSha"].clone();
        let changed = OLD.replace("cargo build", "cargo build --release");
        let dir = tempfile::tempdir().unwrap();
        git(&["clone", "-q", &repo.clone_url, "work"], dir.path()).await;
        let work = dir.path().join("work");
        commit_file(&work, ".ferrisgit-ci.yml", &changed, "someone else").await;
        git(&["push", "-q", "origin", "HEAD:main"], &work).await;

        let res = repo
            .propose_as(&repo.admin, json!({ "yaml": NEW, "baseSha": opened }))
            .await;

        assert_eq!(res.status(), 409);
    }

    #[sqlx::test]
    async fn a_reader_cannot_open_or_save_and_a_contributor_can(pool: PgPool) {
        let repo = Repo::with(pool, &[(".ferrisgit-ci.yml", OLD.as_bytes())]).await;
        create_user(&repo.client, repo.addr, &repo.admin, "reader").await;
        create_user(&repo.client, repo.addr, &repo.admin, "writer").await;
        add_collaborator(
            &repo.client,
            repo.addr,
            &repo.admin,
            &repo.id,
            "reader",
            "reader",
        )
        .await;
        add_collaborator(
            &repo.client,
            repo.addr,
            &repo.admin,
            &repo.id,
            "writer",
            "contributor",
        )
        .await;
        let reader = login(&repo.client, repo.addr, "reader", USER_PASSWORD).await;
        let writer = login(&repo.client, repo.addr, "writer", USER_PASSWORD).await;
        let base = repo.file().await["baseSha"].clone();

        // An insufficient role answers like a missing repository, as everywhere else.
        assert_eq!(repo.file_as(&reader).await.status(), 404);
        let denied = repo
            .propose_as(&reader, json!({ "yaml": NEW, "baseSha": base }))
            .await;
        assert_eq!(denied.status(), 404);
        assert_eq!(repo.file_as(&writer).await.status(), 200);
        let allowed = repo
            .propose_as(&writer, json!({ "yaml": NEW, "baseSha": base }))
            .await;
        assert_eq!(allowed.status(), 200);
    }

    #[sqlx::test]
    async fn the_profile_says_what_the_default_branch_is_made_of(pool: PgPool) {
        let repo = Repo::with(
            pool,
            &[
                ("Cargo.toml", b"[workspace]\nmembers = [\"crates/*\"]\n"),
                ("rust-toolchain.toml", b"[toolchain]\nchannel = \"1.86.0\"\n"),
                ("crates/api/Cargo.toml", b"[package]\nname = \"api\"\n"),
                ("crates/api/src/main.rs", b"fn main() {}\n"),
                (".sqlx/query-1.json", b"{}"),
                ("web/package.json", br#"{"scripts":{"test":"vitest run","lint":"eslint ."},"devDependencies":{"vitest":"3"}}"#),
                ("web/pnpm-lock.yaml", b"lockfileVersion: '9.0'\n"),
                ("web/.nvmrc", b"22\n"),
                ("web/node_modules/dep/package.json", b"{}"),
                ("Dockerfile", b"FROM scratch\n"),
            ],
        )
        .await;

        let res =
            common::http::get(&repo.client, repo.addr, &repo.admin, &repo.path("/profile")).await;
        assert_eq!(res.status(), 200);
        let profile: Value = res.json().await.unwrap();

        assert_eq!(
            profile["projects"],
            json!([
                { "dir": "", "evidence": ["Cargo.toml", "rust-toolchain.toml"], "kind": "rust", "workspace": true, "toolchain": "1.86.0", "sqlxOffline": true, "sqlxPostgres": false },
                {
                    "dir": "web",
                    "evidence": ["web/package.json", "web/pnpm-lock.yaml", "web/.nvmrc"],
                    "kind": "node",
                    "packageManager": "pnpm",
                    "nodeVersion": "22",
                    "scripts": { "lint": "eslint .", "test": "vitest run" },
                    "framework": null,
                    "testRunner": "vitest"
                }
            ])
        );
        assert_eq!(profile["dockerfiles"], json!([""]));
        assert_eq!(profile["helmCharts"], json!([]));
    }

    #[sqlx::test]
    async fn an_empty_repository_has_an_empty_profile(pool: PgPool) {
        let repo = Repo::with(pool, &[]).await;

        let profile: Value =
            get_json(&repo.client, repo.addr, &repo.admin, &repo.path("/profile")).await;

        assert_eq!(
            profile,
            json!({ "projects": [], "dockerfiles": [], "helmCharts": [] })
        );
    }

    #[sqlx::test]
    async fn the_profile_is_for_contributors_like_the_file(pool: PgPool) {
        let repo = Repo::with(pool, &[("go.mod", b"go 1.22\n")]).await;
        create_user(&repo.client, repo.addr, &repo.admin, "reader").await;
        add_collaborator(
            &repo.client,
            repo.addr,
            &repo.admin,
            &repo.id,
            "reader",
            "reader",
        )
        .await;
        let reader = login(&repo.client, repo.addr, "reader", USER_PASSWORD).await;

        let res = common::http::get(&repo.client, repo.addr, &reader, &repo.path("/profile")).await;

        assert_eq!(res.status(), 404);
    }

    #[sqlx::test]
    async fn someone_with_no_access_to_a_private_repository_learns_nothing(pool: PgPool) {
        let repo = Repo::with(pool, &[(".ferrisgit-ci.yml", OLD.as_bytes())]).await;
        create_user(&repo.client, repo.addr, &repo.admin, "stranger").await;
        let stranger = login(&repo.client, repo.addr, "stranger", USER_PASSWORD).await;

        assert_eq!(repo.file_as(&stranger).await.status(), 404);
        let res = repo
            .propose_as(&stranger, json!({ "yaml": NEW, "baseSha": "x" }))
            .await;
        assert_eq!(res.status(), 404);
    }
}
