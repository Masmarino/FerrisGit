//! The CI secrets routes: what they touch, what they refuse, and that a secret gets to a job like any masked variable.

mod common;

use common::git::{commit_file, git};
use common::http::{create_user, delete, get, login, mint_api_token, post, post_json, put};
use common::{USER_PASSWORD, assert_error};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;

const ADMIN: &str = "adminpassword123";

struct Setup {
    addr: SocketAddr,
    client: reqwest::Client,
    session: String,
    repo: String,
}

impl Setup {
    async fn new(pool: PgPool) -> Self {
        let addr = common::spawn_app(pool).await.addr;
        let client = reqwest::Client::new();
        let session = login(&client, addr, "admin", ADMIN).await;
        let repo: Value = post_json(
            &client,
            addr,
            &session,
            "/repositories",
            &json!({ "name": "hello", "visibility": "private" }),
        )
        .await;
        Self {
            addr,
            client,
            session,
            repo: repo["id"].as_str().unwrap().to_string(),
        }
    }

    fn secrets(&self) -> String {
        format!("/repositories/{}/ci-secrets", self.repo)
    }

    fn variables(&self) -> String {
        format!("/repositories/{}/ci-variables", self.repo)
    }

    async fn put_secret(&self, key: &str, value: &str) -> reqwest::Response {
        put(
            &self.client,
            self.addr,
            &self.session,
            &format!("{}/{key}", self.secrets()),
            &json!({ "value": value }),
        )
        .await
    }

    async fn post_variable(&self, key: &str, value: &str, masked: bool) -> reqwest::Response {
        post(
            &self.client,
            self.addr,
            &self.session,
            &self.variables(),
            &json!({ "key": key, "value": value, "masked": masked }),
        )
        .await
    }

    async fn list(&self, route: &str) -> Value {
        get(&self.client, self.addr, &self.session, route)
            .await
            .json()
            .await
            .unwrap()
    }
}

#[sqlx::test]
async fn a_secret_is_listed_among_secrets_and_as_a_masked_variable(pool: PgPool) {
    let s = Setup::new(pool).await;

    let created = s.put_secret("DEPLOY_TOKEN", "s3cret-value").await;
    assert_eq!(created.status(), 200);
    let body: Value = created.json().await.unwrap();
    assert_eq!(body["key"], "DEPLOY_TOKEN");
    assert!(!body.to_string().contains("s3cret-value"));

    assert_eq!(s.post_variable("PLAIN", "open", false).await.status(), 200);

    let secrets = s.list(&s.secrets()).await;
    assert_eq!(secrets.as_array().unwrap().len(), 1, "{secrets}");
    assert_eq!(secrets[0]["key"], "DEPLOY_TOKEN");
    assert!(!secrets.to_string().contains("s3cret-value"));

    let variables = s.list(&s.variables()).await;
    assert_eq!(variables.as_array().unwrap().len(), 2, "{variables}");
    let listed = variables
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["key"] == "DEPLOY_TOKEN")
        .unwrap();
    assert_eq!(listed["masked"], true);
}

#[sqlx::test]
async fn setting_a_secret_again_replaces_it(pool: PgPool) {
    let s = Setup::new(pool).await;
    assert_eq!(s.put_secret("TOKEN", "one").await.status(), 200);
    assert_eq!(s.put_secret("TOKEN", "two").await.status(), 200);
    assert_eq!(s.list(&s.secrets()).await.as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn a_bad_name_or_an_empty_value_is_refused(pool: PgPool) {
    let s = Setup::new(pool).await;

    assert_error(
        s.put_secret("1BAD", "value").await,
        400,
        "a CI secret name is letters, digits and _, not starting with a digit",
    )
    .await;
    assert_error(
        s.put_secret("TOKEN", "").await,
        400,
        "a CI secret can't be empty",
    )
    .await;
    assert_eq!(s.list(&s.secrets()).await, json!([]));
}

#[sqlx::test]
async fn a_secret_can_not_overwrite_a_plain_variable(pool: PgPool) {
    let s = Setup::new(pool).await;
    assert_eq!(s.post_variable("NAME", "open", false).await.status(), 200);

    let refused = s.put_secret("NAME", "hidden").await;

    assert_eq!(refused.status(), 409);
    assert_eq!(s.list(&s.secrets()).await, json!([]));
}

#[sqlx::test]
async fn a_secret_can_not_be_unmasked_through_the_variables_route(pool: PgPool) {
    let s = Setup::new(pool).await;
    assert_eq!(s.put_secret("NAME", "hidden").await.status(), 200);

    let refused = s.post_variable("NAME", "open", false).await;
    assert_eq!(refused.status(), 409);
    assert_eq!(
        s.list(&s.secrets()).await[0]["key"],
        "NAME",
        "it stays a secret"
    );

    // What the web interface does: replacing a masked variable with another masked one.
    assert_eq!(s.post_variable("NAME", "new", true).await.status(), 200);
}

#[sqlx::test]
async fn the_variables_route_can_still_mask_a_plain_variable(pool: PgPool) {
    let s = Setup::new(pool).await;
    assert_eq!(s.post_variable("NAME", "open", false).await.status(), 200);

    assert_eq!(s.post_variable("NAME", "hidden", true).await.status(), 200);

    assert_eq!(s.list(&s.secrets()).await[0]["key"], "NAME");
}

#[sqlx::test]
async fn deleting_only_touches_secrets(pool: PgPool) {
    let s = Setup::new(pool).await;
    assert_eq!(s.post_variable("PLAIN", "open", false).await.status(), 200);
    assert_eq!(s.put_secret("TOKEN", "hidden").await.status(), 200);
    let route = |key: &str| format!("{}/{key}", s.secrets());

    let plain = delete(&s.client, s.addr, &s.session, &route("PLAIN")).await;
    assert_eq!(
        plain.status(),
        404,
        "a plain variable isn't a secret to delete"
    );
    assert_eq!(s.list(&s.variables()).await.as_array().unwrap().len(), 2);

    let unknown = delete(&s.client, s.addr, &s.session, &route("NOPE")).await;
    assert_eq!(unknown.status(), 404);

    let deleted = delete(&s.client, s.addr, &s.session, &route("TOKEN")).await;
    assert_eq!(deleted.status(), 204);
    assert_eq!(s.list(&s.secrets()).await, json!([]));
    assert_eq!(s.list(&s.variables()).await.as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn secrets_are_for_maintainers_like_variables(pool: PgPool) {
    let s = Setup::new(pool).await;
    assert_eq!(s.put_secret("TOKEN", "hidden").await.status(), 200);
    create_user(&s.client, s.addr, &s.session, "bob").await;
    let bob = login(&s.client, s.addr, "bob", USER_PASSWORD).await;
    let added = post(
        &s.client,
        s.addr,
        &s.session,
        &format!("/repositories/{}/collaborators", s.repo),
        &json!({ "username": "bob", "role": "contributor" }),
    )
    .await;
    assert!(added.status().is_success());

    assert_eq!(
        get(&s.client, s.addr, &bob, &s.secrets()).await.status(),
        404
    );
    let route = format!("{}/TOKEN", s.secrets());
    let body = json!({ "value": "taken" });
    assert_eq!(
        put(&s.client, s.addr, &bob, &route, &body).await.status(),
        404
    );
    assert_eq!(delete(&s.client, s.addr, &bob, &route).await.status(), 404);
}

#[sqlx::test]
async fn an_api_token_needs_the_pipelines_scope(pool: PgPool) {
    let s = Setup::new(pool).await;
    assert_eq!(s.put_secret("TOKEN", "hidden").await.status(), 200);
    let token = |scopes: Value| {
        let (client, session, addr) = (s.client.clone(), s.session.clone(), s.addr);
        async move {
            let created: Value = post_json(
                &client,
                addr,
                &session,
                "/access-tokens",
                &json!({ "name": "ci", "scopes": scopes, "expiresInDays": 1 }),
            )
            .await;
            created["token"].as_str().unwrap().to_string()
        }
    };
    let route = format!("{}/NEW", s.secrets());
    let body = json!({ "value": "v" });

    let reader = token(json!(["pipelines:read"])).await;
    assert_eq!(
        get(&s.client, s.addr, &reader, &s.secrets()).await.status(),
        200
    );
    assert_error(
        put(&s.client, s.addr, &reader, &route, &body).await,
        403,
        "this token lacks the scope pipelines:write",
    )
    .await;

    let elsewhere = token(json!(["repositories:write"])).await;
    assert_error(
        get(&s.client, s.addr, &elsewhere, &s.secrets()).await,
        403,
        "this token lacks the scope pipelines:read",
    )
    .await;

    let writer = token(json!(["pipelines:write"])).await;
    assert_eq!(
        put(&s.client, s.addr, &writer, &route, &body)
            .await
            .status(),
        200
    );
}

#[sqlx::test]
async fn a_secret_reaches_a_job_and_is_masked_there(pool: PgPool) {
    let s = Setup::new(pool).await;
    assert_eq!(
        s.put_secret("DEPLOY_TOKEN", "s3cret-value").await.status(),
        200
    );
    assert_eq!(
        s.post_variable("PLAIN", "open-value", false).await.status(),
        200
    );

    let git_token = mint_api_token(&s.client, s.addr, &s.session, "dev").await;
    let parent = tempfile::tempdir().unwrap();
    let url = format!("http://admin:{git_token}@{}/admin/hello.git", s.addr);
    git(&["clone", "-q", &url, "work"], parent.path()).await;
    let work = parent.path().join("work");
    commit_file(
        &work,
        ".ferrisgit-ci.yml",
        "stages: [test]\njobs:\n  unit:\n    stage: test\n    image: alpine\n    script: [\"true\"]\n",
        "ci",
    )
    .await;
    git(&["push", "-q", "origin", "HEAD:main"], &work).await;

    let runner: Value = post_json(
        &s.client,
        s.addr,
        &s.session,
        "/admin/runners",
        &json!({ "name": "test", "tags": [] }),
    )
    .await;
    let claimed: Value = common::http::post_empty(
        &s.client,
        s.addr,
        runner["token"].as_str().unwrap(),
        "/runner/jobs/claim",
    )
    .await
    .json()
    .await
    .unwrap();

    assert_eq!(claimed["ciVariables"]["DEPLOY_TOKEN"], "s3cret-value");
    assert_eq!(claimed["ciVariables"]["PLAIN"], "open-value");
    assert_eq!(claimed["maskedValues"], json!(["s3cret-value"]));
}
