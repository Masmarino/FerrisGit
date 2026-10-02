mod common;

use common::git::{commit_file, git};

use common::http::{get_json, login, post_json, post_ok};

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
async fn dashboard_lists_an_issue_assigned_to_the_caller(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;

    let client = reqwest::Client::new();

    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;

    let contributor: serde_json::Value = post_json(&client, addr, &owner_jwt, "/admin/users", &json!({ "username": "contributor", "email": "contributor@example.com", "password": "password12345" })).await;
    let contributor_id = contributor["id"].as_str().unwrap();

    let repo: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    let repo_id = repo["id"].as_str().unwrap();

    post_ok(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/collaborators"),
        &json!({ "username": "contributor", "role": "contributor" }),
    )
    .await;

    let created_issue: serde_json::Value = post_ok(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/issues"),
        &json!({ "title": "Fix the thing", "description": "", "kind": "bug" }),
    )
    .await
    .json()
    .await
    .unwrap();

    post_ok(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/issues/1/assign"),
        &json!({ "assigneeId": contributor_id }),
    )
    .await;

    let contributor_jwt = login(&client, addr, "contributor", "password12345").await;

    let dashboard: serde_json::Value =
        get_json(&client, addr, &contributor_jwt, "/dashboard").await;

    let assigned = dashboard["assignedIssues"].as_array().unwrap();
    assert_eq!(assigned.len(), 1);
    assert_eq!(assigned[0]["title"], "Fix the thing");
    assert_eq!(assigned[0]["number"], 1);
    assert_eq!(assigned[0]["repository"]["path"], json!(["admin", "hello"]));
    assert_eq!(assigned[0]["status"], created_issue["status"]);
    assert_eq!(assigned[0]["kind"], "bug");
    assert_eq!(assigned[0]["id"], created_issue["id"]);
    assert_created_at(
        &assigned[0]["createdAt"],
        &created_issue["createdAt"],
        "assigned issue",
    );

    let owner_dashboard: serde_json::Value =
        get_json(&client, addr, &owner_jwt, "/dashboard").await;
    let authored = owner_dashboard["authoredIssues"].as_array().unwrap();
    assert_eq!(authored.len(), 1);
    assert_eq!(authored[0]["title"], "Fix the thing");
    assert_created_at(
        &authored[0]["createdAt"],
        &created_issue["createdAt"],
        "authored issue",
    );

    let token: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/tokens",
        &json!({ "name": "ci" }),
    )
    .await;
    let _clone_dir = push_main_and_feature(addr, token["token"].as_str().unwrap(), "hello").await;
    let created_mr: serde_json::Value = post_ok(&client, addr, &owner_jwt, &format!("/repositories/{repo_id}/merge-requests"), &json!({ "sourceBranch": "feature", "targetBranch": "main", "title": "Add line two", "description": "" })).await
        .json()
        .await
        .unwrap();

    let owner_dashboard: serde_json::Value =
        get_json(&client, addr, &owner_jwt, "/dashboard").await;
    let authored_mrs = owner_dashboard["authoredMergeRequests"].as_array().unwrap();
    assert_eq!(authored_mrs.len(), 1);
    assert_eq!(authored_mrs[0]["title"], "Add line two");
    assert_eq!(authored_mrs[0]["sourceBranch"], "feature");
    assert_eq!(authored_mrs[0]["targetBranch"], "main");
    assert_eq!(authored_mrs[0]["status"], created_mr["status"]);
    assert_created_at(
        &authored_mrs[0]["createdAt"],
        &created_mr["createdAt"],
        "authored merge request",
    );

    let contributor_dashboard: serde_json::Value =
        get_json(&client, addr, &contributor_jwt, "/dashboard").await;
    let to_review = contributor_dashboard["mergeRequestsToReview"]
        .as_array()
        .unwrap();
    assert_eq!(to_review.len(), 1);
    assert_eq!(to_review[0]["title"], "Add line two");
    assert_eq!(
        to_review[0]["repository"]["path"],
        json!(["admin", "hello"])
    );
    assert_created_at(
        &to_review[0]["createdAt"],
        &created_mr["createdAt"],
        "merge request to review",
    );
}
