mod common;

use common::USER_PASSWORD;
use common::http::{create_user, get, get_json, login, patch, post, post_json, put};

use serde_json::json;
use sqlx::PgPool;
use std::process::Command;

#[sqlx::test]
async fn role_gating_matches_reader_contributor_maintainer_across_git_settings_and_merge_requests(
    pool: PgPool,
) {
    let addr = common::spawn_app(pool).await.addr;

    let client = reqwest::Client::new();
    let run_git = |args: Vec<String>, cwd: std::path::PathBuf| {
        tokio::task::spawn_blocking(move || {
            Command::new("git").args(&args).current_dir(&cwd).status()
        })
    };
    let run_git_output = |args: Vec<String>, cwd: std::path::PathBuf| {
        tokio::task::spawn_blocking(move || {
            Command::new("git").args(&args).current_dir(&cwd).output()
        })
    };

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
    assert_eq!(
        repo_res["path"],
        json!(["admin", "hello"]),
        "a personal repository's resolvable path is [owner_username, name]"
    );
    let repo_id = repo_res["id"].as_str().unwrap().to_string();

    let owner_token_res: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/tokens",
        &json!({ "name": "owner-ci" }),
    )
    .await;
    let owner_plain_token = owner_token_res["token"].as_str().unwrap();

    let owner_clone_parent = tempfile::tempdir().unwrap();
    let owner_clone_url = format!("http://admin:{owner_plain_token}@{addr}/admin/hello.git");
    let owner_repo_path = owner_clone_parent.path().join("repo");
    assert!(
        run_git(
            vec!["clone".to_string(), owner_clone_url, "repo".to_string()],
            owner_clone_parent.path().to_path_buf()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    std::fs::write(owner_repo_path.join("README.md"), "line one\n").unwrap();
    assert!(
        run_git(
            vec!["add".to_string(), ".".to_string()],
            owner_repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    assert!(
        run_git(
            vec![
                "-c".to_string(),
                "user.email=o@o.com".to_string(),
                "-c".to_string(),
                "user.name=owner".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "root".to_string()
            ],
            owner_repo_path.clone()
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
            owner_repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    let root_sha_output = run_git_output(
        vec!["rev-parse".to_string(), "HEAD".to_string()],
        owner_repo_path.clone(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(root_sha_output.status.success());
    let root_sha = String::from_utf8(root_sha_output.stdout)
        .unwrap()
        .trim()
        .to_string();

    for username in ["reader", "contributor", "maintainer"] {
        create_user(&client, addr, &owner_jwt, username).await;
    }

    let reader_jwt = login(&client, addr, "reader", USER_PASSWORD).await;
    let contributor_jwt = login(&client, addr, "contributor", USER_PASSWORD).await;
    let maintainer_jwt = login(&client, addr, "maintainer", USER_PASSWORD).await;

    for (username, role) in [
        ("reader", "reader"),
        ("contributor", "contributor"),
        ("maintainer", "maintainer"),
    ] {
        let status = post(
            &client,
            addr,
            &owner_jwt,
            &format!("/repositories/{repo_id}/collaborators"),
            &json!({ "username": username, "role": role }),
        )
        .await
        .status();
        assert_eq!(
            status, 204,
            "expected adding {username} as {role} to return 204"
        );
    }

    async fn main_tip_sha(
        client: &reqwest::Client,
        addr: std::net::SocketAddr,
        jwt: &str,
        repo_id: &str,
    ) -> String {
        let branches: serde_json::Value = get_json(
            client,
            addr,
            jwt,
            &format!("/repositories/{repo_id}/branches"),
        )
        .await;
        branches
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["name"] == "main")
            .unwrap()["tipSha"]
            .as_str()
            .unwrap()
            .to_string()
    }

    let get_repo_status = get(&client, addr, &reader_jwt, "/repositories/admin/hello")
        .await
        .status();
    assert_eq!(get_repo_status, 200);

    let reader_token_res: serde_json::Value = post_json(
        &client,
        addr,
        &reader_jwt,
        "/tokens",
        &json!({ "name": "reader-ci" }),
    )
    .await;
    let reader_plain_token = reader_token_res["token"].as_str().unwrap();

    let sha_before_reader_push = main_tip_sha(&client, addr, &reader_jwt, &repo_id).await;
    assert_eq!(
        sha_before_reader_push, root_sha,
        "sanity check: main's tip should still be the owner's root commit before the reader's push attempt"
    );

    let reader_clone_parent = tempfile::tempdir().unwrap();
    let reader_clone_url = format!("http://reader:{reader_plain_token}@{addr}/admin/hello.git");
    let reader_repo_path = reader_clone_parent.path().join("repo");
    assert!(
        run_git(
            vec!["clone".to_string(), reader_clone_url, "repo".to_string()],
            reader_clone_parent.path().to_path_buf()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    std::fs::write(
        reader_repo_path.join("README.md"),
        "line one\nreader was here\n",
    )
    .unwrap();
    assert!(
        run_git(
            vec!["add".to_string(), ".".to_string()],
            reader_repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    assert!(
        run_git(
            vec![
                "-c".to_string(),
                "user.email=r@r.com".to_string(),
                "-c".to_string(),
                "user.name=reader".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "reader tries to push".to_string()
            ],
            reader_repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    let reader_push_status = run_git(
        vec![
            "push".to_string(),
            "-q".to_string(),
            "origin".to_string(),
            "HEAD:main".to_string(),
        ],
        reader_repo_path.clone(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        !reader_push_status.success(),
        "a reader's git push must be rejected"
    );

    let sha_after_reader_push = main_tip_sha(&client, addr, &reader_jwt, &repo_id).await;
    assert_eq!(
        sha_after_reader_push, root_sha,
        "a rejected reader push must not move main's tip sha"
    );

    let mr_list_before_reader_create: serde_json::Value = get_json(
        &client,
        addr,
        &reader_jwt,
        &format!("/repositories/{repo_id}/merge-requests"),
    )
    .await;
    assert_eq!(mr_list_before_reader_create.as_array().unwrap().len(), 0);

    let reader_create_mr_status = post(&client, addr, &reader_jwt, &format!("/repositories/{repo_id}/merge-requests"), &json!({ "sourceBranch": "main", "targetBranch": "main", "title": "reader mr", "description": "" })).await
        .status();
    assert_eq!(reader_create_mr_status, 404);

    let mr_list_after_reader_create: serde_json::Value = get_json(
        &client,
        addr,
        &reader_jwt,
        &format!("/repositories/{repo_id}/merge-requests"),
    )
    .await;
    assert_eq!(
        mr_list_after_reader_create.as_array().unwrap().len(),
        0,
        "a rejected merge-request creation must not create anything"
    );

    let reader_settings_status = get(
        &client,
        addr,
        &reader_jwt,
        &format!("/repositories/{repo_id}/settings"),
    )
    .await
    .status();
    assert_eq!(reader_settings_status, 404);

    let contributor_token_res: serde_json::Value = post_json(
        &client,
        addr,
        &contributor_jwt,
        "/tokens",
        &json!({ "name": "contributor-ci" }),
    )
    .await;
    let contributor_plain_token = contributor_token_res["token"].as_str().unwrap();

    let contributor_clone_parent = tempfile::tempdir().unwrap();
    let contributor_clone_url =
        format!("http://contributor:{contributor_plain_token}@{addr}/admin/hello.git");
    let contributor_repo_path = contributor_clone_parent.path().join("repo");
    assert!(
        run_git(
            vec![
                "clone".to_string(),
                contributor_clone_url,
                "repo".to_string()
            ],
            contributor_clone_parent.path().to_path_buf()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    std::fs::write(
        contributor_repo_path.join("README.md"),
        "line one\ncontributor was here\n",
    )
    .unwrap();
    assert!(
        run_git(
            vec!["add".to_string(), ".".to_string()],
            contributor_repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    assert!(
        run_git(
            vec![
                "-c".to_string(),
                "user.email=c@c.com".to_string(),
                "-c".to_string(),
                "user.name=contributor".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "contributor push".to_string()
            ],
            contributor_repo_path.clone()
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
            contributor_repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    let contributor_push_sha_output = run_git_output(
        vec!["rev-parse".to_string(), "HEAD".to_string()],
        contributor_repo_path.clone(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(contributor_push_sha_output.status.success());
    let contributor_push_sha = String::from_utf8(contributor_push_sha_output.stdout)
        .unwrap()
        .trim()
        .to_string();

    let sha_after_contributor_push = main_tip_sha(&client, addr, &contributor_jwt, &repo_id).await;
    assert_eq!(
        sha_after_contributor_push, contributor_push_sha,
        "a successful contributor push must move main's tip sha to the new commit"
    );

    assert!(
        run_git(
            vec![
                "checkout".to_string(),
                "-q".to_string(),
                "-b".to_string(),
                "contributor-feature".to_string()
            ],
            contributor_repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    std::fs::write(contributor_repo_path.join("FEATURE.md"), "feature work\n").unwrap();
    assert!(
        run_git(
            vec!["add".to_string(), ".".to_string()],
            contributor_repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );
    assert!(
        run_git(
            vec![
                "-c".to_string(),
                "user.email=c@c.com".to_string(),
                "-c".to_string(),
                "user.name=contributor".to_string(),
                "commit".to_string(),
                "-q".to_string(),
                "-m".to_string(),
                "feature work".to_string()
            ],
            contributor_repo_path.clone()
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
                "HEAD:contributor-feature".to_string()
            ],
            contributor_repo_path.clone()
        )
        .await
        .unwrap()
        .unwrap()
        .success()
    );

    let contributor_mr_res = post(&client, addr, &contributor_jwt, &format!("/repositories/{repo_id}/merge-requests"), &json!({ "sourceBranch": "contributor-feature", "targetBranch": "main", "title": "Add feature", "description": "" })).await;
    assert_eq!(contributor_mr_res.status(), 200);
    let contributor_mr_body: serde_json::Value = contributor_mr_res.json().await.unwrap();
    assert_eq!(contributor_mr_body["sourceBranch"], "contributor-feature");
    assert_eq!(contributor_mr_body["status"], "open");

    let contributor_get_settings_status = get(
        &client,
        addr,
        &contributor_jwt,
        &format!("/repositories/{repo_id}/settings"),
    )
    .await
    .status();
    assert_eq!(contributor_get_settings_status, 404);

    let settings_before_contributor_put: serde_json::Value = get_json(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/settings"),
    )
    .await;
    assert_eq!(
        settings_before_contributor_put["pipelineFilePath"],
        ".ferrisgit-ci.yml"
    );

    let contributor_put_settings_status = put(
        &client,
        addr,
        &contributor_jwt,
        &format!("/repositories/{repo_id}/settings"),
        &json!({ "pipelineFilePath": "should-not-apply.yml" }),
    )
    .await
    .status();
    assert_eq!(contributor_put_settings_status, 404);

    let settings_after_contributor_put: serde_json::Value = get_json(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/settings"),
    )
    .await;
    assert_eq!(
        settings_after_contributor_put["pipelineFilePath"], ".ferrisgit-ci.yml",
        "a rejected settings PUT must not change anything"
    );

    let collaborators_before_contributor_add: serde_json::Value = get_json(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/collaborators"),
    )
    .await;
    assert_eq!(
        collaborators_before_contributor_add
            .as_array()
            .unwrap()
            .len(),
        3
    );

    let contributor_add_collaborator_status = post(
        &client,
        addr,
        &contributor_jwt,
        &format!("/repositories/{repo_id}/collaborators"),
        &json!({ "username": "admin", "role": "reader" }),
    )
    .await
    .status();
    assert_eq!(contributor_add_collaborator_status, 404);

    let collaborators_after_contributor_add: serde_json::Value = get_json(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/collaborators"),
    )
    .await;
    assert_eq!(
        collaborators_after_contributor_add
            .as_array()
            .unwrap()
            .len(),
        3,
        "a rejected add-collaborator must not add anything"
    );

    let maintainer_get_settings_status = get(
        &client,
        addr,
        &maintainer_jwt,
        &format!("/repositories/{repo_id}/settings"),
    )
    .await
    .status();
    assert_eq!(maintainer_get_settings_status, 200);

    let maintainer_put_settings_res = put(
        &client,
        addr,
        &maintainer_jwt,
        &format!("/repositories/{repo_id}/settings"),
        &json!({ "pipelineFilePath": "custom-pipeline.yml" }),
    )
    .await;
    assert_eq!(maintainer_put_settings_res.status(), 200);
    let maintainer_put_settings_body: serde_json::Value =
        maintainer_put_settings_res.json().await.unwrap();
    assert_eq!(
        maintainer_put_settings_body["pipelineFilePath"],
        "custom-pipeline.yml"
    );

    create_user(&client, addr, &owner_jwt, "extra").await;

    let maintainer_add_collaborator_status = post(
        &client,
        addr,
        &maintainer_jwt,
        &format!("/repositories/{repo_id}/collaborators"),
        &json!({ "username": "extra", "role": "reader" }),
    )
    .await
    .status();
    assert_eq!(maintainer_add_collaborator_status, 204);

    let maintainer_set_role_status = patch(
        &client,
        addr,
        &maintainer_jwt,
        &format!("/repositories/{repo_id}/collaborators/reader"),
        &json!({ "role": "contributor" }),
    )
    .await
    .status();
    assert_eq!(maintainer_set_role_status, 204);

    let final_collaborators: serde_json::Value = get_json(
        &client,
        addr,
        &maintainer_jwt,
        &format!("/repositories/{repo_id}/collaborators"),
    )
    .await;
    let final_collaborators = final_collaborators.as_array().unwrap();
    // The owner has no collaborator row, so this is the original three plus `extra`.
    assert_eq!(
        final_collaborators.len(),
        4,
        "expected reader, contributor, maintainer, and extra to all be listed, got: {final_collaborators:#?}"
    );
    let readers_row = final_collaborators
        .iter()
        .find(|c| c["username"] == "reader")
        .unwrap();
    assert_eq!(
        readers_row["role"], "contributor",
        "reader's role must now report as contributor after the maintainer's role change"
    );
    let extras_row = final_collaborators
        .iter()
        .find(|c| c["username"] == "extra")
        .unwrap();
    assert_eq!(
        extras_row["role"], "reader",
        "extra must appear with the role they were added at"
    );
}
