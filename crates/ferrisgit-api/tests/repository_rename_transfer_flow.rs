mod common;

use common::USER_PASSWORD;
use common::git::{commit_file, git};
use common::http::{
    add_collaborator, create_user, get, get_json, login, mint_api_token, patch, post_json,
};
use serde_json::{Value, json};
use sqlx::PgPool;

#[sqlx::test]
async fn a_renamed_then_transferred_repository_keeps_answering_at_its_old_paths(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
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
    let repo_path = format!("/repositories/by-id/{repo_id}");

    let parent = tempfile::tempdir().unwrap();
    let git_url = |path: &str| format!("http://admin:{token}@{addr}/{path}.git");
    git(
        &["clone", "-q", &git_url("admin/hello"), "work"],
        parent.path(),
    )
    .await;
    let work = parent.path().join("work");
    commit_file(&work, "README.md", "hello\n", "init").await;
    git(&["push", "-q", "origin", "HEAD:main"], &work).await;
    let wiki = parent.path().join("wiki");
    std::fs::create_dir(&wiki).unwrap();
    git(&["init", "-q"], &wiki).await;
    commit_file(&wiki, "Home.md", "# Home\n", "wiki").await;
    git(
        &["push", "-q", &git_url("admin/hello.wiki"), "HEAD:main"],
        &wiki,
    )
    .await;

    // Rename.
    let res = patch(&client, addr, &jwt, &repo_path, &json!({ "name": "world" })).await;
    assert_eq!(res.status(), 200);
    let renamed: Value = res.json().await.unwrap();
    assert_eq!(renamed["name"], "world");
    assert_eq!(renamed["path"], json!(["admin", "world"]));

    // The old path keeps working: the API, the resolver and Git (clone and push).
    let by_old_path = get_json(&client, addr, &jwt, "/repositories/admin/hello").await;
    assert_eq!(by_old_path["id"], repo_id.as_str());
    assert_eq!(by_old_path["path"], json!(["admin", "world"]));
    let resolved = get_json(&client, addr, &jwt, "/resolve/admin/hello").await;
    assert_eq!(resolved["repositoryId"], repo_id.as_str());
    assert_eq!(resolved["path"], json!(["admin", "world"]));
    commit_file(&work, "README.md", "hello again\n", "second").await;
    git(&["push", "-q", "origin", "HEAD:main"], &work).await;
    git(
        &["clone", "-q", &git_url("admin/world"), "renamed"],
        parent.path(),
    )
    .await;
    assert_eq!(
        std::fs::read_to_string(parent.path().join("renamed/README.md")).unwrap(),
        "hello again\n"
    );
    git(
        &["clone", "-q", &git_url("admin/world.wiki"), "renamed-wiki"],
        parent.path(),
    )
    .await;
    assert!(
        parent.path().join("renamed-wiki/Home.md").exists(),
        "the wiki moved along"
    );

    // Transfer into a group the caller maintains.
    post_json(
        &client,
        addr,
        &jwt,
        "/groups",
        &json!({ "name": "acme", "description": "" }),
    )
    .await;
    let res = patch(
        &client,
        addr,
        &jwt,
        &repo_path,
        &json!({ "namespace": "acme" }),
    )
    .await;
    assert_eq!(res.status(), 200);
    let moved: Value = res.json().await.unwrap();
    assert_eq!(moved["path"], json!(["acme", "world"]));
    for old in ["admin/hello", "admin/world"] {
        let resolved = get_json(&client, addr, &jwt, &format!("/resolve/{old}")).await;
        assert_eq!(resolved["type"], "groupRepository", "{old}");
        assert_eq!(resolved["path"], json!(["acme", "world"]), "{old}");
    }
    git(
        &["clone", "-q", &git_url("admin/hello"), "via-oldest"],
        parent.path(),
    )
    .await;
    git(
        &["clone", "-q", &git_url("acme/world"), "via-new"],
        parent.path(),
    )
    .await;

    // A new repository can take the old name: it shadows the redirect.
    let fresh: Value = post_json(
        &client,
        addr,
        &jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    let resolved = get_json(&client, addr, &jwt, "/resolve/admin/hello").await;
    assert_eq!(resolved["repositoryId"], fresh["id"]);
    let resolved = get_json(&client, addr, &jwt, "/resolve/admin/world").await;
    assert_eq!(resolved["repositoryId"], repo_id.as_str());

    // Back to the personal namespace, under a taken name: refused, nothing moves.
    let res = patch(
        &client,
        addr,
        &jwt,
        &repo_path,
        &json!({ "namespace": "admin", "name": "hello" }),
    )
    .await;
    assert_eq!(res.status(), 409);
    let still = get_json(&client, addr, &jwt, &repo_path).await;
    assert_eq!(still["path"], json!(["acme", "world"]));
}

#[sqlx::test]
async fn only_a_maintainer_moves_a_repository_and_only_to_a_namespace_of_their_own(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let jwt = login(&client, addr, "admin", "adminpassword123").await;
    let repo: Value = post_json(
        &client,
        addr,
        &jwt,
        "/repositories",
        &json!({ "name": "hello", "visibility": "private" }),
    )
    .await;
    let repo_id = repo["id"].as_str().unwrap();
    let repo_path = format!("/repositories/by-id/{repo_id}");
    create_user(&client, addr, &jwt, "carol").await;
    add_collaborator(&client, addr, &jwt, repo_id, "carol", "contributor").await;
    let carol = login(&client, addr, "carol", USER_PASSWORD).await;

    let res = patch(
        &client,
        addr,
        &carol,
        &repo_path,
        &json!({ "name": "mine" }),
    )
    .await;
    assert_eq!(res.status(), 404, "a contributor cannot rename");

    // Not someone else's namespace, nor a group the caller doesn't maintain, nor an unknown one.
    post_json(
        &client,
        addr,
        &carol,
        "/groups",
        &json!({ "name": "carolco", "description": "" }),
    )
    .await;
    for namespace in ["carol", "carolco", "nowhere", ""] {
        let res = patch(
            &client,
            addr,
            &jwt,
            &repo_path,
            &json!({ "namespace": namespace }),
        )
        .await;
        assert_eq!(res.status(), 400, "{namespace:?}");
    }
    let res = patch(
        &client,
        addr,
        &jwt,
        &repo_path,
        &json!({ "name": "bad name" }),
    )
    .await;
    assert_eq!(res.status(), 400);

    let unchanged = get(&client, addr, &jwt, &repo_path).await;
    let unchanged: Value = unchanged.json().await.unwrap();
    assert_eq!(unchanged["path"], json!(["admin", "hello"]));
}
