mod common;

use common::USER_PASSWORD;
use common::git::git;

use common::http::{create_user, delete, get, get_json, login, post, post_json};

use serde_json::json;
use sqlx::PgPool;
use std::process::Command;

#[sqlx::test]
async fn nested_group_member_can_clone_and_push_while_creator_loses_access_after_role_removal(
    pool: PgPool,
) {
    let addr = common::spawn_app(pool).await.addr;

    let client = reqwest::Client::new();

    let admin_jwt = login(&client, addr, "admin", "adminpassword123").await;

    for username in ["creator", "member", "colead"] {
        create_user(&client, addr, &admin_jwt, username).await;
    }

    async fn api_token(client: &reqwest::Client, addr: std::net::SocketAddr, jwt: &str) -> String {
        let res: serde_json::Value =
            post_json(client, addr, jwt, "/tokens", &json!({ "name": "ci" })).await;
        res["token"].as_str().unwrap().to_string()
    }

    let creator_jwt = login(&client, addr, "creator", USER_PASSWORD).await;
    let member_jwt = login(&client, addr, "member", USER_PASSWORD).await;

    let creator_token = api_token(&client, addr, &creator_jwt).await;
    let member_token = api_token(&client, addr, &member_jwt).await;

    let acme_res = post(
        &client,
        addr,
        &creator_jwt,
        "/groups",
        &json!({ "name": "acme", "description": "Acme Corp" }),
    )
    .await;
    assert_eq!(acme_res.status(), 200);
    let acme_body: serde_json::Value = acme_res.json().await.unwrap();
    let acme_id = acme_body["id"].as_str().unwrap();

    // Creating a group makes the caller a direct Maintainer of it, so `creator` has two memberships to remove later.

    let backend_res = post(
        &client,
        addr,
        &creator_jwt,
        &format!("/groups/{acme_id}/subgroups"),
        &json!({ "name": "backend", "description": "Backend team" }),
    )
    .await;
    assert_eq!(backend_res.status(), 200);
    let backend_body: serde_json::Value = backend_res.json().await.unwrap();
    let backend_id = backend_body["id"].as_str().unwrap();

    let add_member_status = post(
        &client,
        addr,
        &creator_jwt,
        &format!("/groups/{backend_id}/members"),
        &json!({ "username": "member", "role": "contributor" }),
    )
    .await
    .status();
    assert_eq!(add_member_status, 200);

    // `colead` is there so removing creator's roles is allowed: the last Maintainer of a hierarchy can't be removed.
    let add_colead_status = post(
        &client,
        addr,
        &creator_jwt,
        &format!("/groups/{acme_id}/members"),
        &json!({ "username": "colead", "role": "maintainer" }),
    )
    .await
    .status();
    assert_eq!(add_colead_status, 200);

    let create_repo_res = post(&client, addr, &creator_jwt, "/repositories", &json!({ "name": "terraform-modules", "visibility": "private", "groupPath": "acme/backend" })).await;
    assert_eq!(create_repo_res.status(), 200);

    let clone_parent = tempfile::tempdir().unwrap();
    let member_clone_url =
        format!("http://member:{member_token}@{addr}/acme/backend/terraform-modules.git");

    let member_clone_status = tokio::task::spawn_blocking({
        let url = member_clone_url.clone();
        let dir = clone_parent.path().to_path_buf();
        move || {
            Command::new("git")
                .args(["clone", &url, "member-repo"])
                .current_dir(&dir)
                .status()
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(
        member_clone_status.success(),
        "a group Contributor with only a subgroup-level grant must be able to clone a repository two levels deep"
    );

    let member_repo_path = clone_parent.path().join("member-repo");
    std::fs::write(member_repo_path.join("main.tf"), "# terraform").unwrap();

    git(&["add", "."], &member_repo_path).await;

    git(
        &[
            "-c",
            "user.email=member@example.com",
            "-c",
            "user.name=member",
            "commit",
            "-q",
            "-m",
            "add terraform module",
        ],
        &member_repo_path,
    )
    .await;

    // A Contributor on the subgroup alone can push to a repository two levels deep.
    git(&["push", "origin", "HEAD:main"], &member_repo_path).await;

    let resolve_res: serde_json::Value = get_json(
        &client,
        addr,
        &member_jwt,
        "/resolve/acme/backend/terraform-modules",
    )
    .await;
    let repository_id = resolve_res["repositoryId"].as_str().unwrap().to_string();

    // creator keeps owner_id but has no group role, so the clone is refused: there is no owner bypass.

    let remove_creator_from_acme_status = delete(
        &client,
        addr,
        &creator_jwt,
        &format!("/groups/{acme_id}/members/creator"),
    )
    .await
    .status();
    assert_eq!(
        remove_creator_from_acme_status, 200,
        "removing creator's role on the root group must succeed"
    );

    let remove_creator_from_backend_status = delete(
        &client,
        addr,
        &creator_jwt,
        &format!("/groups/{backend_id}/members/creator"),
    )
    .await
    .status();
    assert_eq!(
        remove_creator_from_backend_status, 200,
        "removing creator's direct role on the subgroup must succeed"
    );

    let creator_by_id_status = get(
        &client,
        addr,
        &creator_jwt,
        &format!("/repositories/by-id/{repository_id}"),
    )
    .await
    .status();
    assert_eq!(
        creator_by_id_status, 404,
        "the creator must lose API read access too once their only group roles are removed — owner_id must grant no implicit access on a group repository"
    );

    let creator_clone_url =
        format!("http://creator:{creator_token}@{addr}/acme/backend/terraform-modules.git");
    let creator_clone_output = tokio::task::spawn_blocking({
        let url = creator_clone_url.clone();
        let dir = clone_parent.path().to_path_buf();
        move || {
            Command::new("git")
                .args(["clone", &url, "creator-repo"])
                .current_dir(&dir)
                .output()
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(
        !creator_clone_output.status.success(),
        "the repository creator must be rejected after their only group roles are removed — owner_id must grant no implicit access on a group repository, got stdout={} stderr={}",
        String::from_utf8_lossy(&creator_clone_output.stdout),
        String::from_utf8_lossy(&creator_clone_output.stderr)
    );
    let creator_clone_stderr = String::from_utf8_lossy(&creator_clone_output.stderr).to_lowercase();
    assert!(
        creator_clone_stderr.contains("401")
            || creator_clone_stderr.contains("auth")
            || creator_clone_stderr.contains("denied")
            || creator_clone_stderr.contains("fatal"),
        "expected an authentication-failure indication in git's stderr, got: {creator_clone_stderr}"
    );

    // Check the endpoint's own 401 challenge, not just what the git CLI makes of it.
    let creator_info_refs_res = client
        .get(format!(
            "http://{addr}/acme/backend/terraform-modules.git/info/refs?service=git-upload-pack"
        ))
        .basic_auth("creator", Some(&creator_token))
        .send()
        .await
        .unwrap();
    assert_eq!(
        creator_info_refs_res.status(),
        401,
        "the creator's token must be rejected by the git smart-HTTP endpoint directly, not just via the git CLI wrapper"
    );
    assert!(
        creator_info_refs_res
            .headers()
            .contains_key(axum::http::header::WWW_AUTHENTICATE),
        "a 401 to a git client must carry the WWW-Authenticate challenge header"
    );

    // No `.git` in the URL means the SPA, not git.

    let spa_res = client
        .get(format!("http://{addr}/acme/backend/terraform-modules"))
        .send()
        .await
        .unwrap();
    assert_ne!(
        spa_res.status(),
        401,
        "a non-.git repository sub-page URL must never be treated as a git request (no WWW-Authenticate challenge)"
    );
    assert!(
        !spa_res
            .headers()
            .contains_key(axum::http::header::WWW_AUTHENTICATE),
        "the SPA fallback must never emit a git WWW-Authenticate challenge"
    );
    let spa_body = spa_res.text().await.unwrap();
    assert_eq!(
        spa_body,
        common::SHELL,
        "a .git-less repository sub-page URL must be served the SPA's index.html, not dispatched to the git handler"
    );
}
