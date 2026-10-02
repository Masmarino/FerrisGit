// Who may write on issues, labels and milestones, who can be assigned, and who manages collaborators, with roles
// granted directly and through groups. The rule from the README: Reader reads, Contributor contributes.

mod common;

use common::{ADMIN_PASSWORD, Options, Server, USER_PASSWORD, spawn_with};
use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn spawn(pool: PgPool) -> Server {
    // Not about MFA, so log in with a plain session.
    spawn_with(pool, Options::default(), |state| {
        state.mfa_enforced = false;
    })
    .await
}

async fn session(server: &Server, username: &str, password: &str) -> String {
    let body: Value = server
        .post(
            "/auth/login",
            json!({ "username": username, "password": password }),
        )
        .await
        .json()
        .await
        .unwrap();
    body["token"].as_str().expect("a session token").to_string()
}

/// Creates the account and returns `(id, session)`.
async fn user(server: &Server, username: &str) -> (String, String) {
    let id = server.new_user(username).await;
    let jwt = session(server, username, USER_PASSWORD).await;
    (id, jwt)
}

async fn call(server: &Server, method: Method, jwt: &str, path: &str, body: Value) -> Response {
    let request = server
        .client
        .request(method, server.url(path))
        .bearer_auth(jwt);
    if body.is_null() {
        request.send().await.unwrap()
    } else {
        request.json(&body).send().await.unwrap()
    }
}

async fn ok_json(server: &Server, method: Method, jwt: &str, path: &str, body: Value) -> Value {
    let response = call(server, method.clone(), jwt, path, body).await;
    let status = response.status();
    assert_eq!(status, StatusCode::OK, "{method} {path}");
    response.json().await.unwrap()
}

async fn create_repository(server: &Server, jwt: &str, body: Value) -> String {
    ok_json(server, Method::POST, jwt, "/repositories", body).await["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn add_collaborator(server: &Server, jwt: &str, repo: &str, username: &str, role: &str) {
    let response = call(
        server,
        Method::POST,
        jwt,
        &format!("/repositories/{repo}/collaborators"),
        json!({ "username": username, "role": role }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

async fn add_group_member(server: &Server, jwt: &str, group: &str, username: &str, role: &str) {
    let response = call(
        server,
        Method::POST,
        jwt,
        &format!("/groups/{group}/members"),
        json!({ "username": username, "role": role }),
    )
    .await;
    assert!(response.status().is_success());
}

#[sqlx::test]
async fn every_write_on_issues_labels_and_milestones_needs_the_contributor_role(pool: PgPool) {
    let server = spawn(pool).await;
    let admin = session(&server, "admin", ADMIN_PASSWORD).await;
    let (_, reader_jwt) = user(&server, "reader").await;
    let (contributor_id, contributor_jwt) = user(&server, "contributor").await;
    // Signed in with no role on the repository still means Reader on a public one.
    let (_, stranger_jwt) = user(&server, "stranger").await;
    let repo = create_repository(
        &server,
        &admin,
        json!({ "name": "hello", "visibility": "public" }),
    )
    .await;
    add_collaborator(&server, &admin, &repo, "reader", "reader").await;
    add_collaborator(&server, &admin, &repo, "contributor", "contributor").await;

    let issue = ok_json(
        &server,
        Method::POST,
        &admin,
        &format!("/repositories/{repo}/issues"),
        json!({ "title": "Existing", "description": "d", "kind": "bug" }),
    )
    .await;
    assert_eq!(issue["number"], 1);
    let label = ok_json(
        &server,
        Method::POST,
        &admin,
        &format!("/repositories/{repo}/labels"),
        json!({ "name": "bug", "color": "#dc2626" }),
    )
    .await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let milestone = ok_json(
        &server,
        Method::POST,
        &admin,
        &format!("/repositories/{repo}/milestones"),
        json!({ "title": "v1", "description": "", "dueDate": null }),
    )
    .await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let writes: Vec<(Method, String, Value)> = vec![
        (
            Method::POST,
            format!("/repositories/{repo}/issues"),
            json!({ "title": "Sneaky", "description": "d", "kind": "bug" }),
        ),
        (
            Method::POST,
            format!("/repositories/{repo}/issues/1/comments"),
            json!({ "body": "Sneaky" }),
        ),
        (
            Method::PATCH,
            format!("/repositories/{repo}/issues/1"),
            json!({ "title": "Renamed", "description": "d", "kind": "bug", "milestoneId": null }),
        ),
        (
            Method::PATCH,
            format!("/repositories/{repo}/issues/1/status"),
            json!({ "status": "in_progress" }),
        ),
        (
            Method::POST,
            format!("/repositories/{repo}/issues/1/assign"),
            json!({ "assigneeId": contributor_id }),
        ),
        (
            Method::PUT,
            format!("/repositories/{repo}/issues/1/labels"),
            json!({ "labelIds": [label] }),
        ),
        (
            Method::POST,
            format!("/repositories/{repo}/issues/1/close"),
            Value::Null,
        ),
        (
            Method::POST,
            format!("/repositories/{repo}/issues/1/reopen"),
            Value::Null,
        ),
        (
            Method::POST,
            format!("/repositories/{repo}/labels"),
            json!({ "name": "sneaky", "color": "#000000" }),
        ),
        (
            Method::PATCH,
            format!("/labels/{label}"),
            json!({ "name": "renamed", "color": "#000000" }),
        ),
        (Method::DELETE, format!("/labels/{label}"), Value::Null),
        (
            Method::POST,
            format!("/repositories/{repo}/milestones"),
            json!({ "title": "Sneaky", "description": "", "dueDate": null }),
        ),
        (
            Method::PATCH,
            format!("/milestones/{milestone}"),
            json!({ "title": "Renamed", "description": "", "dueDate": null, "state": "closed" }),
        ),
        (
            Method::DELETE,
            format!("/milestones/{milestone}"),
            Value::Null,
        ),
    ];

    // Too low a role looks like an unknown repository: a 404 whatever the visibility.
    for (who, jwt) in [("reader", &reader_jwt), ("stranger", &stranger_jwt)] {
        for (method, path, body) in &writes {
            let response = call(&server, method.clone(), jwt, path, body.clone()).await;
            assert_eq!(
                response.status(),
                StatusCode::NOT_FOUND,
                "{who}: {method} {path}"
            );
        }
    }

    // Reading stays open to both.
    for jwt in [&reader_jwt, &stranger_jwt] {
        for path in [
            format!("/repositories/{repo}/issues"),
            format!("/repositories/{repo}/issues/1"),
            format!("/repositories/{repo}/issues/1/comments"),
            format!("/repositories/{repo}/labels"),
            format!("/repositories/{repo}/milestones"),
        ] {
            let response = call(&server, Method::GET, jwt, &path, Value::Null).await;
            assert_eq!(response.status(), StatusCode::OK, "GET {path}");
        }
    }

    // None of the refused writes left a trace.
    let issues = ok_json(
        &server,
        Method::GET,
        &admin,
        &format!("/repositories/{repo}/issues"),
        Value::Null,
    )
    .await;
    assert_eq!(issues.as_array().unwrap().len(), 1);
    assert_eq!(issues[0]["title"], "Existing");
    assert_eq!(issues[0]["status"], "todo");
    assert_eq!(issues[0]["commentCount"], 0);
    assert_eq!(issues[0]["assigneeId"], Value::Null);

    // A Contributor does all of it, from the same requests.
    for (method, path, body) in &writes {
        let response = call(
            &server,
            method.clone(),
            &contributor_jwt,
            path,
            body.clone(),
        )
        .await;
        assert!(
            response.status().is_success(),
            "contributor: {method} {path} answered {}",
            response.status()
        );
    }
}

#[sqlx::test]
async fn a_contributor_can_open_and_comment_on_an_issue_of_a_private_repository(pool: PgPool) {
    let server = spawn(pool).await;
    let admin = session(&server, "admin", ADMIN_PASSWORD).await;
    let (contributor_id, contributor_jwt) = user(&server, "contributor").await;
    let repo = create_repository(
        &server,
        &admin,
        json!({ "name": "secret", "visibility": "private" }),
    )
    .await;
    add_collaborator(&server, &admin, &repo, "contributor", "contributor").await;

    let issue = ok_json(
        &server,
        Method::POST,
        &contributor_jwt,
        &format!("/repositories/{repo}/issues"),
        json!({ "title": "From a contributor", "description": "d", "kind": "task" }),
    )
    .await;
    assert_eq!(issue["authorId"], contributor_id.as_str());

    let comment = ok_json(
        &server,
        Method::POST,
        &contributor_jwt,
        &format!("/repositories/{repo}/issues/1/comments"),
        json!({ "body": "Noted" }),
    )
    .await;
    assert_eq!(comment["body"], "Noted");
}

/// A team group where `lead` is Maintainer, `dev` Contributor and `viewer` Reader; `outsider` is in no group. The
/// repository is in a subgroup, so every role is inherited.
struct Team {
    repo: String,
    lead_jwt: String,
    dev_id: String,
    dev_jwt: String,
    viewer_id: String,
    outsider_id: String,
}

async fn team_with_a_repository(server: &Server) -> Team {
    let (_, lead_jwt) = user(server, "lead").await;
    let (dev_id, dev_jwt) = user(server, "dev").await;
    let (viewer_id, _) = user(server, "viewer").await;
    let (outsider_id, _) = user(server, "outsider").await;

    let team = ok_json(
        server,
        Method::POST,
        &lead_jwt,
        "/groups",
        json!({ "name": "team", "description": "" }),
    )
    .await["id"]
        .as_str()
        .unwrap()
        .to_string();
    ok_json(
        server,
        Method::POST,
        &lead_jwt,
        &format!("/groups/{team}/subgroups"),
        json!({ "name": "app", "description": "" }),
    )
    .await;
    add_group_member(server, &lead_jwt, &team, "dev", "contributor").await;
    add_group_member(server, &lead_jwt, &team, "viewer", "reader").await;

    let repo = create_repository(
        server,
        &lead_jwt,
        json!({ "name": "service", "visibility": "private", "groupPath": "team/app" }),
    )
    .await;
    Team {
        repo,
        lead_jwt,
        dev_id,
        dev_jwt,
        viewer_id,
        outsider_id,
    }
}

#[sqlx::test]
async fn an_issue_can_be_assigned_to_a_contributor_whose_role_comes_from_a_group(pool: PgPool) {
    let server = spawn(pool).await;
    let team = team_with_a_repository(&server).await;
    let repo = &team.repo;
    let issue = ok_json(
        &server,
        Method::POST,
        &team.dev_jwt,
        &format!("/repositories/{repo}/issues"),
        json!({ "title": "Flaky test", "description": "d", "kind": "bug" }),
    )
    .await;
    assert_eq!(issue["number"], 1);
    let assign = format!("/repositories/{repo}/issues/1/assign");

    // "Assign me" as a group Contributor with no direct grant on the repository.
    let assigned = ok_json(
        &server,
        Method::POST,
        &team.dev_jwt,
        &assign,
        json!({ "assigneeId": team.dev_id }),
    )
    .await;
    assert_eq!(assigned["assigneeId"], team.dev_id.as_str());

    // A group Maintainer can hand it to a group Contributor too.
    let reassigned = ok_json(
        &server,
        Method::POST,
        &team.lead_jwt,
        &assign,
        json!({ "assigneeId": team.dev_id }),
    )
    .await;
    assert_eq!(reassigned["assigneeId"], team.dev_id.as_str());

    // But not to someone who can't contribute: a group Reader, or a user with no role.
    for refused in [&team.viewer_id, &team.outsider_id] {
        let response = call(
            &server,
            Method::POST,
            &team.lead_jwt,
            &assign,
            json!({ "assigneeId": refused }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{refused}");
    }
    let still: Value = ok_json(
        &server,
        Method::GET,
        &team.lead_jwt,
        &format!("/repositories/{repo}/issues/1"),
        Value::Null,
    )
    .await;
    assert_eq!(still["assigneeId"], team.dev_id.as_str());
}

#[sqlx::test]
async fn a_direct_reader_collaborator_cannot_be_assigned_an_issue(pool: PgPool) {
    let server = spawn(pool).await;
    let admin = session(&server, "admin", ADMIN_PASSWORD).await;
    let (reader_id, _) = user(&server, "reader").await;
    let repo = create_repository(
        &server,
        &admin,
        json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    add_collaborator(&server, &admin, &repo, "reader", "reader").await;
    ok_json(
        &server,
        Method::POST,
        &admin,
        &format!("/repositories/{repo}/issues"),
        json!({ "title": "Bug", "description": "d", "kind": "bug" }),
    )
    .await;

    let response = call(
        &server,
        Method::POST,
        &admin,
        &format!("/repositories/{repo}/issues/1/assign"),
        json!({ "assigneeId": reader_id }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn a_group_maintainer_manages_the_collaborators_of_a_group_repository(pool: PgPool) {
    let server = spawn(pool).await;
    let team = team_with_a_repository(&server).await;
    let repo = &team.repo;
    let (_, _) = user(&server, "guest").await;
    let collaborators = format!("/repositories/{repo}/collaborators");

    // lead is Maintainer of the parent group with no direct grant on the repository.
    add_collaborator(&server, &team.lead_jwt, repo, "guest", "reader").await;
    let listed = ok_json(
        &server,
        Method::GET,
        &team.lead_jwt,
        &collaborators,
        Value::Null,
    )
    .await;
    assert_eq!(listed[0]["username"], "guest");
    assert_eq!(listed[0]["role"], "reader");

    let promoted = call(
        &server,
        Method::PATCH,
        &team.lead_jwt,
        &format!("{collaborators}/guest"),
        json!({ "role": "contributor" }),
    )
    .await;
    assert_eq!(promoted.status(), StatusCode::NO_CONTENT);
    let listed = ok_json(
        &server,
        Method::GET,
        &team.lead_jwt,
        &collaborators,
        Value::Null,
    )
    .await;
    assert_eq!(listed[0]["role"], "contributor");

    let removed = call(
        &server,
        Method::DELETE,
        &team.lead_jwt,
        &format!("{collaborators}/guest"),
        Value::Null,
    )
    .await;
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
    let listed = ok_json(
        &server,
        Method::GET,
        &team.lead_jwt,
        &collaborators,
        Value::Null,
    )
    .await;
    assert!(listed.as_array().unwrap().is_empty());
}

#[sqlx::test]
async fn a_group_contributor_cannot_manage_the_collaborators_of_a_group_repository(pool: PgPool) {
    let server = spawn(pool).await;
    let team = team_with_a_repository(&server).await;
    let repo = &team.repo;
    let (_, _) = user(&server, "guest").await;

    let response = call(
        &server,
        Method::POST,
        &team.dev_jwt,
        &format!("/repositories/{repo}/collaborators"),
        json!({ "username": "guest", "role": "reader" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
