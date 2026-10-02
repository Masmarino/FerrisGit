// `PATCH /api/repositories/by-id/{id}`: description and visibility can be changed later, by the owner and Maintainers
// only, and the public catalog follows at once. The name never changes.

mod common;

use common::http::login;
use reqwest::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;

struct Harness {
    addr: SocketAddr,
    client: reqwest::Client,
    admin: String,
}

async fn spawn(pool: PgPool) -> Harness {
    let addr = common::spawn_app(pool).await.addr;

    let client = reqwest::Client::new();
    let admin = login(&client, addr, "admin", "adminpassword123").await;
    Harness {
        addr,
        client,
        admin,
    }
}

impl Harness {
    async fn create_user(&self, username: &str) -> String {
        self.client
            .post(format!("http://{}/api/admin/users", self.addr))
            .bearer_auth(&self.admin)
            .json(&json!({ "username": username, "email": format!("{username}@example.com"), "password": "password12345" }))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        login(&self.client, self.addr, username, "password12345").await
    }

    async fn create_repository(&self, visibility: &str) -> String {
        let repo: Value = self
            .client
            .post(format!("http://{}/api/repositories", self.addr))
            .bearer_auth(&self.admin)
            .json(&json!({ "name": "hello", "visibility": visibility, "description": "Au début" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        repo["id"].as_str().unwrap().to_string()
    }

    async fn add_collaborator(&self, repo_id: &str, username: &str, role: &str) {
        let status = self
            .client
            .post(format!(
                "http://{}/api/repositories/{repo_id}/collaborators",
                self.addr
            ))
            .bearer_auth(&self.admin)
            .json(&json!({ "username": username, "role": role }))
            .send()
            .await
            .unwrap()
            .status();
        assert_eq!(status, 204);
    }

    async fn patch(&self, jwt: &str, repo_id: &str, body: Value) -> reqwest::Response {
        self.client
            .patch(format!(
                "http://{}/api/repositories/by-id/{repo_id}",
                self.addr
            ))
            .bearer_auth(jwt)
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    async fn public_catalog(&self) -> Vec<String> {
        let list: Value = self
            .client
            .get(format!("http://{}/api/public/repositories", self.addr))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["name"].as_str().unwrap().to_string())
            .collect()
    }

    async fn anonymous_summary_status(&self, repo_id: &str) -> StatusCode {
        self.client
            .get(format!(
                "http://{}/api/public/repositories/by-id/{repo_id}",
                self.addr
            ))
            .send()
            .await
            .unwrap()
            .status()
    }
}

#[sqlx::test]
async fn the_owner_changes_the_description_and_the_visibility_and_the_name_stays(pool: PgPool) {
    let h = spawn(pool).await;
    let repo_id = h.create_repository("private").await;

    let res = h
        .patch(
            &h.admin,
            &repo_id,
            json!({ "description": "Une plateforme Git", "visibility": "public" }),
        )
        .await;

    assert_eq!(res.status(), 200);
    let updated: Value = res.json().await.unwrap();
    assert_eq!(updated["description"], "Une plateforme Git");
    assert_eq!(updated["visibility"], "public");
    assert_eq!(updated["name"], "hello");
    assert_eq!(updated["owner"], "admin");
    assert_eq!(updated["role"], "owner");
    let read_back: Value = h
        .client
        .get(format!(
            "http://{}/api/repositories/by-id/{repo_id}",
            h.addr
        ))
        .bearer_auth(&h.admin)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(read_back["description"], "Une plateforme Git");
    assert_eq!(read_back["visibility"], "public");
}

#[sqlx::test]
async fn only_the_fields_present_change_and_an_empty_description_is_allowed(pool: PgPool) {
    let h = spawn(pool).await;
    let repo_id = h.create_repository("public").await;

    let only_visibility: Value = h
        .patch(&h.admin, &repo_id, json!({ "visibility": "private" }))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(only_visibility["visibility"], "private");
    assert_eq!(only_visibility["description"], "Au début");

    let only_description: Value = h
        .patch(&h.admin, &repo_id, json!({ "description": "" }))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(only_description["description"], "");
    assert_eq!(only_description["visibility"], "private");

    let nothing = h.patch(&h.admin, &repo_id, json!({})).await;
    assert_eq!(nothing.status(), 200);
    let unchanged: Value = nothing.json().await.unwrap();
    assert_eq!(unchanged["visibility"], "private");
}

#[sqlx::test]
async fn an_unknown_visibility_is_refused_and_changes_nothing(pool: PgPool) {
    let h = spawn(pool).await;
    let repo_id = h.create_repository("private").await;

    let res = h
        .patch(
            &h.admin,
            &repo_id,
            json!({ "description": "Ne doit pas passer", "visibility": "secret" }),
        )
        .await;

    assert_eq!(res.status(), 400);
    let read_back: Value = h
        .client
        .get(format!(
            "http://{}/api/repositories/by-id/{repo_id}",
            h.addr
        ))
        .bearer_auth(&h.admin)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(read_back["description"], "Au début");
    assert_eq!(read_back["visibility"], "private");
}

#[sqlx::test]
async fn a_maintainer_may_edit_but_a_contributor_a_reader_and_a_stranger_may_not(pool: PgPool) {
    let h = spawn(pool).await;
    let repo_id = h.create_repository("private").await;
    let maintainer = h.create_user("maintainer").await;
    let contributor = h.create_user("contributor").await;
    let reader = h.create_user("reader").await;
    let stranger = h.create_user("stranger").await;
    h.add_collaborator(&repo_id, "maintainer", "maintainer")
        .await;
    h.add_collaborator(&repo_id, "contributor", "contributor")
        .await;
    h.add_collaborator(&repo_id, "reader", "reader").await;

    let ok = h
        .patch(
            &maintainer,
            &repo_id,
            json!({ "description": "Par un mainteneur" }),
        )
        .await;
    assert_eq!(ok.status(), 200);
    assert_eq!(ok.json::<Value>().await.unwrap()["role"], "maintainer");

    for (who, jwt) in [
        ("contributor", &contributor),
        ("reader", &reader),
        ("stranger", &stranger),
    ] {
        let res = h
            .patch(jwt, &repo_id, json!({ "visibility": "public" }))
            .await;
        assert_eq!(
            res.status(),
            404,
            "{who} must not even learn that it exists"
        );
    }
    let anonymous = h
        .client
        .patch(format!(
            "http://{}/api/repositories/by-id/{repo_id}",
            h.addr
        ))
        .json(&json!({ "visibility": "public" }))
        .send()
        .await
        .unwrap();
    assert_eq!(anonymous.status(), 401);
    assert_eq!(h.public_catalog().await, Vec::<String>::new());
}

#[sqlx::test]
async fn a_public_repository_that_is_made_private_leaves_the_public_pages_at_once_and_comes_back(
    pool: PgPool,
) {
    let h = spawn(pool).await;
    let repo_id = h.create_repository("public").await;
    assert_eq!(h.public_catalog().await, vec!["hello".to_string()]);
    assert_eq!(h.anonymous_summary_status(&repo_id).await, StatusCode::OK);

    h.patch(&h.admin, &repo_id, json!({ "visibility": "private" }))
        .await
        .error_for_status()
        .unwrap();

    assert!(h.public_catalog().await.is_empty());
    assert_eq!(
        h.anonymous_summary_status(&repo_id).await,
        StatusCode::NOT_FOUND
    );

    h.patch(&h.admin, &repo_id, json!({ "visibility": "public" }))
        .await
        .error_for_status()
        .unwrap();

    assert_eq!(h.public_catalog().await, vec!["hello".to_string()]);
    assert_eq!(h.anonymous_summary_status(&repo_id).await, StatusCode::OK);
}

#[sqlx::test]
async fn patching_an_unknown_repository_is_a_404(pool: PgPool) {
    let h = spawn(pool).await;

    let res = h
        .patch(
            &h.admin,
            "00000000-0000-0000-0000-000000000000",
            json!({ "description": "x" }),
        )
        .await;

    assert_eq!(res.status(), 404);
}
