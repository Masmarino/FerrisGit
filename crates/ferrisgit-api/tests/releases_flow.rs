mod common;

use common::git::{commit_file, git};

use common::http::{delete, get, get_json, login, patch, post, post_json, post_ok};

use serde_json::json;
use sqlx::PgPool;

#[sqlx::test]
async fn releases_are_maintainer_gated_drafts_stay_hidden_and_assets_round_trip(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;

    let client = reqwest::Client::new();

    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;

    let token_res: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/tokens",
        &json!({ "name": "ci" }),
    )
    .await;
    let plain_token = token_res["token"].as_str().unwrap();

    let repo_res: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    let repo_id = repo_res["id"].as_str().unwrap();

    post_ok(&client, addr, &owner_jwt, "/admin/users", &json!({ "username": "contributor-carl", "email": "carl@example.com", "password": "password12345" })).await;
    post_ok(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/collaborators"),
        &json!({ "username": "contributor-carl", "role": "contributor" }),
    )
    .await;
    let carl_jwt = login(&client, addr, "contributor-carl", "password12345").await;

    let clone_parent = tempfile::tempdir().unwrap();
    let clone_url = format!("http://admin:{plain_token}@{addr}/admin/hello.git");
    let repo_path = clone_parent.path().join("repo");
    git(&["clone", &clone_url, "repo"], clone_parent.path()).await;

    std::fs::write(repo_path.join("README.md"), "hello\n").unwrap();

    git(&["add", "."], &repo_path).await;

    git(
        &[
            "-c",
            "user.email=t@t.com",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "root",
        ],
        &repo_path,
    )
    .await;

    git(&["push", "-q", "origin", "HEAD:main"], &repo_path).await;

    let commit_sha = git(&["rev-parse", "HEAD"], &repo_path).await;

    let forbidden = post(&client, addr, &carl_jwt, &format!("/repositories/{repo_id}/releases"), &json!({ "tagName": "v1.0.0", "targetCommitSha": commit_sha, "title": "First release", "draft": false })).await;
    assert_eq!(forbidden.status(), reqwest::StatusCode::NOT_FOUND);

    // A '/' in a tag name mustn't reach the use case or git, it would break routes that treat {tag_name} as one segment.
    let invalid_tag = post(&client, addr, &owner_jwt, &format!("/repositories/{repo_id}/releases"), &json!({ "tagName": "release/1.0", "targetCommitSha": commit_sha, "title": "Bad tag", "draft": false })).await;
    assert!(
        invalid_tag.status().is_client_error(),
        "a tag name containing '/' must be rejected, got {}",
        invalid_tag.status()
    );

    let draft: serde_json::Value = post_json(&client, addr, &owner_jwt, &format!("/repositories/{repo_id}/releases"), &json!({ "tagName": "v1.0.0", "targetCommitSha": commit_sha, "title": "First release", "notes": "notes", "draft": true })).await;
    assert!(draft["draft"].as_bool().unwrap());
    assert!(draft["publishedAt"].is_null());

    // Checked over the wire via `ls-remote`, not by guessing the bare repo's on-disk layout.
    let verify_dir = tempfile::tempdir().unwrap();
    git(&["clone", "-q", &clone_url, "verify"], verify_dir.path()).await;
    let verify_path = verify_dir.path().join("verify");
    let remote_tags = git(&["ls-remote", "--tags", "origin"], &verify_path).await;
    assert!(
        remote_tags.contains("refs/tags/v1.0.0"),
        "the release's tag must be a real, pushed-visible git tag"
    );

    let carl_sees: serde_json::Value = get_json(
        &client,
        addr,
        &carl_jwt,
        &format!("/repositories/{repo_id}/releases"),
    )
    .await;
    assert!(carl_sees.as_array().unwrap().is_empty());
    let carl_detail = get(
        &client,
        addr,
        &carl_jwt,
        &format!("/repositories/{repo_id}/releases/v1.0.0"),
    )
    .await;
    assert_eq!(carl_detail.status(), reqwest::StatusCode::NOT_FOUND);

    let owner_detail: serde_json::Value = get_json(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/releases/v1.0.0"),
    )
    .await;
    assert_eq!(
        owner_detail["targetCommitSha"].as_str().unwrap(),
        commit_sha
    );

    patch(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/releases/v1.0.0"),
        &json!({ "draft": false }),
    )
    .await
    .error_for_status()
    .unwrap();
    let carl_sees_now: serde_json::Value = get_json(
        &client,
        addr,
        &carl_jwt,
        &format!("/repositories/{repo_id}/releases"),
    )
    .await;
    assert_eq!(carl_sees_now.as_array().unwrap().len(), 1);

    let asset_bytes = b"binary content, not really a tarball".to_vec();
    let part = reqwest::multipart::Part::bytes(asset_bytes.clone())
        .file_name("archive.tar.gz")
        .mime_str("application/gzip")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);
    let uploaded: serde_json::Value = client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/releases/v1.0.0/assets"
        ))
        .bearer_auth(owner_jwt)
        .multipart(form)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let asset_id = uploaded["id"].as_str().unwrap();
    assert_eq!(uploaded["filename"], "archive.tar.gz");

    let downloaded = get(
        &client,
        addr,
        &carl_jwt,
        &format!("/repositories/{repo_id}/releases/v1.0.0/assets/{asset_id}"),
    )
    .await;
    assert_eq!(downloaded.status(), reqwest::StatusCode::OK);
    let downloaded_bytes = downloaded.bytes().await.unwrap();
    assert_eq!(downloaded_bytes.as_ref(), asset_bytes.as_slice());

    let forbidden_delete_asset = delete(
        &client,
        addr,
        &carl_jwt,
        &format!("/repositories/{repo_id}/releases/v1.0.0/assets/{asset_id}"),
    )
    .await;
    assert_eq!(
        forbidden_delete_asset.status(),
        reqwest::StatusCode::NOT_FOUND
    );
    let forbidden_delete_release = delete(
        &client,
        addr,
        &carl_jwt,
        &format!("/repositories/{repo_id}/releases/v1.0.0"),
    )
    .await;
    assert_eq!(
        forbidden_delete_release.status(),
        reqwest::StatusCode::NOT_FOUND
    );

    // 404, not 403, so a release's existence isn't revealed to someone who can't manage it.
    let forbidden_update = patch(
        &client,
        addr,
        &carl_jwt,
        &format!("/repositories/{repo_id}/releases/v1.0.0"),
        &json!({ "title": "Hijacked title" }),
    )
    .await;
    assert_eq!(forbidden_update.status(), reqwest::StatusCode::NOT_FOUND);
}

/// The target commit is read live from git, so a deleted tag gives `targetCommitSha: null`, not a 500.
#[sqlx::test]
async fn a_release_stays_viewable_with_a_null_target_commit_after_its_git_tag_is_deleted(
    pool: PgPool,
) {
    let addr = common::spawn_app(pool).await.addr;

    let client = reqwest::Client::new();

    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;

    let token_res: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/tokens",
        &json!({ "name": "ci" }),
    )
    .await;
    let plain_token = token_res["token"].as_str().unwrap();

    let repo_res: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    let repo_id = repo_res["id"].as_str().unwrap();

    let clone_parent = tempfile::tempdir().unwrap();
    let clone_url = format!("http://admin:{plain_token}@{addr}/admin/hello.git");
    let repo_path = clone_parent.path().join("repo");
    git(&["clone", &clone_url, "repo"], clone_parent.path()).await;

    commit_file(&repo_path, "README.md", "hello\n", "root").await;
    git(&["push", "-q", "origin", "HEAD:main"], &repo_path).await;
    let commit_sha = git(&["rev-parse", "HEAD"], &repo_path).await;

    let release: serde_json::Value = post_json(&client, addr, &owner_jwt, &format!("/repositories/{repo_id}/releases"), &json!({ "tagName": "v1.0.0", "targetCommitSha": commit_sha, "title": "First release", "draft": false })).await;
    assert_eq!(release["tagName"], "v1.0.0");

    git(&["push", "-q", "origin", "--delete", "v1.0.0"], &repo_path).await;

    let detail = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/releases/v1.0.0"),
    )
    .await;
    assert_eq!(
        detail.status(),
        reqwest::StatusCode::OK,
        "the release must stay viewable, not 500, after its tag is deleted"
    );
    let detail: serde_json::Value = detail.json().await.unwrap();
    assert!(detail["targetCommitSha"].is_null());
    assert_eq!(detail["tagName"], "v1.0.0");
}

/// Deleting a release used to leave its git tag stuck. Tag deletion is Maintainer-only and refused while a release references it.
#[sqlx::test]
async fn deleting_a_tag_is_maintainer_gated_and_refused_while_a_release_still_uses_it(
    pool: PgPool,
) {
    let addr = common::spawn_app(pool).await.addr;

    let client = reqwest::Client::new();

    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;

    let token_res: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/tokens",
        &json!({ "name": "ci" }),
    )
    .await;
    let plain_token = token_res["token"].as_str().unwrap();

    let repo_res: serde_json::Value = post_json(
        &client,
        addr,
        &owner_jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    let repo_id = repo_res["id"].as_str().unwrap();

    post_ok(&client, addr, &owner_jwt, "/admin/users", &json!({ "username": "contributor-carl", "email": "carl@example.com", "password": "password12345" })).await;
    post_ok(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/collaborators"),
        &json!({ "username": "contributor-carl", "role": "contributor" }),
    )
    .await;
    let carl_jwt = login(&client, addr, "contributor-carl", "password12345").await;

    let clone_parent = tempfile::tempdir().unwrap();
    let clone_url = format!("http://admin:{plain_token}@{addr}/admin/hello.git");
    let repo_path = clone_parent.path().join("repo");
    git(&["clone", &clone_url, "repo"], clone_parent.path()).await;

    commit_file(&repo_path, "README.md", "hello\n", "root").await;
    git(&["push", "-q", "origin", "HEAD:main"], &repo_path).await;
    let commit_sha = git(&["rev-parse", "HEAD"], &repo_path).await;

    post_ok(&client, addr, &owner_jwt, &format!("/repositories/{repo_id}/releases"), &json!({ "tagName": "v1.0.0", "targetCommitSha": commit_sha, "title": "Wrong-commit draft", "draft": true })).await;

    let forbidden = delete(
        &client,
        addr,
        &carl_jwt,
        &format!("/repositories/{repo_id}/tags/v1.0.0"),
    )
    .await;
    assert_eq!(
        forbidden.status(),
        reqwest::StatusCode::NOT_FOUND,
        "insufficient role maps to 404 in this codebase, not 403"
    );

    let conflict = delete(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/tags/v1.0.0"),
    )
    .await;
    assert_eq!(conflict.status(), reqwest::StatusCode::CONFLICT);

    delete(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/releases/v1.0.0"),
    )
    .await
    .error_for_status()
    .unwrap();

    let tags_after_release_delete: Vec<serde_json::Value> = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/tags"),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(
        tags_after_release_delete.len(),
        1,
        "the tag itself must still exist — deleting a release never deletes its tag"
    );

    let deleted = delete(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/tags/v1.0.0"),
    )
    .await;
    assert_eq!(deleted.status(), reqwest::StatusCode::NO_CONTENT);

    let tags_after_tag_delete: Vec<serde_json::Value> = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/tags"),
    )
    .await
    .json()
    .await
    .unwrap();
    assert!(
        tags_after_tag_delete.is_empty(),
        "the tag must actually be gone"
    );

    let already_gone = delete(
        &client,
        addr,
        &owner_jwt,
        &format!("/repositories/{repo_id}/tags/v1.0.0"),
    )
    .await;
    assert_eq!(already_gone.status(), reqwest::StatusCode::NOT_FOUND);
}
