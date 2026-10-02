mod common;

use common::USER_PASSWORD;
use common::git::{commit_file, git};

use common::http::{login, post_json, post_ok};

use serde_json::json;
use sqlx::PgPool;

async fn push_main_and_feature(
    addr: std::net::SocketAddr,
    plain_token: &str,
    repo_name: &str,
) -> tempfile::TempDir {
    let clone_parent = tempfile::tempdir().unwrap();
    let clone_url = format!("http://admin:{plain_token}@{addr}/admin/{repo_name}.git");
    let repo_path = clone_parent.path().join("repo");
    git(&["clone", "-q", &clone_url, "repo"], clone_parent.path()).await;
    commit_file(&repo_path, "README.md", "line one\n", "root").await;
    git(&["push", "-q", "origin", "HEAD:main"], &repo_path).await;
    git(&["checkout", "-q", "-b", "feature"], &repo_path).await;
    commit_file(
        &repo_path,
        "README.md",
        "line one\nline two\n",
        "feature work",
    )
    .await;
    git(&["push", "-q", "origin", "HEAD:feature"], &repo_path).await;
    clone_parent
}

fn assert_created_at(actual: &serde_json::Value, expected: &serde_json::Value, what: &str) {
    let actual = actual
        .as_str()
        .unwrap_or_else(|| panic!("{what}: createdAt must be a string, got {actual}"));
    let expected = expected.as_str().unwrap();
    let parsed = chrono::DateTime::parse_from_rfc3339(actual)
        .unwrap_or_else(|e| panic!("{what}: createdAt {actual:?} is not RFC 3339: {e}"));
    assert_eq!(
        parsed,
        chrono::DateTime::parse_from_rfc3339(expected).unwrap(),
        "{what}: createdAt must equal the item's own creation time"
    );
}

#[sqlx::test]
async fn search_results_are_scoped_to_repository_access(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;

    let client = reqwest::Client::new();

    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;

    for (username, password) in [
        ("alice-collaborator", "password12345"),
        ("group-member", "password12345"),
        ("stranger-outsider", "password12345"),
    ] {
        post_ok(&client, addr, &owner_jwt, "/admin/users", &json!({ "username": username, "email": format!("{username}@example.com"), "password": password })).await;
    }

    let repo_res: serde_json::Value = post_json(&client, addr, &owner_jwt, "/repositories", &json!({ "name": "hush-hush", "description": "top secret widget project", "visibility": "private" })).await;
    let repo_id = repo_res["id"].as_str().unwrap();

    post_ok(&client, addr, &owner_jwt, &format!("/repositories/{repo_id}/issues"), &json!({ "title": "gizmo-flavored bug in the widget project", "description": "", "kind": "bug" })).await;

    post_ok(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/collaborators"),
        &json!({ "username": "alice-collaborator", "role": "reader" }),
    )
    .await;

    let group_res: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/groups",
        &json!({ "name": "widgets-group", "description": "" }),
    )
    .await;
    let group_id = group_res["id"].as_str().unwrap();

    post_ok(
        &client,
        addr,
        &owner_jwt,
        &format!("/groups/{group_id}/members"),
        &json!({ "username": "group-member", "role": "reader" }),
    )
    .await;

    // No route moves an existing repo, so create a second group-owned repository with the same searchable title.
    let group_repo_res: serde_json::Value = post_json(&client, addr, &owner_jwt, "/repositories", &json!({ "name": "hush-hush-group", "description": "top secret widget project", "visibility": "private", "groupPath": "widgets-group" })).await;
    let group_repo_id = group_repo_res["id"].as_str().unwrap();
    post_ok(&client, addr, &owner_jwt, &format!("/repositories/{group_repo_id}/issues"), &json!({ "title": "gizmo-flavored bug in the group project", "description": "", "kind": "bug" })).await;

    let public_repo_res: serde_json::Value = post_json(&client, addr, &owner_jwt, "/repositories", &json!({ "name": "open-widget", "description": "top secret widget project", "visibility": "public" })).await;
    let public_repo_id = public_repo_res["id"].as_str().unwrap();
    let public_issue: serde_json::Value = post_ok(&client, addr, &owner_jwt, &format!("/repositories/{public_repo_id}/issues"), &json!({ "title": "gizmo-flavored bug in the open project", "description": "", "kind": "bug" })).await
        .json()
        .await
        .unwrap();

    let token: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/tokens",
        &json!({ "name": "ci" }),
    )
    .await;
    let _clone_dir =
        push_main_and_feature(addr, token["token"].as_str().unwrap(), "open-widget").await;
    let public_mr: serde_json::Value = post_ok(&client, addr, &owner_jwt, &format!("/repositories/{public_repo_id}/merge-requests"), &json!({ "sourceBranch": "feature", "targetBranch": "main", "title": "gizmo-flavored line two", "description": "" })).await
        .json()
        .await
        .unwrap();

    async fn search(
        client: &reqwest::Client,
        addr: std::net::SocketAddr,
        jwt: &str,
        q: &str,
    ) -> serde_json::Value {
        client
            .get(format!("http://{addr}/api/search"))
            .bearer_auth(jwt)
            .query(&[("q", q)])
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    let alice_jwt = login(&client, addr, "alice-collaborator", USER_PASSWORD).await;
    let alice_results = search(&client, addr, &alice_jwt, "gizmo-flavored").await;
    assert_eq!(
        alice_results["issues"].as_array().unwrap().len(),
        2,
        "collaborator should see their own repo's issue plus the public repo's issue"
    );

    let group_member_jwt = login(&client, addr, "group-member", USER_PASSWORD).await;
    let group_member_results = search(&client, addr, &group_member_jwt, "gizmo-flavored").await;
    let group_member_titles: Vec<&str> = group_member_results["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].as_str().unwrap())
        .collect();
    assert_eq!(
        group_member_titles.len(),
        2,
        "group member should see the group's issue plus the public repo's issue"
    );
    assert!(group_member_titles.contains(&"gizmo-flavored bug in the group project"));
    assert!(group_member_titles.contains(&"gizmo-flavored bug in the open project"));

    let stranger_jwt = login(&client, addr, "stranger-outsider", USER_PASSWORD).await;
    let stranger_results = search(&client, addr, &stranger_jwt, "gizmo-flavored").await;
    assert_eq!(
        stranger_results["issues"].as_array().unwrap().len(),
        1,
        "a stranger should see only the public repo's issue, not either private repo's issue"
    );
    assert_eq!(
        stranger_results["issues"][0]["title"], "gizmo-flavored bug in the open project",
        "the ONE visible issue must be the public repo's, not either private one leaking through"
    );

    let stranger_issue = &stranger_results["issues"][0];
    assert_eq!(stranger_issue["id"], public_issue["id"]);
    assert_eq!(stranger_issue["number"], 1);
    assert_eq!(stranger_issue["kind"], "bug");
    assert_eq!(stranger_issue["status"], public_issue["status"]);
    assert_eq!(
        stranger_issue["repository"]["path"],
        json!(["admin", "open-widget"])
    );
    assert_created_at(
        &stranger_issue["createdAt"],
        &public_issue["createdAt"],
        "issue result",
    );

    let stranger_mrs = stranger_results["mergeRequests"].as_array().unwrap();
    assert_eq!(stranger_mrs.len(), 1);
    assert_eq!(stranger_mrs[0]["id"], public_mr["id"]);
    assert_eq!(stranger_mrs[0]["title"], "gizmo-flavored line two");
    assert_eq!(stranger_mrs[0]["sourceBranch"], "feature");
    assert_eq!(stranger_mrs[0]["targetBranch"], "main");
    assert_eq!(stranger_mrs[0]["status"], public_mr["status"]);
    assert_eq!(
        stranger_mrs[0]["repository"]["path"],
        json!(["admin", "open-widget"])
    );
    assert_created_at(
        &stranger_mrs[0]["createdAt"],
        &public_mr["createdAt"],
        "merge request result",
    );

    let stranger_user_search = search(&client, addr, &stranger_jwt, "alice-collaborator").await;
    assert_eq!(stranger_user_search["users"].as_array().unwrap().len(), 1);
    assert_eq!(
        stranger_user_search["users"][0]["username"],
        "alice-collaborator"
    );
}
