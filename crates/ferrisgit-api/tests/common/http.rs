//! Bare REST calls for the tests that talk to the API with a plain `reqwest::Client`. `path` is relative to `/api`.

use serde::Serialize;
use serde_json::Value;
use std::net::SocketAddr;

fn url(addr: SocketAddr, path: &str) -> String {
    format!("http://{addr}/api{path}")
}

/// Logs in and returns the session token. Needs a server whose MFA is off (`spawn_app`).
pub async fn login(
    client: &reqwest::Client,
    addr: SocketAddr,
    username: &str,
    password: &str,
) -> String {
    let res: Value = post_anon(
        client,
        addr,
        "/auth/login",
        &serde_json::json!({ "username": username, "password": password }),
    )
    .await
    .json()
    .await
    .unwrap();
    res["token"].as_str().unwrap().to_string()
}

pub async fn get(
    client: &reqwest::Client,
    addr: SocketAddr,
    token: &str,
    path: &str,
) -> reqwest::Response {
    client
        .get(url(addr, path))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
}

pub async fn get_json(
    client: &reqwest::Client,
    addr: SocketAddr,
    token: &str,
    path: &str,
) -> Value {
    get(client, addr, token, path).await.json().await.unwrap()
}

pub async fn post(
    client: &reqwest::Client,
    addr: SocketAddr,
    token: &str,
    path: &str,
    body: &impl Serialize,
) -> reqwest::Response {
    client
        .post(url(addr, path))
        .bearer_auth(token)
        .json(body)
        .send()
        .await
        .unwrap()
}

pub async fn post_json(
    client: &reqwest::Client,
    addr: SocketAddr,
    token: &str,
    path: &str,
    body: &impl Serialize,
) -> Value {
    post(client, addr, token, path, body)
        .await
        .json()
        .await
        .unwrap()
}

/// Like [`post`], but fails the test on a non-success status.
pub async fn post_ok(
    client: &reqwest::Client,
    addr: SocketAddr,
    token: &str,
    path: &str,
    body: &impl Serialize,
) -> reqwest::Response {
    post(client, addr, token, path, body)
        .await
        .error_for_status()
        .unwrap()
}

pub async fn post_empty(
    client: &reqwest::Client,
    addr: SocketAddr,
    token: &str,
    path: &str,
) -> reqwest::Response {
    client
        .post(url(addr, path))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
}

pub async fn post_anon(
    client: &reqwest::Client,
    addr: SocketAddr,
    path: &str,
    body: &impl Serialize,
) -> reqwest::Response {
    client
        .post(url(addr, path))
        .json(body)
        .send()
        .await
        .unwrap()
}

pub async fn put(
    client: &reqwest::Client,
    addr: SocketAddr,
    token: &str,
    path: &str,
    body: &impl Serialize,
) -> reqwest::Response {
    client
        .put(url(addr, path))
        .bearer_auth(token)
        .json(body)
        .send()
        .await
        .unwrap()
}

pub async fn patch(
    client: &reqwest::Client,
    addr: SocketAddr,
    token: &str,
    path: &str,
    body: &impl Serialize,
) -> reqwest::Response {
    client
        .patch(url(addr, path))
        .bearer_auth(token)
        .json(body)
        .send()
        .await
        .unwrap()
}

pub async fn delete(
    client: &reqwest::Client,
    addr: SocketAddr,
    token: &str,
    path: &str,
) -> reqwest::Response {
    client
        .delete(url(addr, path))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
}

/// Creates a user through the admin API, with the shared test password and `<username>@example.com`.
pub async fn create_user(
    client: &reqwest::Client,
    addr: SocketAddr,
    admin_token: &str,
    username: &str,
) {
    post_ok(
        client,
        addr,
        admin_token,
        "/admin/users",
        &serde_json::json!({
            "username": username,
            "email": format!("{username}@example.com"),
            "password": super::USER_PASSWORD,
        }),
    )
    .await;
}

/// Adds `username` to the repository and asserts the 204.
pub async fn add_collaborator(
    client: &reqwest::Client,
    addr: SocketAddr,
    owner_token: &str,
    repo_id: &str,
    username: &str,
    role: &str,
) {
    let status = post(
        client,
        addr,
        owner_token,
        &format!("/repositories/{repo_id}/collaborators"),
        &serde_json::json!({ "username": username, "role": role }),
    )
    .await
    .status();
    assert_eq!(
        status, 204,
        "expected adding {username} as {role} to return 204"
    );
}

/// Mints a personal access token for the session's user.
pub async fn mint_api_token(
    client: &reqwest::Client,
    addr: SocketAddr,
    token: &str,
    name: &str,
) -> String {
    let res = post_json(
        client,
        addr,
        token,
        "/tokens",
        &serde_json::json!({ "name": name }),
    )
    .await;
    res["token"].as_str().unwrap().to_string()
}

/// A server with MFA off and a client, for the tests that read better as `api.post(jwt, path, body)`.
/// `post` and `patch` fail the test on a non-success status and return the JSON body (`Null` when there is none).
pub struct Api {
    pub client: reqwest::Client,
    pub addr: SocketAddr,
}

impl Api {
    pub async fn start(pool: sqlx::PgPool) -> Self {
        Self {
            client: reqwest::Client::new(),
            addr: super::spawn_app(pool).await.addr,
        }
    }

    pub async fn login(&self, username: &str, password: &str) -> String {
        login(&self.client, self.addr, username, password).await
    }

    pub async fn post(&self, token: &str, path: &str, body: Value) -> Value {
        let res = post(&self.client, self.addr, token, path, &body).await;
        assert!(
            res.status().is_success(),
            "POST {path} failed with {}",
            res.status()
        );
        res.json().await.unwrap_or(Value::Null)
    }

    pub async fn post_no_body(&self, token: &str, path: &str) -> reqwest::StatusCode {
        post_empty(&self.client, self.addr, token, path)
            .await
            .status()
    }

    pub async fn patch(&self, token: &str, path: &str, body: Value) -> Value {
        let res = patch(&self.client, self.addr, token, path, &body).await;
        assert!(
            res.status().is_success(),
            "PATCH {path} failed with {}",
            res.status()
        );
        res.json().await.unwrap()
    }

    pub async fn get(&self, token: &str, path: &str) -> Value {
        get_json(&self.client, self.addr, token, path).await
    }

    pub async fn get_status(&self, token: &str, path: &str) -> reqwest::StatusCode {
        get(&self.client, self.addr, token, path).await.status()
    }
}
