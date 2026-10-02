mod common;

use std::net::SocketAddr;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use ferrisgit_api::login_rate_limiter::LoginRateLimiter;
use ferrisgit_api::state::AppState;
use reqwest::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use common::{ADMIN_PASSWORD, USER_PASSWORD};

struct Harness {
    addr: SocketAddr,
    client: reqwest::Client,
    admin: String,
}

/// A large public budget unless `customize` says otherwise, so only the throttling test ever meets it.
async fn spawn(pool: PgPool, customize: impl FnOnce(&mut AppState)) -> Harness {
    let app = common::spawn_app_with(
        pool,
        common::Options {
            trusted_proxy_cidrs: "127.0.0.1/32".to_string(),
            ..Default::default()
        },
        |state| {
            state.public_rate_limiter = Arc::new(LoginRateLimiter::with_limits(
                100_000,
                Duration::from_secs(60),
            ));
            customize(state);
        },
    )
    .await;
    let addr = app.addr;

    let client = reqwest::Client::new();
    let mut harness = Harness {
        addr,
        client,
        admin: String::new(),
    };
    harness.admin = harness.login("admin", ADMIN_PASSWORD).await;
    harness
}

impl Harness {
    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    async fn login(&self, username: &str, password: &str) -> String {
        let body: Value = self
            .client
            .post(self.url("/api/auth/login"))
            .json(&json!({ "username": username, "password": password }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        body["token"].as_str().expect("a session token").to_string()
    }

    async fn create_user(&self, username: &str) -> String {
        self.client
            .post(self.url("/api/admin/users"))
            .bearer_auth(&self.admin)
            .json(&json!({
                "username": username,
                "email": format!("{username}@example.com"),
                "password": USER_PASSWORD,
            }))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        self.login(username, USER_PASSWORD).await
    }

    async fn create_repo(&self, jwt: &str, body: Value) -> String {
        let repo: Value = self
            .client
            .post(self.url("/api/repositories"))
            .bearer_auth(jwt)
            .json(&body)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        repo["id"].as_str().unwrap().to_string()
    }

    async fn star(&self, jwt: &str, repository_id: &str) {
        self.client
            .post(self.url(&format!("/api/repositories/by-id/{repository_id}/star")))
            .bearer_auth(jwt)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
    }

    async fn get(&self, path: &str) -> reqwest::Response {
        self.client.get(self.url(path)).send().await.unwrap()
    }

    async fn get_as(&self, jwt: &str, path: &str) -> reqwest::Response {
        self.client
            .get(self.url(path))
            .bearer_auth(jwt)
            .send()
            .await
            .unwrap()
    }

    async fn get_json(&self, path: &str) -> Value {
        let response = self.get(path).await;
        assert_eq!(response.status(), StatusCode::OK, "GET {path}");
        response.json().await.unwrap()
    }

    async fn put_settings(&self, jwt: Option<&str>, body: Value) -> reqwest::Response {
        let mut request = self.client.put(self.url("/api/admin/settings")).json(&body);
        if let Some(jwt) = jwt {
            request = request.bearer_auth(jwt);
        }
        request.send().await.unwrap()
    }

    async fn set_switches(&self, body: Value) {
        let response = self.put_settings(Some(&self.admin), body).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    async fn api_token(&self) -> String {
        let body: Value = self
            .client
            .post(self.url("/api/tokens"))
            .bearer_auth(&self.admin)
            .json(&json!({ "name": "push" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        body["token"].as_str().unwrap().to_string()
    }
}

/// Status and raw body, to prove two refusals are indistinguishable.
async fn status_and_bytes(response: reqwest::Response) -> (StatusCode, Vec<u8>) {
    let status = response.status();
    (status, response.bytes().await.unwrap().to_vec())
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args([
            "-c",
            "user.email=dev@example.com",
            "-c",
            "user.name=Dev",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// Pushes one commit to `remote` and returns its sha. Runs in `spawn_blocking`: a blocking git subprocess would
/// starve the server task on the same runtime.
async fn push_initial_commit(remote: String) -> String {
    tokio::task::spawn_blocking(move || {
        let work = tempfile::tempdir().unwrap();
        let dir = work.path();
        std::fs::write(dir.join("README.md"), "# Hello\n\nA public project.\n").unwrap();
        std::fs::create_dir(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("src/main.rs"),
            "fn main() {\n    println!(\"hi\");\n}\n",
        )
        .unwrap();
        std::fs::write(dir.join("tool.py"), "print('hi')\n").unwrap();
        git(dir, &["init", "-q"]);
        git(dir, &["add", "."]);
        git(dir, &["commit", "-q", "-m", "Initial commit"]);
        git(dir, &["push", "-q", &remote, "main"]);
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    })
    .await
    .unwrap()
}

#[sqlx::test]
async fn anonymous_visitors_list_search_sort_and_page_public_repositories(pool: PgPool) {
    let h = spawn(pool, |_| {}).await;
    let bob = h.create_user("bob").await;
    h.client
        .post(h.url("/api/groups"))
        .bearer_auth(&h.admin)
        .json(&json!({ "name": "acme" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let alpha = h
        .create_repo(
            &h.admin,
            json!({ "name": "alpha", "visibility": "public", "description": "Rust tooling" }),
        )
        .await;
    let beta = h
        .create_repo(&bob, json!({ "name": "beta", "visibility": "public" }))
        .await;
    let gamma = h
        .create_repo(
            &h.admin,
            json!({ "name": "gamma", "visibility": "public", "groupPath": "acme" }),
        )
        .await;
    h.create_repo(
        &h.admin,
        json!({ "name": "secret", "visibility": "private", "description": "Rust secrets" }),
    )
    .await;
    h.star(&bob, &beta).await;
    h.star(&h.admin, &beta).await;
    h.star(&bob, &gamma).await;

    let listing = h.get("/api/public/repositories").await;
    assert_eq!(listing.status(), StatusCode::OK);
    assert_eq!(listing.headers()["cache-control"], "no-cache");
    let listing: Value = listing.json().await.unwrap();
    assert_eq!(listing["total"], 3);
    assert_eq!(listing["page"], 1);
    assert_eq!(listing["perPage"], 20);
    let items = listing["items"].as_array().unwrap();
    let names: Vec<&str> = items.iter().map(|i| i["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        vec!["beta", "gamma", "alpha"],
        "most starred first, then by name"
    );
    let gamma_item = &items[1];
    assert_eq!(gamma_item["id"], gamma.as_str());
    assert_eq!(gamma_item["path"], json!(["acme", "gamma"]));
    assert_eq!(gamma_item["owner"], "admin");
    assert_eq!(gamma_item["stars"], 1);
    let mut fields: Vec<&String> = gamma_item.as_object().unwrap().keys().collect();
    fields.sort();
    assert_eq!(
        fields,
        vec![
            "createdAt",
            "description",
            "id",
            "name",
            "owner",
            "path",
            "stars"
        ]
    );
    assert_eq!(items[2]["path"], json!(["admin", "alpha"]));
    assert_eq!(items[0]["path"], json!(["bob", "beta"]));
    assert!(
        !listing.to_string().contains("example.com"),
        "no e-mail address is ever listed"
    );

    let by_name = h.get_json("/api/public/repositories?sort=name").await;
    let names: Vec<&str> = by_name["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["alpha", "beta", "gamma"]);
    let newest = h.get_json("/api/public/repositories?sort=created").await;
    assert_eq!(newest["items"][0]["name"], "gamma");
    assert_eq!(newest["items"][2]["name"], "alpha");

    let search = h.get_json("/api/public/repositories?q=RUST").await;
    assert_eq!(search["total"], 1, "the private match stays hidden");
    assert_eq!(search["items"][0]["id"], alpha.as_str());
    let by_group_path = h.get_json("/api/public/repositories?q=acme/").await;
    assert_eq!(by_group_path["items"][0]["name"], "gamma");
    assert_eq!(by_group_path["total"], 1);

    let second_page = h
        .get_json("/api/public/repositories?sort=name&page=2&perPage=2")
        .await;
    assert_eq!(second_page["total"], 3);
    assert_eq!(second_page["page"], 2);
    assert_eq!(second_page["perPage"], 2);
    assert_eq!(second_page["items"].as_array().unwrap().len(), 1);
    assert_eq!(second_page["items"][0]["name"], "gamma");

    let too_long = "a".repeat(101);
    for invalid in [
        "perPage=51".to_string(),
        "perPage=0".to_string(),
        "page=0".to_string(),
        "page=101".to_string(),
        "page=two".to_string(),
        "sort=downloads".to_string(),
        format!("q={too_long}"),
    ] {
        let response = h.get(&format!("/api/public/repositories?{invalid}")).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{invalid}");
    }
}

#[sqlx::test]
async fn private_unknown_and_switched_off_repositories_answer_the_same_404(pool: PgPool) {
    let h = spawn(pool, |_| {}).await;
    let public = h
        .create_repo(&h.admin, json!({ "name": "hello", "visibility": "public" }))
        .await;
    let private = h
        .create_repo(
            &h.admin,
            json!({ "name": "secret", "visibility": "private" }),
        )
        .await;
    let unknown = Uuid::new_v4().to_string();
    let routes = [
        "/api/public/repositories/by-id/{id}",
        "/api/public/repositories/by-id/{id}/tree/main",
        "/api/public/repositories/by-id/{id}/tree/main/src",
        "/api/public/repositories/by-id/{id}/blob/main/README.md",
        "/api/public/repositories/by-id/{id}/readme/main",
        "/api/public/repositories/by-id/{id}/contributors/main",
        "/api/public/repositories/by-id/{id}/languages/main",
        "/api/public/repositories/by-id/{id}/commits",
        "/api/public/repositories/{id}/branches",
        "/api/public/repositories/{id}/tags",
        "/api/public/repositories/{id}/releases",
        "/api/public/repositories/{id}/releases/v1.0.0",
        "/api/public/repositories/{id}/releases/v1.0.0/assets/00000000-0000-0000-0000-000000000000",
    ];

    let reference = status_and_bytes(h.get(&routes[0].replace("{id}", &unknown)).await).await;
    assert_eq!(reference.0, StatusCode::NOT_FOUND);
    for route in routes {
        for id in [&unknown, &private] {
            let response = h.get(&route.replace("{id}", id)).await;
            assert_eq!(response.headers()["cache-control"], "no-cache");
            assert_eq!(
                status_and_bytes(response).await,
                reference,
                "{route} for {id}"
            );
        }
    }
    assert_eq!(
        h.get(&format!("/api/public/repositories/by-id/{public}"))
            .await
            .status(),
        StatusCode::OK
    );
    let path_not_found = status_and_bytes(h.get("/api/public/resolve/nobody/nothing").await).await;
    assert_eq!(path_not_found.0, StatusCode::NOT_FOUND);
    assert_eq!(
        status_and_bytes(h.get("/api/public/resolve/admin/secret").await).await,
        path_not_found
    );
    assert_eq!(
        h.get("/api/public/resolve/admin/hello").await.status(),
        StatusCode::OK
    );
    let config = h.get_json("/api/auth/config").await;
    assert_eq!(config["publicPagesEnabled"], true);

    h.set_switches(json!({ "publicPagesEnabled": false })).await;

    for route in routes {
        let response = h.get(&route.replace("{id}", &public)).await;
        assert_eq!(
            status_and_bytes(response).await,
            reference,
            "{route} switched off"
        );
    }
    assert_eq!(
        status_and_bytes(h.get("/api/public/resolve/admin/hello").await).await,
        path_not_found
    );
    assert_eq!(
        h.get("/api/public/repositories").await.status(),
        StatusCode::NOT_FOUND
    );
    let config = h.get_json("/api/auth/config").await;
    assert_eq!(config["publicPagesEnabled"], false);
    assert_eq!(
        h.get_as(&h.admin, &format!("/api/repositories/by-id/{public}"))
            .await
            .status(),
        StatusCode::OK,
        "signed-in access does not depend on the switch"
    );
}

#[sqlx::test]
async fn every_public_route_answers_exactly_what_the_authed_route_answers(pool: PgPool) {
    let h = spawn(pool, |_| {}).await;
    let bob = h.create_user("bob").await;
    let token = h.api_token().await;
    h.client
        .post(h.url("/api/groups"))
        .bearer_auth(&h.admin)
        .json(&json!({ "name": "acme" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let id = h
        .create_repo(
            &h.admin,
            json!({ "name": "hello", "visibility": "public", "description": "A demo" }),
        )
        .await;
    let grouped = h
        .create_repo(
            &h.admin,
            json!({ "name": "grouped", "visibility": "public", "groupPath": "acme" }),
        )
        .await;
    let sha = push_initial_commit(format!("http://admin:{token}@{}/admin/hello.git", h.addr)).await;
    h.star(&h.admin, &id).await;

    let release = |tag: &str, draft: bool| json!({ "tagName": tag, "targetCommitSha": sha, "title": tag, "notes": "Notes", "draft": draft });
    for (tag, draft) in [("v1.0.0", false), ("v2.0.0-rc", true)] {
        h.client
            .post(h.url(&format!("/api/repositories/{id}/releases")))
            .bearer_auth(&h.admin)
            .json(&release(tag, draft))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
    }
    let mut asset_ids = Vec::new();
    for tag in ["v1.0.0", "v2.0.0-rc"] {
        let part = reqwest::multipart::Part::bytes(format!("asset of {tag}").into_bytes())
            .file_name("archive.tar.gz")
            .mime_str("application/gzip")
            .unwrap();
        let asset: Value = h
            .client
            .post(h.url(&format!("/api/repositories/{id}/releases/{tag}/assets")))
            .bearer_auth(&h.admin)
            .multipart(reqwest::multipart::Form::new().part("file", part))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        asset_ids.push(asset["id"].as_str().unwrap().to_string());
    }
    let (published_asset, draft_asset) = (&asset_ids[0], &asset_ids[1]);

    // bob reads the public repository like any signed-in non-member: as a Reader, drafts hidden.
    let mirrored = [
        (
            "/api/resolve/admin/hello",
            "/api/public/resolve/admin/hello",
        ),
        (
            "/api/resolve/acme/grouped",
            "/api/public/resolve/acme/grouped",
        ),
        (
            "/api/repositories/by-id/{id}/tree/main",
            "/api/public/repositories/by-id/{id}/tree/main",
        ),
        (
            "/api/repositories/by-id/{id}/tree/main?lastCommit=false",
            "/api/public/repositories/by-id/{id}/tree/main?lastCommit=false",
        ),
        (
            "/api/repositories/by-id/{id}/tree/main/src",
            "/api/public/repositories/by-id/{id}/tree/main/src",
        ),
        (
            "/api/repositories/by-id/{id}/tree/nope",
            "/api/public/repositories/by-id/{id}/tree/nope",
        ),
        (
            "/api/repositories/by-id/{id}/blob/main/src/main.rs",
            "/api/public/repositories/by-id/{id}/blob/main/src/main.rs",
        ),
        (
            "/api/repositories/by-id/{id}/readme/main",
            "/api/public/repositories/by-id/{id}/readme/main",
        ),
        (
            "/api/repositories/by-id/{id}/contributors/main",
            "/api/public/repositories/by-id/{id}/contributors/main",
        ),
        (
            "/api/repositories/by-id/{id}/languages/main",
            "/api/public/repositories/by-id/{id}/languages/main",
        ),
        (
            "/api/repositories/by-id/{id}/commits",
            "/api/public/repositories/by-id/{id}/commits",
        ),
        (
            "/api/repositories/by-id/{id}/commits?ref=main",
            "/api/public/repositories/by-id/{id}/commits?ref=main",
        ),
        (
            "/api/repositories/{id}/branches",
            "/api/public/repositories/{id}/branches",
        ),
        (
            "/api/repositories/{id}/tags",
            "/api/public/repositories/{id}/tags",
        ),
        (
            "/api/repositories/{id}/releases",
            "/api/public/repositories/{id}/releases",
        ),
        (
            "/api/repositories/{id}/releases/v1.0.0",
            "/api/public/repositories/{id}/releases/v1.0.0",
        ),
    ];
    for (authed, public) in mirrored {
        let authed = authed.replace("{id}", &id);
        let public = public.replace("{id}", &id);
        let (authed_status, authed_body) = status_and_bytes(h.get_as(&bob, &authed).await).await;
        let response = h.get(&public).await;
        assert_eq!(response.headers()["cache-control"], "no-cache", "{public}");
        let (public_status, public_body) = status_and_bytes(response).await;
        assert_eq!(public_status, authed_status, "{public}");
        let authed_json: Value = serde_json::from_slice(&authed_body).unwrap();
        let public_json: Value = serde_json::from_slice(&public_body).unwrap();
        assert_eq!(public_json, authed_json, "{public}");
    }
    let resolved = h.get_json("/api/public/resolve/acme/grouped").await;
    assert_eq!(resolved["type"], "groupRepository");
    assert_eq!(resolved["repositoryId"], grouped.as_str());
    let tree = h
        .get_json(&format!("/api/public/repositories/by-id/{id}/tree/main"))
        .await;
    assert_eq!(
        tree.as_array().unwrap().len(),
        3,
        "the comparison ran on real content"
    );
    let releases = h
        .get_json(&format!("/api/public/repositories/{id}/releases"))
        .await;
    assert_eq!(releases.as_array().unwrap().len(), 1);
    assert_eq!(releases[0]["tagName"], "v1.0.0");
    let maintainer_view: Value = h
        .get_as(&h.admin, &format!("/api/repositories/{id}/releases"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        maintainer_view.as_array().unwrap().len(),
        2,
        "the Maintainer still sees the draft on the authed route"
    );

    let authed_summary: Value = h
        .get_as(&bob, &format!("/api/repositories/by-id/{id}"))
        .await
        .json()
        .await
        .unwrap();
    let mut public_summary = h
        .get_json(&format!("/api/public/repositories/by-id/{id}"))
        .await;
    assert!(public_summary["role"].is_null());
    assert_eq!(public_summary["isStarred"], false);
    assert_eq!(public_summary["starCount"], 1);
    assert_eq!(authed_summary["role"], "reader");
    public_summary["role"] = authed_summary["role"].clone();
    assert_eq!(public_summary, authed_summary);
    let group_summary = h
        .get_json(&format!("/api/public/repositories/by-id/{grouped}"))
        .await;
    assert_eq!(group_summary["path"], json!(["acme", "grouped"]));
    assert!(group_summary["role"].is_null());

    let download = h
        .get(&format!(
            "/api/public/repositories/{id}/releases/v1.0.0/assets/{published_asset}"
        ))
        .await;
    assert_eq!(download.status(), StatusCode::OK);
    assert_eq!(download.headers()["content-type"], "application/gzip");
    assert_eq!(
        download.headers()["content-disposition"],
        "attachment; filename=\"archive.tar.gz\""
    );
    assert_eq!(download.bytes().await.unwrap().as_ref(), b"asset of v1.0.0");

    let missing_release = status_and_bytes(
        h.get(&format!("/api/public/repositories/{id}/releases/v9"))
            .await,
    )
    .await;
    assert_eq!(missing_release.0, StatusCode::NOT_FOUND);
    assert_eq!(
        status_and_bytes(
            h.get(&format!("/api/public/repositories/{id}/releases/v2.0.0-rc"))
                .await
        )
        .await,
        missing_release,
        "a draft answers like a missing release"
    );
    assert_eq!(
        status_and_bytes(
            h.get(&format!(
                "/api/public/repositories/{id}/releases/v2.0.0-rc/assets/{draft_asset}"
            ))
            .await
        )
        .await,
        missing_release,
        "a draft's asset answers like a missing release"
    );
    assert_eq!(
        status_and_bytes(
            h.get(&format!(
                "/api/public/repositories/{id}/releases/v9/assets/{draft_asset}"
            ))
            .await
        )
        .await,
        missing_release
    );

    let with_bogus_session = h
        .client
        .get(h.url(&format!("/api/public/repositories/by-id/{id}")))
        .bearer_auth("not-a-session")
        .send()
        .await
        .unwrap();
    assert_eq!(
        with_bogus_session.status(),
        StatusCode::OK,
        "the public routes ignore the Authorization header"
    );
}

#[sqlx::test]
async fn the_public_budget_is_per_client_and_answers_429_with_retry_after(pool: PgPool) {
    let h = spawn(pool, |state| {
        state.public_rate_limiter =
            Arc::new(LoginRateLimiter::with_limits(3, Duration::from_secs(60)));
    })
    .await;
    let id = h
        .create_repo(&h.admin, json!({ "name": "hello", "visibility": "public" }))
        .await;
    let from = |ip: &'static str, path: String| {
        h.client
            .get(h.url(&path))
            .header("x-forwarded-for", ip)
            .send()
    };

    for path in [
        "/api/public/repositories".to_string(),
        format!("/api/public/repositories/by-id/{id}"),
        format!("/api/public/repositories/{}/branches", Uuid::new_v4()),
    ] {
        assert_ne!(
            from("203.0.113.7", path).await.unwrap().status(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }
    let refused = from(
        "203.0.113.7",
        format!("/api/public/repositories/by-id/{id}"),
    )
    .await
    .unwrap();
    assert_eq!(refused.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after: u64 = refused.headers()["retry-after"]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=60).contains(&retry_after));
    assert_eq!(refused.headers()["cache-control"], "no-cache");

    assert_eq!(
        from("203.0.113.8", "/api/public/repositories".to_string())
            .await
            .unwrap()
            .status(),
        StatusCode::OK,
        "another client keeps its own budget"
    );
    assert_eq!(
        h.client
            .get(h.url(&format!("/api/repositories/by-id/{id}")))
            .header("x-forwarded-for", "203.0.113.7")
            .bearer_auth(&h.admin)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK,
        "the signed-in routes do not spend the public budget"
    );
}

#[sqlx::test]
async fn only_an_administrator_changes_the_public_pages_switches(pool: PgPool) {
    let h = spawn(pool, |_| {}).await;
    let bob = h.create_user("bob").await;

    let settings: Value = h
        .get_as(&h.admin, "/api/admin/settings")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(settings["publicPagesEnabled"], true);
    assert_eq!(settings["seoIndexingEnabled"], false);

    let switch_both = json!({ "publicPagesEnabled": false, "seoIndexingEnabled": true });
    assert_eq!(
        h.put_settings(Some(&bob), switch_both.clone())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        h.put_settings(None, switch_both.clone()).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        h.get_as(&bob, "/api/admin/settings").await.status(),
        StatusCode::UNAUTHORIZED
    );
    let unchanged: Value = h
        .get_as(&h.admin, "/api/admin/settings")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(unchanged["publicPagesEnabled"], true);
    assert_eq!(unchanged["seoIndexingEnabled"], false);

    let updated: Value = h
        .put_settings(Some(&h.admin), switch_both)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(updated["publicPagesEnabled"], false);
    assert_eq!(updated["seoIndexingEnabled"], true);

    let partial: Value = h
        .put_settings(Some(&h.admin), json!({ "publicPagesEnabled": true }))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(partial["publicPagesEnabled"], true);
    assert_eq!(
        partial["seoIndexingEnabled"], true,
        "an omitted switch is left as it is"
    );
    let unrelated: Value = h
        .put_settings(Some(&h.admin), json!({ "jwtTtlHours": 24 }))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(unrelated["publicPagesEnabled"], true);
    assert_eq!(unrelated["seoIndexingEnabled"], true);
    assert_eq!(unrelated["registrationEnabled"], false);
}

#[sqlx::test]
async fn robots_txt_and_the_robots_header_follow_the_switches(pool: PgPool) {
    let h = spawn(pool, |_| {}).await;
    let pages = [
        "/",
        "/explore/some/spa/route",
        "/api/auth/config",
        "/api/public/repositories",
        "/health",
    ];

    let robots = h.get("/robots.txt").await;
    assert_eq!(robots.status(), StatusCode::OK);
    assert!(
        robots.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/plain")
    );
    assert_eq!(robots.text().await.unwrap(), "User-agent: *\nDisallow: /\n");
    for page in pages {
        assert_eq!(
            h.get(page).await.headers()["x-robots-tag"],
            "noindex, nofollow",
            "{page}"
        );
    }

    h.set_switches(json!({ "seoIndexingEnabled": true })).await;
    assert_eq!(
        h.get("/robots.txt").await.text().await.unwrap(),
        "User-agent: *\nDisallow: /api/\nDisallow: /account\nDisallow: /admin/\n"
    );
    for page in pages {
        assert!(
            h.get(page).await.headers().get("x-robots-tag").is_none(),
            "{page}"
        );
    }

    h.set_switches(json!({ "publicPagesEnabled": false })).await;
    assert_eq!(
        h.get("/robots.txt").await.text().await.unwrap(),
        "User-agent: *\nDisallow: /\n",
        "indexing needs the public pages too"
    );
    assert_eq!(
        h.get("/").await.headers()["x-robots-tag"],
        "noindex, nofollow"
    );
}

#[sqlx::test]
async fn the_signed_in_routes_still_refuse_anonymous_callers(pool: PgPool) {
    let h = spawn(pool, |_| {}).await;
    let id = h
        .create_repo(&h.admin, json!({ "name": "hello", "visibility": "public" }))
        .await;

    for path in [
        "/api/repositories".to_string(),
        format!("/api/repositories/by-id/{id}"),
        format!("/api/repositories/by-id/{id}/tree/main"),
        format!("/api/repositories/by-id/{id}/blob/main/README.md"),
        format!("/api/repositories/by-id/{id}/readme/main"),
        format!("/api/repositories/by-id/{id}/commits"),
        format!("/api/repositories/{id}/branches"),
        format!("/api/repositories/{id}/tags"),
        format!("/api/repositories/{id}/releases"),
        format!("/api/repositories/{id}/collaborators"),
        format!("/api/repositories/{id}/settings"),
        format!("/api/repositories/{id}/issues"),
        "/api/resolve/admin/hello".to_string(),
        "/api/admin/settings".to_string(),
    ] {
        assert_eq!(
            h.get(&path).await.status(),
            StatusCode::UNAUTHORIZED,
            "{path}"
        );
    }
}
