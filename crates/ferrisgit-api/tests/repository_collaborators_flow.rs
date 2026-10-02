mod common;

use common::http::{
    create_user, delete, get, get_json, login, post, post_empty, post_json, post_ok, put,
};

use serde_json::json;
use sqlx::PgPool;
use std::process::Command;

#[sqlx::test]
async fn a_collaborator_can_use_a_repo_they_do_not_own_and_a_stranger_cannot(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;

    let client = reqwest::Client::new();

    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;

    let repo_res: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    assert_eq!(repo_res["owner"], "admin");
    assert_eq!(repo_res["role"], "owner");
    let repo_id = repo_res["id"].as_str().unwrap().to_string();

    for username in ["alice", "stranger"] {
        create_user(&client, addr, &owner_jwt, username).await;
    }

    let alice_jwt = login(&client, addr, "alice", "password12345").await;

    let stranger_jwt = login(&client, addr, "stranger", "password12345").await;

    let before_status = get(&client, addr, &alice_jwt, "/repositories/admin/hello")
        .await
        .status();
    assert_eq!(before_status, 404);

    post_ok(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/collaborators"),
        &json!({ "username": "alice", "role": "maintainer" }),
    )
    .await;

    let collaborators_list: serde_json::Value = get_json(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/collaborators"),
    )
    .await;
    let usernames: Vec<&str> = collaborators_list
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["username"].as_str().unwrap())
        .collect();
    assert_eq!(usernames, vec!["alice"]);

    let alice_repos: serde_json::Value = get_json(&client, addr, &alice_jwt, "/repositories").await;
    let alice_repos = alice_repos.as_array().unwrap();
    assert_eq!(alice_repos.len(), 1);
    assert_eq!(alice_repos[0]["name"], "hello");
    assert_eq!(alice_repos[0]["owner"], "admin");
    assert_eq!(alice_repos[0]["role"], "maintainer");

    put(
        &client,
        addr,
        &alice_jwt,
        &format!("/repositories/{repo_id}/settings"),
        &json!({ "ciEnabled": true }),
    )
    .await
    .error_for_status()
    .unwrap();
    post_ok(
        &client,
        addr,
        &alice_jwt,
        &format!("/repositories/{repo_id}/ci-variables"),
        &json!({ "key": "SECRET", "value": "s3cr3t", "masked": true }),
    )
    .await;

    let alice_token_res: serde_json::Value = post_json(
        &client,
        addr,
        &alice_jwt,
        "/tokens",
        &json!({ "name": "alice-ci" }),
    )
    .await;
    let alice_plain_token = alice_token_res["token"].as_str().unwrap();

    let clone_parent = tempfile::tempdir().unwrap();
    let clone_url = format!("http://alice:{alice_plain_token}@{addr}/admin/hello.git");
    let repo_path = clone_parent.path().join("repo");
    let run_git = |args: Vec<String>, cwd: std::path::PathBuf| {
        tokio::task::spawn_blocking(move || {
            Command::new("git").args(&args).current_dir(&cwd).status()
        })
    };
    assert!(
        run_git(
            vec!["clone".to_string(), clone_url, "repo".to_string()],
            clone_parent.path().to_path_buf()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    std::fs::write(repo_path.join("README.md"), "hello from alice\n").unwrap();
    assert!(
        run_git(vec!["add".to_string(), ".".to_string()], repo_path.clone())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    assert!(
        run_git(
            vec![
                "-c".to_string(),
                "user.email=a@a.com".to_string(),
                "-c".to_string(),
                "user.name=alice".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "alice's commit".to_string()
            ],
            repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    assert!(
        run_git(
            vec![
                "push".to_string(),
                "-q".to_string(),
                "origin".to_string(),
                "HEAD:main".to_string()
            ],
            repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    assert!(
        run_git(
            vec![
                "checkout".to_string(),
                "-q".to_string(),
                "-b".to_string(),
                "feature".to_string()
            ],
            repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    std::fs::write(
        repo_path.join("README.md"),
        "hello from alice\nfeature line\n",
    )
    .unwrap();
    assert!(
        run_git(vec!["add".to_string(), ".".to_string()], repo_path.clone())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    assert!(
        run_git(
            vec![
                "-c".to_string(),
                "user.email=a@a.com".to_string(),
                "-c".to_string(),
                "user.name=alice".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "feature work".to_string()
            ],
            repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    assert!(
        run_git(
            vec![
                "push".to_string(),
                "-q".to_string(),
                "origin".to_string(),
                "HEAD:feature".to_string()
            ],
            repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    let mr_res: serde_json::Value = post_json(&client, addr, &alice_jwt, &format!("/repositories/{repo_id}/merge-requests"), &json!({ "sourceBranch": "feature", "targetBranch": "main", "title": "Add feature", "description": "" })).await;
    let mr_id = mr_res["id"].as_str().unwrap();
    let merge_res = post_empty(
        &client,
        addr,
        &alice_jwt,
        &format!("/merge-requests/{mr_id}/merge"),
    )
    .await;
    assert_eq!(merge_res.status(), 200);

    assert_eq!(
        get(&client, addr, &stranger_jwt, "/repositories/admin/hello")
            .await
            .status(),
        404
    );
    assert_eq!(
        post(&client, addr, &stranger_jwt, &format!("/repositories/{repo_id}/merge-requests"), &json!({ "sourceBranch": "feature", "targetBranch": "main", "title": "x", "description": "" })).await
            .status(),
        404
    );

    post_ok(
        &client,
        addr,
        &alice_jwt,
        &format!("/repositories/{repo_id}/collaborators"),
        &json!({ "username": "stranger", "role": "reader" }),
    )
    .await;

    delete(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/collaborators/alice"),
    )
    .await
    .error_for_status()
    .unwrap();
    assert_eq!(
        get(&client, addr, &alice_jwt, "/repositories/admin/hello")
            .await
            .status(),
        404
    );
}

/// The public Reader bypass must not expose who the collaborators are (a product decision).
#[sqlx::test]
async fn a_public_repos_collaborator_list_is_not_exposed_to_a_non_collaborator(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;

    let client = reqwest::Client::new();

    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;

    let repo_res: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "public" }),
    )
    .await;
    let repo_id = repo_res["id"].as_str().unwrap().to_string();

    create_user(&client, addr, &owner_jwt, "stranger").await;
    let stranger_jwt = login(&client, addr, "stranger", "password12345").await;

    assert_eq!(
        get(&client, addr, &stranger_jwt, "/repositories/admin/hello")
            .await
            .status(),
        200
    );

    assert_eq!(
        get(
            &client,
            addr,
            &stranger_jwt,
            &format!("/repositories/{repo_id}/collaborators")
        )
        .await
        .status(),
        404
    );

    let owner_view = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/collaborators"),
    )
    .await;
    assert_eq!(owner_view.status(), 200);
}
