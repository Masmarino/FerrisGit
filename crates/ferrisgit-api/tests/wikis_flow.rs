use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use serde_json::json;
use sqlx::PgPool;
use std::net::SocketAddr;
use std::process::Command;

struct Roles {
    owner_jwt: String,
    maintainer_jwt: String,
    maintainer_username: String,
    maintainer_token: String,
    contributor_jwt: String,
    reader_jwt: String,
}

// The storage and static directories are leaked on purpose: they must outlive the spawned `axum::serve` task.
async fn spawn_server(pool: PgPool) -> SocketAddr {
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

    let storage_dir = tempfile::tempdir().unwrap().keep();
    let static_dir = tempfile::tempdir().unwrap().keep();
    std::fs::write(static_dir.join("index.html"), "<html></html>").unwrap();

    let config = Config {
        database_url: String::new(),
        jwt_secret: "test-secret-that-is-at-least-32-characters-long".to_string(),
        storage_root: storage_dir.to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:0".to_string(),
        static_dir: static_dir.to_string_lossy().to_string(),
        bootstrap_admin_username: Some("admin".to_string()),
        bootstrap_admin_password: Some("adminpassword123".to_string()),
        settings_encryption_key: [b'k'; 32],
        public_url: "http://localhost:4200".to_string(),
        trusted_proxy_cidrs: vec![],
    };

    let mut state = AppState::new(pool, config.clone()).await;
    state.mfa_enforced = false; // these tests are not about MFA: they log in with a plain session
    BootstrapAdminUseCase::new(state.users.clone(), state.hasher.clone())
        .execute(
            config.bootstrap_admin_username.clone(),
            config.bootstrap_admin_password.clone(),
        )
        .await
        .unwrap();

    let app = build_router(state, &static_dir);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

async fn login(
    client: &reqwest::Client,
    addr: SocketAddr,
    username: &str,
    password: &str,
) -> String {
    let res: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    res["token"].as_str().unwrap().to_string()
}

async fn create_user(client: &reqwest::Client, addr: SocketAddr, owner_jwt: &str, username: &str) {
    client
        .post(format!("http://{addr}/api/admin/users"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "username": username, "email": format!("{username}@example.com"), "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
}

async fn add_collaborator(
    client: &reqwest::Client,
    addr: SocketAddr,
    owner_jwt: &str,
    repo_id: &str,
    username: &str,
    role: &str,
) {
    let status = client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/collaborators"
        ))
        .bearer_auth(owner_jwt)
        .json(&json!({ "username": username, "role": role }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        status, 204,
        "expected adding {username} as {role} to return 204"
    );
}

async fn mint_api_token(
    client: &reqwest::Client,
    addr: SocketAddr,
    jwt: &str,
    name: &str,
) -> String {
    let res: serde_json::Value = client
        .post(format!("http://{addr}/api/tokens"))
        .bearer_auth(jwt)
        .json(&json!({ "name": name }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    res["token"].as_str().unwrap().to_string()
}

async fn seed_server_with_repo_and_roles(pool: PgPool) -> (SocketAddr, String, String, Roles) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();

    let owner_jwt = login(&client, addr, "admin", "adminpassword123").await;

    let repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "name": "hello", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
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

    let create_resp = client
        .put(format!(
            "http://{addr}/api/repositories/{repository_id}/wiki/pages/Home"
        ))
        .bearer_auth(&users.contributor_jwt)
        .json(&json!({ "content": "# Home", "baseSha": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_resp.status(), 200);
    let created: serde_json::Value = create_resp.json().await.unwrap();
    let head_sha = created["headSha"].as_str().unwrap().to_string();

    let edit_resp = client
        .put(format!(
            "http://{addr}/api/repositories/{repository_id}/wiki/pages/Home"
        ))
        .bearer_auth(&users.contributor_jwt)
        .json(&json!({ "content": "# Home v2", "baseSha": head_sha }))
        .send()
        .await
        .unwrap();
    assert_eq!(edit_resp.status(), 200);

    let latest: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repository_id}/wiki/pages/Home"
        ))
        .bearer_auth(&users.contributor_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let delete_resp = client
        .delete(format!(
            "http://{addr}/api/repositories/{repository_id}/wiki/pages/Home?baseSha={}",
            latest["headSha"].as_str().unwrap()
        ))
        .bearer_auth(&users.contributor_jwt)
        .send()
        .await
        .unwrap();
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

    let create_resp = client
        .put(format!(
            "http://{addr}/api/repositories/{repository_id}/wiki/pages/Home"
        ))
        .bearer_auth(&users.maintainer_jwt)
        .json(&json!({ "content": "# Home", "baseSha": null }))
        .send()
        .await
        .unwrap();
    let created: serde_json::Value = create_resp.json().await.unwrap();

    let read_resp = client
        .get(format!(
            "http://{addr}/api/repositories/{repository_id}/wiki/pages/Home"
        ))
        .bearer_auth(&users.reader_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(read_resp.status(), 200);

    let write_resp = client
        .put(format!(
            "http://{addr}/api/repositories/{repository_id}/wiki/pages/About"
        ))
        .bearer_auth(&users.reader_jwt)
        .json(&json!({ "content": "# About", "baseSha": created["headSha"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(write_resp.status(), 404);

    let delete_resp = client
        .delete(format!(
            "http://{addr}/api/repositories/{repository_id}/wiki/pages/Home?baseSha={}",
            created["headSha"].as_str().unwrap()
        ))
        .bearer_auth(&users.maintainer_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(delete_resp.status(), 204);
}

#[sqlx::test]
async fn a_stale_base_sha_is_rejected_but_a_retry_with_the_fresh_sha_succeeds(pool: PgPool) {
    let (addr, repository_id, _path, users) = seed_server_with_repo_and_roles(pool).await;
    let client = reqwest::Client::new();

    let create_resp = client
        .put(format!(
            "http://{addr}/api/repositories/{repository_id}/wiki/pages/Home"
        ))
        .bearer_auth(&users.maintainer_jwt)
        .json(&json!({ "content": "# v1", "baseSha": null }))
        .send()
        .await
        .unwrap();
    let created: serde_json::Value = create_resp.json().await.unwrap();
    let stale_sha = created["headSha"].as_str().unwrap().to_string();

    client
        .put(format!(
            "http://{addr}/api/repositories/{repository_id}/wiki/pages/Home"
        ))
        .bearer_auth(&users.maintainer_jwt)
        .json(&json!({ "content": "# v2", "baseSha": stale_sha }))
        .send()
        .await
        .unwrap();

    let conflict_resp = client
        .put(format!(
            "http://{addr}/api/repositories/{repository_id}/wiki/pages/Home"
        ))
        .bearer_auth(&users.maintainer_jwt)
        .json(&json!({ "content": "# v3 (stale)", "baseSha": stale_sha }))
        .send()
        .await
        .unwrap();
    assert_eq!(conflict_resp.status(), 409);

    let fresh: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repository_id}/wiki/pages/Home"
        ))
        .bearer_auth(&users.maintainer_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let retry_resp = client
        .put(format!(
            "http://{addr}/api/repositories/{repository_id}/wiki/pages/Home"
        ))
        .bearer_auth(&users.maintainer_jwt)
        .json(&json!({ "content": "# v3", "baseSha": fresh["headSha"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(retry_resp.status(), 200);
}

#[sqlx::test]
async fn revisions_list_history_most_recent_first_and_revision_content_returns_historical_text(
    pool: PgPool,
) {
    let (addr, repository_id, _path, users) = seed_server_with_repo_and_roles(pool).await;
    let client = reqwest::Client::new();

    let v1: serde_json::Value = client
        .put(format!(
            "http://{addr}/api/repositories/{repository_id}/wiki/pages/Home"
        ))
        .bearer_auth(&users.contributor_jwt)
        .json(&json!({ "content": "# v1", "baseSha": null, "message": "first" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let v1_sha = v1["headSha"].as_str().unwrap().to_string();

    let v2: serde_json::Value = client
        .put(format!(
            "http://{addr}/api/repositories/{repository_id}/wiki/pages/Home"
        ))
        .bearer_auth(&users.contributor_jwt)
        .json(&json!({ "content": "# v2", "baseSha": v1_sha, "message": "second" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let v2_sha = v2["headSha"].as_str().unwrap().to_string();
    assert_ne!(v1_sha, v2_sha);

    let revisions_resp = client
        .get(format!(
            "http://{addr}/api/repositories/{repository_id}/wiki/pages/Home/revisions"
        ))
        .bearer_auth(&users.reader_jwt)
        .send()
        .await
        .unwrap();
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

    let v1_content: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repository_id}/wiki/pages/Home/revisions/{v1_sha}"
        ))
        .bearer_auth(&users.reader_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        v1_content["content"], "# v1",
        "the first revision's content endpoint must return the historical text, not current HEAD"
    );

    let v2_content: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repository_id}/wiki/pages/Home/revisions/{v2_sha}"
        ))
        .bearer_auth(&users.reader_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(v2_content["content"], "# v2");

    let bogus_resp = client
        .get(format!("http://{addr}/api/repositories/{repository_id}/wiki/pages/Home/revisions/0000000000000000000000000000000000000000"))
        .bearer_auth(&users.reader_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(bogus_resp.status(), 404);

    // 404, not 403/401: never reveal the repository's existence.
    let stranger_jwt = {
        create_user(&client, addr, &users.owner_jwt, "stranger").await;
        login(&client, addr, "stranger", "password12345").await
    };
    let stranger_resp = client
        .get(format!(
            "http://{addr}/api/repositories/{repository_id}/wiki/pages/Home/revisions"
        ))
        .bearer_auth(&stranger_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(stranger_resp.status(), 404);
}

#[sqlx::test]
async fn a_git_push_to_a_nonexistent_wiki_lazily_creates_it_and_the_page_becomes_visible_via_the_api(
    pool: PgPool,
) {
    let (addr, repository_id, repo_path_segment, users) =
        seed_server_with_repo_and_roles(pool).await;
    let work_dir = tempfile::tempdir().unwrap();

    // git subprocesses run in `spawn_blocking`: a blocking `Command` call deadlocks the single-threaded `#[sqlx::test]` runtime against the in-process `axum::serve` task.
    let work_dir_path = work_dir.path().to_path_buf();
    let clone_url = format!(
        "http://{}:{}@{addr}/{repo_path_segment}.wiki.git",
        users.maintainer_username, users.maintainer_token
    );
    tokio::task::spawn_blocking({
        let work_dir_path = work_dir_path.clone();
        move || {
            Command::new("git")
                .args(["init"])
                .current_dir(&work_dir_path)
                .status()
                .unwrap()
        }
    })
    .await
    .unwrap();
    tokio::fs::write(work_dir_path.join("Home.md"), "# Pushed from git")
        .await
        .unwrap();
    let push_status = tokio::task::spawn_blocking({
        let work_dir_path = work_dir_path.clone();
        let clone_url = clone_url.clone();
        move || {
            Command::new("git")
                .args(["add", "."])
                .current_dir(&work_dir_path)
                .status()
                .unwrap();
            Command::new("git")
                .args([
                    "-c",
                    "user.email=a@b.c",
                    "-c",
                    "user.name=A",
                    "commit",
                    "-q",
                    "-m",
                    "seed",
                ])
                .current_dir(&work_dir_path)
                .status()
                .unwrap();
            Command::new("git")
                .args(["remote", "add", "origin", &clone_url])
                .current_dir(&work_dir_path)
                .status()
                .unwrap();
            Command::new("git")
                .args(["push", "-q", "origin", "HEAD:main"])
                .current_dir(&work_dir_path)
                .status()
                .unwrap()
        }
    })
    .await
    .unwrap();
    assert!(
        push_status.success(),
        "git push to the wiki repo must succeed and lazily create it"
    );

    let client = reqwest::Client::new();
    let list_resp: serde_json::Value = client
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
    assert_eq!(list_resp["pages"][0]["slug"], "Home");
}

// Lazy creation is write-path only: a read of a never-touched wiki 404s.
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

/// Regression: a wiki push must not create a pipeline from the real repository's HEAD.
#[sqlx::test]
async fn a_git_push_to_a_wiki_does_not_trigger_pipeline_creation_for_the_real_repository(
    pool: PgPool,
) {
    let (addr, repository_id, repo_path_segment, users) =
        seed_server_with_repo_and_roles(pool).await;
    let client = reqwest::Client::new();

    let repo_work_dir = tempfile::tempdir().unwrap();
    let repo_work_dir_path = repo_work_dir.path().to_path_buf();
    let repo_clone_url = format!(
        "http://{}:{}@{addr}/{repo_path_segment}.git",
        users.maintainer_username, users.maintainer_token
    );
    tokio::task::spawn_blocking({
        let repo_work_dir_path = repo_work_dir_path.clone();
        move || {
            Command::new("git")
                .args(["init"])
                .current_dir(&repo_work_dir_path)
                .status()
                .unwrap()
        }
    })
    .await
    .unwrap();
    tokio::fs::write(
        repo_work_dir_path.join(".ferrisgit-ci.yml"),
        "stages: [build]\njobs:\n  hello:\n    stage: build\n    image: alpine:3.20\n    script:\n      - echo hi\n",
    )
    .await
    .unwrap();
    let repo_push_status = tokio::task::spawn_blocking({
        let repo_work_dir_path = repo_work_dir_path.clone();
        let repo_clone_url = repo_clone_url.clone();
        move || {
            Command::new("git")
                .args(["add", "."])
                .current_dir(&repo_work_dir_path)
                .status()
                .unwrap();
            Command::new("git")
                .args([
                    "-c",
                    "user.email=a@b.c",
                    "-c",
                    "user.name=A",
                    "commit",
                    "-q",
                    "-m",
                    "add ci file",
                ])
                .current_dir(&repo_work_dir_path)
                .status()
                .unwrap();
            Command::new("git")
                .args(["remote", "add", "origin", &repo_clone_url])
                .current_dir(&repo_work_dir_path)
                .status()
                .unwrap();
            Command::new("git")
                .args(["push", "-q", "origin", "HEAD:main"])
                .current_dir(&repo_work_dir_path)
                .status()
                .unwrap()
        }
    })
    .await
    .unwrap();
    assert!(
        repo_push_status.success(),
        "the push adding the CI file to the real repo must succeed"
    );

    // The pipeline is created before the git response is built, so no polling is needed.
    let pipelines_after_real_push: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repository_id}/pipelines"
        ))
        .bearer_auth(&users.maintainer_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
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

    let wiki_work_dir = tempfile::tempdir().unwrap();
    let wiki_work_dir_path = wiki_work_dir.path().to_path_buf();
    let wiki_clone_url = format!(
        "http://{}:{}@{addr}/{repo_path_segment}.wiki.git",
        users.maintainer_username, users.maintainer_token
    );
    tokio::task::spawn_blocking({
        let wiki_work_dir_path = wiki_work_dir_path.clone();
        move || {
            Command::new("git")
                .args(["init"])
                .current_dir(&wiki_work_dir_path)
                .status()
                .unwrap()
        }
    })
    .await
    .unwrap();
    tokio::fs::write(wiki_work_dir_path.join("Home.md"), "# Pushed from git")
        .await
        .unwrap();
    let wiki_push_status = tokio::task::spawn_blocking({
        let wiki_work_dir_path = wiki_work_dir_path.clone();
        let wiki_clone_url = wiki_clone_url.clone();
        move || {
            Command::new("git")
                .args(["add", "."])
                .current_dir(&wiki_work_dir_path)
                .status()
                .unwrap();
            Command::new("git")
                .args([
                    "-c",
                    "user.email=a@b.c",
                    "-c",
                    "user.name=A",
                    "commit",
                    "-q",
                    "-m",
                    "seed",
                ])
                .current_dir(&wiki_work_dir_path)
                .status()
                .unwrap();
            Command::new("git")
                .args(["remote", "add", "origin", &wiki_clone_url])
                .current_dir(&wiki_work_dir_path)
                .status()
                .unwrap();
            Command::new("git")
                .args(["push", "-q", "origin", "HEAD:main"])
                .current_dir(&wiki_work_dir_path)
                .status()
                .unwrap()
        }
    })
    .await
    .unwrap();
    assert!(
        wiki_push_status.success(),
        "git push to the wiki repo must succeed"
    );

    let pipelines_after_wiki_push: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repository_id}/pipelines"
        ))
        .bearer_auth(&users.maintainer_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
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

/// Regression: a first push to a non-`main` branch left the wiki's pinned HEAD dangling, so the wiki looked empty.
#[sqlx::test]
async fn a_git_push_to_a_non_main_branch_of_a_brand_new_wiki_still_becomes_visible_via_the_api(
    pool: PgPool,
) {
    let (addr, repository_id, repo_path_segment, users) =
        seed_server_with_repo_and_roles(pool).await;
    let work_dir = tempfile::tempdir().unwrap();

    let work_dir_path = work_dir.path().to_path_buf();
    let clone_url = format!(
        "http://{}:{}@{addr}/{repo_path_segment}.wiki.git",
        users.maintainer_username, users.maintainer_token
    );
    tokio::task::spawn_blocking({
        let work_dir_path = work_dir_path.clone();
        move || {
            Command::new("git")
                .args(["init"])
                .current_dir(&work_dir_path)
                .status()
                .unwrap()
        }
    })
    .await
    .unwrap();
    tokio::fs::write(work_dir_path.join("Home.md"), "# Pushed to master")
        .await
        .unwrap();
    let push_status = tokio::task::spawn_blocking({
        let work_dir_path = work_dir_path.clone();
        let clone_url = clone_url.clone();
        move || {
            Command::new("git")
                .args(["add", "."])
                .current_dir(&work_dir_path)
                .status()
                .unwrap();
            Command::new("git")
                .args([
                    "-c",
                    "user.email=a@b.c",
                    "-c",
                    "user.name=A",
                    "commit",
                    "-q",
                    "-m",
                    "seed",
                ])
                .current_dir(&work_dir_path)
                .status()
                .unwrap();
            Command::new("git")
                .args(["remote", "add", "origin", &clone_url])
                .current_dir(&work_dir_path)
                .status()
                .unwrap();
            // Not `main` on purpose, so the wiki's first write lands on a differently named branch.
            Command::new("git")
                .args(["push", "-q", "origin", "HEAD:master"])
                .current_dir(&work_dir_path)
                .status()
                .unwrap()
        }
    })
    .await
    .unwrap();
    assert!(
        push_status.success(),
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
