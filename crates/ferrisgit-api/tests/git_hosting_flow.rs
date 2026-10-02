mod common;

use common::git::git;

use common::http::{get_json, login, post_json};

use serde_json::json;
use sqlx::PgPool;
use std::process::Command;

#[sqlx::test]
async fn full_git_hosting_flow_works_end_to_end(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;

    let client = reqwest::Client::new();

    let jwt = login(&client, addr, "admin", "adminpassword123").await;

    let token_res: serde_json::Value =
        post_json(&client, addr, &jwt, "/tokens", &json!({ "name": "ci" })).await;
    let plain_token = token_res["token"].as_str().unwrap();

    let repo_res: serde_json::Value = post_json(
        &client,
        addr,
        &jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    assert_eq!(repo_res["name"], "hello");

    let clone_parent = tempfile::tempdir().unwrap();
    let clone_url = format!("http://admin:{plain_token}@{addr}/admin/hello.git");

    git(&["clone", &clone_url, "repo"], clone_parent.path()).await;

    let repo_path = clone_parent.path().join("repo");
    std::fs::write(repo_path.join("README.md"), "# hello").unwrap();

    git(&["add", "."], &repo_path).await;

    git(
        &[
            "-c",
            "user.email=ci@example.com",
            "-c",
            "user.name=ci",
            "commit",
            "-q",
            "-m",
            "init",
        ],
        &repo_path,
    )
    .await;

    git(&["push", "origin", "HEAD:main"], &repo_path).await;

    let commits_res: serde_json::Value =
        get_json(&client, addr, &jwt, "/repositories/admin/hello/commits").await;
    assert_eq!(commits_res.as_array().unwrap().len(), 1);

    // Regression: a push whose packfile exceeds axum's default 2 MiB body limit must succeed. The bytes are incompressible, so zlib cannot shrink the pack below the limit.
    let mut state_val: u64 = 0x9E3779B97F4A7C15;
    let large_file: Vec<u8> = (0..3 * 1024 * 1024)
        .map(|_| {
            state_val ^= state_val << 13;
            state_val ^= state_val >> 7;
            state_val ^= state_val << 17;
            (state_val & 0xFF) as u8
        })
        .collect();
    std::fs::write(repo_path.join("large.bin"), &large_file).unwrap();

    git(&["add", "."], &repo_path).await;

    git(
        &[
            "-c",
            "user.email=ci@example.com",
            "-c",
            "user.name=ci",
            "commit",
            "-q",
            "-m",
            "add large file",
        ],
        &repo_path,
    )
    .await;

    // A packfile above 2 MiB must get through the request body limit.
    git(&["push", "origin", "HEAD:main"], &repo_path).await;

    let commits_res_after_large: serde_json::Value =
        get_json(&client, addr, &jwt, "/repositories/admin/hello/commits").await;
    assert_eq!(commits_res_after_large.as_array().unwrap().len(), 2);

    let run_git = |args: Vec<&'static str>| {
        let repo_path = repo_path.clone();
        async move {
            tokio::task::spawn_blocking(move || {
                Command::new("git")
                    .args(args)
                    .current_dir(&repo_path)
                    .status()
            })
            .await
            .unwrap()
            .unwrap()
        }
    };
    assert!(
        run_git(vec!["checkout", "-q", "-b", "feature"])
            .await
            .success()
    );
    assert!(
        run_git(vec![
            "-c",
            "user.email=ci@example.com",
            "-c",
            "user.name=ci",
            "commit",
            "--allow-empty",
            "-q",
            "-m",
            "feature work"
        ])
        .await
        .success()
    );
    assert!(
        run_git(vec!["push", "-q", "origin", "feature"])
            .await
            .success()
    );

    let repo_id = repo_res["id"].as_str().unwrap();
    let get_commits = |url: String| {
        let client = client.clone();
        let jwt = jwt.clone();
        async move { client.get(url).bearer_auth(jwt).send().await.unwrap() }
    };
    let messages = |value: &serde_json::Value| -> Vec<String> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["message"].as_str().unwrap().to_string())
            .collect()
    };

    let blob_sha = {
        let out = tokio::task::spawn_blocking({
            let repo_path = repo_path.clone();
            move || {
                Command::new("git")
                    .args(["rev-parse", "HEAD:README.md"])
                    .current_dir(&repo_path)
                    .output()
            }
        })
        .await
        .unwrap()
        .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    };

    for base in [
        format!("http://{addr}/api/repositories/admin/hello/commits"),
        format!("http://{addr}/api/repositories/by-id/{repo_id}/commits"),
    ] {
        let head: serde_json::Value = get_commits(base.clone()).await.json().await.unwrap();
        assert_eq!(
            messages(&head),
            vec!["add large file", "init"],
            "no ref lists HEAD's history ({base})"
        );
        assert!(head[0]["authorEmail"].is_string());

        let feature: serde_json::Value = get_commits(format!("{base}?ref=feature"))
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(
            messages(&feature),
            vec!["feature work", "add large file", "init"],
            "ref=feature lists the branch history ({base})"
        );

        let main: serde_json::Value = get_commits(format!("{base}?ref=main"))
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(messages(&main), vec!["add large file", "init"]);

        let unknown = get_commits(format!("{base}?ref=does-not-exist")).await;
        assert_eq!(
            unknown.status(),
            reqwest::StatusCode::NOT_FOUND,
            "unknown ref is a 404 ({base})"
        );

        // A revision that resolves but is not a commit is also a 404, never a 500.
        for not_a_commit in [blob_sha.as_str(), "HEAD%5E%7Btree%7D", "HEAD:README.md"] {
            let res = get_commits(format!("{base}?ref={not_a_commit}")).await;
            assert_eq!(
                res.status(),
                reqwest::StatusCode::NOT_FOUND,
                "ref={not_a_commit} is a 404 ({base})"
            );
        }

        let explicit_head = get_commits(format!("{base}?ref=HEAD")).await;
        assert_eq!(
            explicit_head.status(),
            reqwest::StatusCode::OK,
            "ref=HEAD ({base})"
        );
        let explicit_head: serde_json::Value = explicit_head.json().await.unwrap();
        assert_eq!(
            explicit_head, head,
            "ref=HEAD lists the same commits as no ref ({base})"
        );
    }

    // An empty repository: HEAD is unborn, so both no ref and `?ref=HEAD` answer an empty list, not a 404.
    let empty_res: serde_json::Value = post_json(
        &client,
        addr,
        &jwt,
        "/repositories",
        &json!({ "name": "empty", "visibility": "private" }),
    )
    .await;
    let empty_id = empty_res["id"].as_str().unwrap();
    for base in [
        format!("http://{addr}/api/repositories/admin/empty/commits"),
        format!("http://{addr}/api/repositories/by-id/{empty_id}/commits"),
    ] {
        for url in [base.clone(), format!("{base}?ref=HEAD")] {
            let res = get_commits(url.clone()).await;
            assert_eq!(
                res.status(),
                reqwest::StatusCode::OK,
                "{url} on an empty repository"
            );
            let body: serde_json::Value = res.json().await.unwrap();
            assert_eq!(body, json!([]), "{url} on an empty repository");
        }
    }

    std::fs::create_dir(repo_path.join("docs")).unwrap();
    std::fs::write(repo_path.join("docs/guide.md"), "# guide").unwrap();
    assert!(run_git(vec!["add", "."]).await.success());
    assert!(
        run_git(vec![
            "-c",
            "user.email=ci@example.com",
            "-c",
            "user.name=ci",
            "commit",
            "-q",
            "-m",
            "add docs"
        ])
        .await
        .success()
    );
    assert!(
        run_git(vec!["push", "-q", "origin", "feature"])
            .await
            .success()
    );

    let tree_base = format!("http://{addr}/api/repositories/by-id/{repo_id}/tree/feature");
    for (url, expected_names) in [
        (tree_base.clone(), vec!["README.md", "docs", "large.bin"]),
        (format!("{tree_base}/docs"), vec!["guide.md"]),
    ] {
        let entries = |value: &serde_json::Value| -> Vec<(String, bool)> {
            let mut entries: Vec<(String, bool)> = value
                .as_array()
                .unwrap()
                .iter()
                .map(|e| {
                    (
                        e["name"].as_str().unwrap().to_string(),
                        e["isDir"].as_bool().unwrap(),
                    )
                })
                .collect();
            entries.sort();
            entries
        };

        let full: serde_json::Value = get_commits(url.clone()).await.json().await.unwrap();
        assert_eq!(
            entries(&full)
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            expected_names,
            "{url}"
        );
        assert!(
            full.as_array()
                .unwrap()
                .iter()
                .all(|e| e["lastCommit"].is_object()),
            "the default listing carries each entry's last commit ({url})"
        );

        let names_only = get_commits(format!("{url}?lastCommit=false")).await;
        assert_eq!(
            names_only.status(),
            reqwest::StatusCode::OK,
            "{url}?lastCommit=false"
        );
        let names_only: serde_json::Value = names_only.json().await.unwrap();
        assert_eq!(
            entries(&names_only),
            entries(&full),
            "same entries with lastCommit=false ({url})"
        );
        assert!(
            names_only
                .as_array()
                .unwrap()
                .iter()
                .all(|e| e.get("lastCommit") == Some(&serde_json::Value::Null)),
            "every lastCommit is null with lastCommit=false ({url})"
        );

        let explicit_true: serde_json::Value = get_commits(format!("{url}?lastCommit=true"))
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(
            explicit_true, full,
            "lastCommit=true is the default ({url})"
        );
    }
}
