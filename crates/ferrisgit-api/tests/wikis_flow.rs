mod common;

use common::git::push_files;
use common::http::{
    add_collaborator, create_user, delete, get, get_json, login, mint_api_token, post_json, put,
};

use serde_json::json;
use sqlx::PgPool;
use std::net::SocketAddr;

struct Roles {
    owner_jwt: String,
    maintainer_jwt: String,
    maintainer_username: String,
    maintainer_token: String,
    contributor_jwt: String,
    reader_jwt: String,
}

async fn seed_server_with_repo_and_roles(pool: PgPool) -> (SocketAddr, String, String, Roles) {
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
    let repo_id = repo_res["id"].as_str().unwrap().to_string();
    let path_segments: Vec<String> = repo_res["path"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let path_segment = path_segments.join("/");

    for username in ["maintainer", "contributor", "reader"] {
        create_user(&client, addr, &owner_jwt, username).await;
    }
    add_collaborator(
        &client,
        addr,
        &owner_jwt,
        &repo_id,
        "maintainer",
        "maintainer",
    )
    .await;
    add_collaborator(
        &client,
        addr,
        &owner_jwt,
        &repo_id,
        "contributor",
        "contributor",
    )
    .await;
    add_collaborator(&client, addr, &owner_jwt, &repo_id, "reader", "reader").await;

    let maintainer_jwt = login(&client, addr, "maintainer", "password12345").await;
    let contributor_jwt = login(&client, addr, "contributor", "password12345").await;
    let reader_jwt = login(&client, addr, "reader", "password12345").await;
    let maintainer_token = mint_api_token(&client, addr, &maintainer_jwt, "maintainer-ci").await;

    (
        addr,
        repo_id,
        path_segment,
        Roles {
            owner_jwt,
            maintainer_jwt,
            maintainer_username: "maintainer".to_string(),
            maintainer_token,
            contributor_jwt,
            reader_jwt,
        },
    )
}

#[sqlx::test]
async fn a_contributor_can_create_and_edit_but_not_delete_a_page(pool: PgPool) {
    let (addr, repository_id, _path, users) = seed_server_with_repo_and_roles(pool).await;
    let client = reqwest::Client::new();

    let create_resp = put(
        &client,
        addr,
        &users.contributor_jwt,
        &format!("/repositories/{repository_id}/wiki/pages/Home"),
        &json!({ "content": "# Home", "baseSha": null }),
    )
    .await;
    assert_eq!(create_resp.status(), 200);
    let created: serde_json::Value = create_resp.json().await.unwrap();
    let head_sha = created["headSha"].as_str().unwrap().to_string();

    let edit_resp = put(
        &client,
        addr,
        &users.contributor_jwt,
        &format!("/repositories/{repository_id}/wiki/pages/Home"),
        &json!({ "content": "# Home v2", "baseSha": head_sha }),
    )
    .await;
    assert_eq!(edit_resp.status(), 200);

    let latest: serde_json::Value = get_json(
        &client,
        addr,
        &users.contributor_jwt,
        &format!("/repositories/{repository_id}/wiki/pages/Home"),
    )
    .await;
    let delete_resp = delete(
        &client,
        addr,
        &users.contributor_jwt,
        &format!(
            "/repositories/{repository_id}/wiki/pages/Home?baseSha={}",
            latest["headSha"].as_str().unwrap()
        ),
    )
    .await;
    assert_eq!(
        delete_resp.status(),
        404,
        "this codebase's convention is 404, not 403, for insufficient role"
    );
}

#[sqlx::test]
async fn a_maintainer_can_delete_a_page_a_reader_cannot_write_at_all(pool: PgPool) {
    let (addr, repository_id, _path, users) = seed_server_with_repo_and_roles(pool).await;
    let client = reqwest::Client::new();

    let create_resp = put(
        &client,
        addr,
        &users.maintainer_jwt,
        &format!("/repositories/{repository_id}/wiki/pages/Home"),
        &json!({ "content": "# Home", "baseSha": null }),
    )
    .await;
    let created: serde_json::Value = create_resp.json().await.unwrap();

    let read_resp = get(
        &client,
        addr,
        &users.reader_jwt,
        &format!("/repositories/{repository_id}/wiki/pages/Home"),
    )
    .await;
    assert_eq!(read_resp.status(), 200);

    let write_resp = put(
        &client,
        addr,
        &users.reader_jwt,
        &format!("/repositories/{repository_id}/wiki/pages/About"),
        &json!({ "content": "# About", "baseSha": created["headSha"] }),
    )
    .await;
    assert_eq!(write_resp.status(), 404);

    let delete_resp = delete(
        &client,
        addr,
        &users.maintainer_jwt,
        &format!(
            "/repositories/{repository_id}/wiki/pages/Home?baseSha={}",
            created["headSha"].as_str().unwrap()
        ),
    )
    .await;
    assert_eq!(delete_resp.status(), 204);
}

#[sqlx::test]
async fn a_stale_base_sha_is_rejected_but_a_retry_with_the_fresh_sha_succeeds(pool: PgPool) {
    let (addr, repository_id, _path, users) = seed_server_with_repo_and_roles(pool).await;
    let client = reqwest::Client::new();

    let create_resp = put(
        &client,
        addr,
        &users.maintainer_jwt,
        &format!("/repositories/{repository_id}/wiki/pages/Home"),
        &json!({ "content": "# v1", "baseSha": null }),
    )
    .await;
    let created: serde_json::Value = create_resp.json().await.unwrap();
    let stale_sha = created["headSha"].as_str().unwrap().to_string();

    put(
        &client,
        addr,
        &users.maintainer_jwt,
        &format!("/repositories/{repository_id}/wiki/pages/Home"),
        &json!({ "content": "# v2", "baseSha": stale_sha }),
    )
    .await;

    let conflict_resp = put(
        &client,
        addr,
        &users.maintainer_jwt,
        &format!("/repositories/{repository_id}/wiki/pages/Home"),
        &json!({ "content": "# v3 (stale)", "baseSha": stale_sha }),
    )
    .await;
    assert_eq!(conflict_resp.status(), 409);

    let fresh: serde_json::Value = get_json(
        &client,
        addr,
        &users.maintainer_jwt,
        &format!("/repositories/{repository_id}/wiki/pages/Home"),
    )
    .await;
    let retry_resp = put(
        &client,
        addr,
        &users.maintainer_jwt,
        &format!("/repositories/{repository_id}/wiki/pages/Home"),
        &json!({ "content": "# v3", "baseSha": fresh["headSha"] }),
    )
    .await;
    assert_eq!(retry_resp.status(), 200);
}

#[sqlx::test]
async fn revisions_list_history_most_recent_first_and_revision_content_returns_historical_text(
    pool: PgPool,
) {
    let (addr, repository_id, _path, users) = seed_server_with_repo_and_roles(pool).await;
    let client = reqwest::Client::new();

    let v1: serde_json::Value = put(
        &client,
        addr,
        &users.contributor_jwt,
        &format!("/repositories/{repository_id}/wiki/pages/Home"),
        &json!({ "content": "# v1", "baseSha": null, "message": "first" }),
    )
    .await
    .json()
    .await
    .unwrap();
    let v1_sha = v1["headSha"].as_str().unwrap().to_string();

    let v2: serde_json::Value = put(
        &client,
        addr,
        &users.contributor_jwt,
        &format!("/repositories/{repository_id}/wiki/pages/Home"),
        &json!({ "content": "# v2", "baseSha": v1_sha, "message": "second" }),
    )
    .await
    .json()
    .await
    .unwrap();
    let v2_sha = v2["headSha"].as_str().unwrap().to_string();
    assert_ne!(v1_sha, v2_sha);

    let revisions_resp = get(
        &client,
        addr,
        &users.reader_jwt,
        &format!("/repositories/{repository_id}/wiki/pages/Home/revisions"),
    )
    .await;
    assert_eq!(revisions_resp.status(), 200);
    let revisions: serde_json::Value = revisions_resp.json().await.unwrap();
    let revisions = revisions.as_array().unwrap();
    assert_eq!(
        revisions.len(),
        2,
        "two saves must produce two revisions, got: {revisions:#?}"
    );
    assert_eq!(
        revisions[0]["commitSha"], v2_sha,
        "most recent revision must come first"
    );
    assert_eq!(revisions[0]["message"], "second");
    assert_eq!(revisions[1]["commitSha"], v1_sha);
    assert_eq!(revisions[1]["message"], "first");

    let v1_content: serde_json::Value = get_json(
        &client,
        addr,
        &users.reader_jwt,
        &format!("/repositories/{repository_id}/wiki/pages/Home/revisions/{v1_sha}"),
    )
    .await;
    assert_eq!(
        v1_content["content"], "# v1",
        "the first revision's content endpoint must return the historical text, not current HEAD"
    );

    let v2_content: serde_json::Value = get_json(
        &client,
        addr,
        &users.reader_jwt,
        &format!("/repositories/{repository_id}/wiki/pages/Home/revisions/{v2_sha}"),
    )
    .await;
    assert_eq!(v2_content["content"], "# v2");

    let bogus_resp = get(&client, addr, &users.reader_jwt, &format!("/repositories/{repository_id}/wiki/pages/Home/revisions/0000000000000000000000000000000000000000")).await;
    assert_eq!(bogus_resp.status(), 404);

    // 404 rather than 403/401, so the repository's existence isn't revealed.
    let stranger_jwt = {
        create_user(&client, addr, &users.owner_jwt, "stranger").await;
        login(&client, addr, "stranger", "password12345").await
    };
    let stranger_resp = get(
        &client,
        addr,
        &stranger_jwt,
        &format!("/repositories/{repository_id}/wiki/pages/Home/revisions"),
    )
    .await;
    assert_eq!(stranger_resp.status(), 404);
}

#[sqlx::test]
async fn a_git_push_to_a_nonexistent_wiki_lazily_creates_it_and_the_page_becomes_visible_via_the_api(
    pool: PgPool,
) {
    let (addr, repository_id, repo_path_segment, users) =
        seed_server_with_repo_and_roles(pool).await;

    let clone_url = format!(
        "http://{}:{}@{addr}/{repo_path_segment}.wiki.git",
        users.maintainer_username, users.maintainer_token
    );
    assert!(
        push_files(
            &clone_url,
            "main",
            "seed",
            &[("Home.md", "# Pushed from git".as_bytes())]
        )
        .await,
        "git push to the wiki repo must succeed and lazily create it"
    );

    let client = reqwest::Client::new();
    let list_resp: serde_json::Value = get_json(
        &client,
        addr,
        &users.maintainer_jwt,
        &format!("/repositories/{repository_id}/wiki"),
    )
    .await;
    assert_eq!(list_resp["pages"][0]["slug"], "Home");
}

// A wiki is only created on write: reading an untouched one is a 404.
#[sqlx::test]
async fn a_git_clone_of_a_nonexistent_wiki_404s(pool: PgPool) {
    let (addr, _repository_id, repo_path_segment, users) =
        seed_server_with_repo_and_roles(pool).await;
    let url = format!(
        "http://{}:{}@{addr}/{repo_path_segment}.wiki.git/info/refs?service=git-upload-pack",
        users.maintainer_username, users.maintainer_token
    );
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), 404);
}

/// A wiki push used to create a pipeline from the real repository's HEAD.
#[sqlx::test]
async fn a_git_push_to_a_wiki_does_not_trigger_pipeline_creation_for_the_real_repository(
    pool: PgPool,
) {
    let (addr, repository_id, repo_path_segment, users) =
        seed_server_with_repo_and_roles(pool).await;
    let client = reqwest::Client::new();

    let repo_clone_url = format!(
        "http://{}:{}@{addr}/{repo_path_segment}.git",
        users.maintainer_username, users.maintainer_token
    );
    assert!(
        push_files(
            &repo_clone_url,
            "main",
            "add ci file",
            &[(".ferrisgit-ci.yml", "stages: [build]\njobs:\n  hello:\n    stage: build\n    image: alpine:3.20\n    script:\n      - echo hi\n".as_bytes())]
        )
        .await,
        "the push adding the CI file to the real repo must succeed"
    );

    // The pipeline is created before the git response goes out, so no polling.
    let pipelines_after_real_push: serde_json::Value = get_json(
        &client,
        addr,
        &users.maintainer_jwt,
        &format!("/repositories/{repository_id}/pipelines"),
    )
    .await;
    let pipelines_after_real_push = pipelines_after_real_push.as_array().unwrap();
    assert_eq!(
        pipelines_after_real_push.len(),
        1,
        "the push to the real repo (with a CI file at HEAD) must create exactly one pipeline: {pipelines_after_real_push:#?}"
    );
    let pipeline_ids_after_real_push: Vec<String> = pipelines_after_real_push
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect();

    let wiki_clone_url = format!(
        "http://{}:{}@{addr}/{repo_path_segment}.wiki.git",
        users.maintainer_username, users.maintainer_token
    );
    assert!(
        push_files(
            &wiki_clone_url,
            "main",
            "seed",
            &[("Home.md", "# Pushed from git".as_bytes())]
        )
        .await,
        "git push to the wiki repo must succeed"
    );

    let pipelines_after_wiki_push: serde_json::Value = get_json(
        &client,
        addr,
        &users.maintainer_jwt,
        &format!("/repositories/{repository_id}/pipelines"),
    )
    .await;
    let pipelines_after_wiki_push = pipelines_after_wiki_push.as_array().unwrap();
    let pipeline_ids_after_wiki_push: Vec<String> = pipelines_after_wiki_push
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        pipeline_ids_after_wiki_push, pipeline_ids_after_real_push,
        "a push to the wiki must not create any new pipeline for the real repository"
    );
}

/// A first push to a branch other than main used to leave the wiki's HEAD dangling, so the wiki looked empty.
#[sqlx::test]
async fn a_git_push_to_a_non_main_branch_of_a_brand_new_wiki_still_becomes_visible_via_the_api(
    pool: PgPool,
) {
    let (addr, repository_id, repo_path_segment, users) =
        seed_server_with_repo_and_roles(pool).await;

    let clone_url = format!(
        "http://{}:{}@{addr}/{repo_path_segment}.wiki.git",
        users.maintainer_username, users.maintainer_token
    );
    assert!(
        push_files(
            &clone_url,
            "master",
            "seed",
            &[("Home.md", "# Pushed to master".as_bytes())]
        )
        .await,
        "git push to master on the brand-new wiki repo must succeed"
    );

    let list_resp: serde_json::Value = reqwest::Client::new()
        .get(format!(
            "http://{addr}/api/repositories/{repository_id}/wiki"
        ))
        .bearer_auth(&users.maintainer_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        list_resp["headSha"].is_string(),
        "headSha must resolve, not be null, once HEAD is healed: {list_resp:#?}"
    );
    assert_eq!(
        list_resp["pages"][0]["slug"], "Home",
        "the page pushed to master must be visible: {list_resp:#?}"
    );
}
