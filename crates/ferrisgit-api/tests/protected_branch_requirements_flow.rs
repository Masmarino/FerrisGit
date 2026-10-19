mod common;

use common::USER_PASSWORD;
use common::git::{commit_file, git};
use common::http::{
    add_collaborator, create_user, get_json, login, mint_api_token, post, post_json, post_ok, put,
};
use serde_json::{Value, json};
use sqlx::PgPool;

const PIPELINE_FILE: &str =
    "stages: [test]\njobs:\n  unit:\n    stage: test\n    image: alpine\n    script: [\"true\"]\n";

#[sqlx::test]
async fn a_protected_branch_takes_a_merge_only_once_its_approvals_and_checks_are_there(
    pool: PgPool,
) {
    let app = common::spawn_app(pool).await;
    let (addr, pool) = (app.addr, app.pool.clone());
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
    commit_file(&work, ".ferrisgit-ci.yml", PIPELINE_FILE, "ci").await;
    git(&["push", "-q", "origin", "HEAD:main"], &work).await;

    let rules_path = format!("/repositories/{repo_id}/protected-branches");
    let res = post(
        &client,
        addr,
        &jwt,
        &rules_path,
        &json!({ "pattern": "main", "requiredApprovals": 1, "requiredChecks": ["pipeline/unit", " pipeline/unit "] }),
    )
    .await;
    assert_eq!(res.status(), 201);
    let rule: Value = res.json().await.unwrap();
    assert_eq!(rule["requiredApprovals"], 1);
    assert_eq!(rule["requiredChecks"], json!(["pipeline/unit"]));

    git(&["checkout", "-q", "-b", "feature"], &work).await;
    commit_file(&work, "feature.txt", "feature\n", "feature").await;
    git(&["push", "-q", "origin", "feature"], &work).await;
    let mr: Value = post_json(
        &client,
        addr,
        &jwt,
        &format!("/repositories/{repo_id}/merge-requests"),
        &json!({ "sourceBranch": "feature", "targetBranch": "main", "title": "Feature", "description": "" }),
    )
    .await;
    let mr_id = mr["id"].as_str().unwrap().to_string();

    // Before anything: no approval, and the job has not finished.
    let mergeability = get_json(
        &client,
        addr,
        &jwt,
        &format!("/merge-requests/{mr_id}/mergeability"),
    )
    .await;
    assert_eq!(
        mergeability,
        json!({
            "mergeable": false,
            "conditions": [
                { "kind": "noChangesRequested", "met": true, "requestedBy": [], "message": null },
                { "kind": "approvals", "met": false, "required": 1, "given": 0, "message": "0/1 approvals" },
                { "kind": "check", "met": false, "name": "pipeline/unit", "status": "pending", "message": "check 'pipeline/unit' has not finished" },
            ]
        })
    );
    let reviews = get_json(
        &client,
        addr,
        &jwt,
        &format!("/merge-requests/{mr_id}/reviews"),
    )
    .await;
    assert_eq!(
        reviews["requiredApprovals"], 1,
        "the rule raises the approvals"
    );
    assert_eq!(reviews["blocked"], true);

    let res = post(
        &client,
        addr,
        &jwt,
        &format!("/merge-requests/{mr_id}/merge"),
        &json!({}),
    )
    .await;
    assert_eq!(res.status(), 400);
    let error: Value = res.json().await.unwrap();
    assert_eq!(
        error["error"],
        "0/1 approvals; check 'pipeline/unit' has not finished"
    );

    // An approval from someone other than the author, then the job succeeds: the merge goes through.
    create_user(&client, addr, &jwt, "reviewer").await;
    add_collaborator(&client, addr, &jwt, &repo_id, "reviewer", "contributor").await;
    let reviewer = login(&client, addr, "reviewer", USER_PASSWORD).await;
    post_ok(
        &client,
        addr,
        &reviewer,
        &format!("/merge-requests/{mr_id}/reviews"),
        &json!({ "decision": "approved" }),
    )
    .await;
    let pipeline_id: uuid::Uuid = mr["pipeline"]["id"].as_str().unwrap().parse().unwrap();
    sqlx::query("UPDATE jobs SET status = 'success', finished_at = now() WHERE pipeline_id = $1")
        .bind(pipeline_id)
        .execute(&pool)
        .await
        .unwrap();
    let mergeability = get_json(
        &client,
        addr,
        &jwt,
        &format!("/merge-requests/{mr_id}/mergeability"),
    )
    .await;
    assert_eq!(mergeability["mergeable"], true, "{mergeability:#}");
    let merged: Value = post_json(
        &client,
        addr,
        &jwt,
        &format!("/merge-requests/{mr_id}/merge"),
        &json!({}),
    )
    .await;
    assert_eq!(merged["outcome"], "merged", "{merged:#}");

    // A merged merge request has nothing left to meet.
    let after = get_json(
        &client,
        addr,
        &jwt,
        &format!("/merge-requests/{mr_id}/mergeability"),
    )
    .await;
    assert_eq!(after, json!({ "mergeable": true, "conditions": [] }));

    // The requirements of a rule can be changed, within bounds.
    let rule_path = format!("{rules_path}/{}", rule["id"].as_str().unwrap());
    let res = put(
        &client,
        addr,
        &jwt,
        &rule_path,
        &json!({ "requiredChecks": ["quality-gate"] }),
    )
    .await;
    assert_eq!(res.status(), 200);
    let updated: Value = res.json().await.unwrap();
    assert_eq!(updated["requiredApprovals"], 1);
    assert_eq!(updated["requiredChecks"], json!(["quality-gate"]));
    let res = put(
        &client,
        addr,
        &jwt,
        &rule_path,
        &json!({ "requiredApprovals": -1 }),
    )
    .await;
    assert_eq!(res.status(), 400);
    let listed = get_json(&client, addr, &jwt, &rules_path).await;
    assert_eq!(listed[0]["requiredChecks"], json!(["quality-gate"]));
}
