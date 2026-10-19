mod common;

use common::USER_PASSWORD;
use common::git::{commit_file, git};
use common::http::{
    add_collaborator, create_user, delete, get, get_json, login, mint_api_token, post, post_empty,
    post_json,
};
use serde_json::json;
use sqlx::PgPool;
use std::path::Path;
use std::process::Command;

/// Runs `git <args>` in `cwd` and returns whether it succeeded, with its stderr, so a refusal can be asserted.
async fn try_git(args: &[&str], cwd: &Path) -> (bool, String) {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let cwd = cwd.to_path_buf();
    let output = tokio::task::spawn_blocking(move || {
        Command::new("git").args(&args).current_dir(&cwd).output()
    })
    .await
    .unwrap()
    .unwrap();
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

/// The commit `branch` points to on the server, through a fresh `ls-remote`.
async fn remote_tip(cwd: &Path, branch: &str) -> String {
    git(
        &["ls-remote", "origin", &format!("refs/heads/{branch}")],
        cwd,
    )
    .await
    .split_whitespace()
    .next()
    .unwrap_or_default()
    .to_string()
}

#[sqlx::test]
async fn a_push_to_a_protected_branch_is_refused_over_git_http_with_the_reason(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let owner_token = mint_api_token(&client, addr, &owner_jwt, "owner").await;

    let repo: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    let repo_id = repo["id"].as_str().unwrap();
    let rules_path = format!("/repositories/{repo_id}/protected-branches");

    // Protected before anything is pushed: the owner, a Maintainer, may still create the branch.
    let res = post(
        &client,
        addr,
        &owner_jwt,
        &rules_path,
        &json!({ "pattern": " main " }),
    )
    .await;
    assert_eq!(res.status(), 201);
    let main_rule: serde_json::Value = res.json().await.unwrap();
    assert_eq!(main_rule["pattern"], "main");

    let parent = tempfile::tempdir().unwrap();
    let owner_url = format!("http://admin:{owner_token}@{addr}/admin/hello.git");
    git(&["clone", "-q", &owner_url, "owner"], parent.path()).await;
    let owner_dir = parent.path().join("owner");
    commit_file(&owner_dir, "README.md", "# hello\n", "init").await;
    git(&["push", "-q", "origin", "HEAD:main"], &owner_dir).await;
    let first_tip = remote_tip(&owner_dir, "main").await;
    assert!(!first_tip.is_empty(), "the first push creates main");

    // From now on main only moves through a merge request, even for its owner.
    commit_file(&owner_dir, "README.md", "# hello again\n", "direct").await;
    let (pushed, stderr) = try_git(&["push", "origin", "HEAD:main"], &owner_dir).await;
    assert!(!pushed, "a direct push to main must fail");
    assert!(
        stderr.contains("branch 'main' is protected"),
        "git prints the reason: {stderr}"
    );
    assert!(stderr.contains("merge request"), "{stderr}");
    assert_eq!(remote_tip(&owner_dir, "main").await, first_tip);

    // Rewritten history: a force push.
    git(&["reset", "-q", "--hard", "HEAD~1"], &owner_dir).await;
    git(
        &[
            "-c",
            "user.email=t@t.com",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "--amend",
            "-m",
            "init, rewritten",
        ],
        &owner_dir,
    )
    .await;
    commit_file(&owner_dir, "other.txt", "rewritten\n", "rewrite").await;
    let (pushed, stderr) = try_git(&["push", "--force", "origin", "HEAD:main"], &owner_dir).await;
    assert!(!pushed, "a force push to main must fail");
    assert!(stderr.contains("protected"), "{stderr}");
    let (pushed, stderr) = try_git(&["push", "origin", ":main"], &owner_dir).await;
    assert!(!pushed, "deleting main must fail");
    assert!(stderr.contains("cannot be deleted"), "{stderr}");
    assert_eq!(remote_tip(&owner_dir, "main").await, first_tip);

    // A push touching main and another branch is refused as a whole.
    let (pushed, stderr) = try_git(
        &["push", "origin", "+HEAD:main", "HEAD:refs/heads/side"],
        &owner_dir,
    )
    .await;
    assert!(!pushed);
    assert!(stderr.contains("side"), "{stderr}");
    assert!(remote_tip(&owner_dir, "side").await.is_empty());

    // A contributor works on a branch of their own and gets it merged.
    create_user(&client, addr, &owner_jwt, "carol").await;
    add_collaborator(&client, addr, &owner_jwt, repo_id, "carol", "contributor").await;
    let carol_jwt = login(&client, addr, "carol", USER_PASSWORD).await;
    let carol_token = mint_api_token(&client, addr, &carol_jwt, "carol").await;
    let carol_url = format!("http://carol:{carol_token}@{addr}/admin/hello.git");
    git(&["clone", "-q", &carol_url, "carol"], parent.path()).await;
    let carol_dir = parent.path().join("carol");
    commit_file(&carol_dir, "feature.txt", "feature\n", "feature").await;
    let (pushed, stderr) = try_git(&["push", "origin", "HEAD:main"], &carol_dir).await;
    assert!(!pushed);
    assert!(stderr.contains("protected"), "{stderr}");
    git(&["push", "-q", "origin", "HEAD:feature"], &carol_dir).await;

    let mr: serde_json::Value = post_json(
        &client,
        addr,
        &carol_jwt,
        &format!("/repositories/{repo_id}/merge-requests"),
        &json!({ "sourceBranch": "feature", "targetBranch": "main", "title": "Feature", "description": "" }),
    )
    .await;
    let mr_id = mr["id"].as_str().unwrap();
    let merged: serde_json::Value = post_empty(
        &client,
        addr,
        &owner_jwt,
        &format!("/merge-requests/{mr_id}/merge"),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(merged["outcome"], "merged");
    assert_eq!(
        remote_tip(&owner_dir, "main").await,
        merged["mergeCommitSha"].as_str().unwrap(),
        "a merge request still moves a protected branch"
    );

    // Creating a protected branch is for Maintainers only.
    let res = post(
        &client,
        addr,
        &owner_jwt,
        &rules_path,
        &json!({ "pattern": "release/*" }),
    )
    .await;
    assert_eq!(res.status(), 201);
    let (pushed, stderr) = try_git(&["push", "origin", "HEAD:release/1.0"], &carol_dir).await;
    assert!(!pushed);
    assert!(stderr.contains("only a Maintainer"), "{stderr}");
    git(&["push", "-q", "origin", "HEAD:release/1.0"], &owner_dir).await;

    // Once main is no longer protected, a direct push goes through again.
    let res = delete(
        &client,
        addr,
        &owner_jwt,
        &format!("{rules_path}/{}", main_rule["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(res.status(), 204);
    git(&["pull", "-q", "--no-rebase", "origin", "main"], &carol_dir).await;
    commit_file(&carol_dir, "after.txt", "after\n", "after").await;
    git(&["push", "-q", "origin", "HEAD:main"], &carol_dir).await;
}

#[sqlx::test]
async fn only_maintainers_manage_the_protected_branches_and_readers_see_them(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let repo: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    let repo_id = repo["id"].as_str().unwrap();
    let rules_path = format!("/repositories/{repo_id}/protected-branches");
    for (username, role) in [("rita", "reader"), ("carl", "contributor")] {
        create_user(&client, addr, &owner_jwt, username).await;
        add_collaborator(&client, addr, &owner_jwt, repo_id, username, role).await;
    }
    let reader_jwt = login(&client, addr, "rita", USER_PASSWORD).await;
    let contributor_jwt = login(&client, addr, "carl", USER_PASSWORD).await;

    let res = post(
        &client,
        addr,
        &contributor_jwt,
        &rules_path,
        &json!({ "pattern": "main" }),
    )
    .await;
    assert_eq!(res.status(), 404, "a contributor cannot protect a branch");

    for bad in ["", "refs/heads/main", "a b", "x..y"] {
        let res = post(
            &client,
            addr,
            &owner_jwt,
            &rules_path,
            &json!({ "pattern": bad }),
        )
        .await;
        assert_eq!(res.status(), 400, "{bad:?}");
    }
    let rule: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        &rules_path,
        &json!({ "pattern": "main" }),
    )
    .await;
    let res = post(
        &client,
        addr,
        &owner_jwt,
        &rules_path,
        &json!({ "pattern": "main" }),
    )
    .await;
    assert_eq!(res.status(), 409);

    let listed = get_json(&client, addr, &reader_jwt, &rules_path).await;
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["pattern"], "main");
    assert!(listed[0]["createdAt"].is_string());

    let rule_path = format!("{rules_path}/{}", rule["id"].as_str().unwrap());
    assert_eq!(
        delete(&client, addr, &contributor_jwt, &rule_path)
            .await
            .status(),
        404
    );
    assert_eq!(
        delete(&client, addr, &owner_jwt, &rule_path).await.status(),
        204
    );
    assert_eq!(
        delete(&client, addr, &owner_jwt, &rule_path).await.status(),
        404
    );
    let listed = get(&client, addr, &reader_jwt, &rules_path).await;
    assert_eq!(listed.json::<serde_json::Value>().await.unwrap(), json!([]));
}
