use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::path::Path;
use std::process::Command;

async fn git(args: &[&str], cwd: &Path) -> String {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let cwd = cwd.to_path_buf();
    let output = tokio::task::spawn_blocking(move || {
        Command::new("git").args(&args).current_dir(&cwd).output()
    })
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

async fn commit_file(repo_path: &Path, file: &str, content: &str, message: &str) {
    std::fs::write(repo_path.join(file), content).unwrap();
    git(&["add", "."], repo_path).await;
    git(
        &[
            "-c",
            "user.email=t@t.com",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            message,
        ],
        repo_path,
    )
    .await;
}

struct Api {
    client: reqwest::Client,
    addr: std::net::SocketAddr,
}

impl Api {
    async fn login(&self, username: &str, password: &str) -> String {
        let res: Value = self
            .client
            .post(format!("http://{}/api/auth/login", self.addr))
            .json(&json!({ "username": username, "password": password }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        res["token"].as_str().unwrap().to_string()
    }

    async fn post(&self, jwt: &str, path: &str, body: Value) -> Value {
        let res = self
            .client
            .post(format!("http://{}/api{path}", self.addr))
            .bearer_auth(jwt)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert!(
            res.status().is_success(),
            "POST {path} failed with {}",
            res.status()
        );
        res.json().await.unwrap_or(Value::Null)
    }

    async fn get(&self, jwt: &str, path: &str) -> Value {
        self.client
            .get(format!("http://{}/api{path}", self.addr))
            .bearer_auth(jwt)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    async fn patch(&self, jwt: &str, path: &str, body: Value) -> Value {
        let res = self
            .client
            .patch(format!("http://{}/api{path}", self.addr))
            .bearer_auth(jwt)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert!(
            res.status().is_success(),
            "PATCH {path} failed with {}",
            res.status()
        );
        res.json().await.unwrap()
    }
}

/// The temp dirs must outlive the test.
async fn spawn_app(pool: PgPool) -> (Api, Vec<tempfile::TempDir>) {
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

    let storage_dir = tempfile::tempdir().unwrap();
    let static_dir = tempfile::tempdir().unwrap();
    std::fs::write(static_dir.path().join("index.html"), "<html></html>").unwrap();

    let config = Config {
        database_url: String::new(),
        jwt_secret: "test-secret-that-is-at-least-32-characters-long".to_string(),
        storage_root: storage_dir.path().to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:0".to_string(),
        static_dir: static_dir.path().to_string_lossy().to_string(),
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

    let app = build_router(state, static_dir.path());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (
        Api {
            client: reqwest::Client::new(),
            addr,
        },
        vec![storage_dir, static_dir],
    )
}

fn assert_user_ref(value: &Value, username: &str, context: &str) {
    assert_eq!(
        value["username"], username,
        "{context}: wrong username in {value:#}"
    );
    assert!(
        value["id"].is_string(),
        "{context}: a user reference carries the user id, got {value:#}"
    );
}

#[sqlx::test]
async fn issues_expose_author_assignee_and_comment_count(pool: PgPool) {
    let (api, _dirs) = spawn_app(pool).await;
    let jwt = api.login("admin", "adminpassword123").await;
    let jwt = jwt.as_str();

    let repo_id = api
        .post(
            jwt,
            "/repositories",
            json!({ "name": "hello", "visibility": "private" }),
        )
        .await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let contributor = api.post(jwt, "/admin/users", json!({ "username": "contributor", "email": "contributor@example.com", "password": "password12345" })).await;
    let contributor_id = contributor["id"].as_str().unwrap().to_string();
    api.post(
        jwt,
        &format!("/repositories/{repo_id}/collaborators"),
        json!({ "username": "contributor", "role": "contributor" }),
    )
    .await;
    let contributor_jwt = api.login("contributor", "password12345").await;
    let contributor_jwt = contributor_jwt.as_str();

    let created = api
        .post(
            jwt,
            &format!("/repositories/{repo_id}/issues"),
            json!({ "title": "Bug in login", "description": "It fails", "kind": "bug" }),
        )
        .await;
    assert_user_ref(&created["author"], "admin", "create");
    assert!(
        created["assignee"].is_null(),
        "an unassigned issue has assignee: null, got {created:#}"
    );
    assert_eq!(created["commentCount"], 0);
    assert_eq!(created["number"], 1);
    assert_eq!(created["title"], "Bug in login");
    assert_eq!(created["status"], "todo");
    assert_eq!(created["authorId"], created["author"]["id"]);
    assert!(created["assigneeId"].is_null());

    let assigned = api
        .post(
            jwt,
            &format!("/repositories/{repo_id}/issues/1/assign"),
            json!({ "assigneeId": contributor_id }),
        )
        .await;
    assert_user_ref(&assigned["assignee"], "contributor", "assign");
    assert_user_ref(&assigned["author"], "admin", "assign");
    assert_eq!(assigned["assigneeId"], contributor_id.as_str());

    let list = api
        .get(jwt, &format!("/repositories/{repo_id}/issues"))
        .await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_user_ref(&list[0]["author"], "admin", "list");
    assert_user_ref(&list[0]["assignee"], "contributor", "list");
    assert_eq!(list[0]["commentCount"], 0);
    let detail = api
        .get(jwt, &format!("/repositories/{repo_id}/issues/1"))
        .await;
    assert_user_ref(&detail["author"], "admin", "detail");
    assert_user_ref(&detail["assignee"], "contributor", "detail");
    assert_eq!(detail["commentCount"], 0);

    api.post(
        jwt,
        &format!("/repositories/{repo_id}/issues"),
        json!({ "title": "Second", "description": "", "kind": "task" }),
    )
    .await;
    let list = api
        .get(jwt, &format!("/repositories/{repo_id}/issues"))
        .await;
    let second = list
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["number"] == 2)
        .unwrap();
    assert!(second["assignee"].is_null());
    assert_eq!(second["commentCount"], 0);

    let first = api
        .post(
            jwt,
            &format!("/repositories/{repo_id}/issues/1/comments"),
            json!({ "body": "looking into it" }),
        )
        .await;
    assert_user_ref(&first["author"], "admin", "POST comment");
    assert_eq!(first["body"], "looking into it");
    assert!(first["authorId"].is_string());
    let second_comment = api
        .post(
            contributor_jwt,
            &format!("/repositories/{repo_id}/issues/1/comments"),
            json!({ "body": "me too" }),
        )
        .await;
    assert_user_ref(&second_comment["author"], "contributor", "POST comment");

    let comments = api
        .get(jwt, &format!("/repositories/{repo_id}/issues/1/comments"))
        .await;
    let comments = comments.as_array().unwrap();
    assert_eq!(comments.len(), 2);
    assert_user_ref(&comments[0]["author"], "admin", "GET comments");
    assert_user_ref(&comments[1]["author"], "contributor", "GET comments");

    let list = api
        .get(jwt, &format!("/repositories/{repo_id}/issues"))
        .await;
    let first_in_list = list
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["number"] == 1)
        .unwrap();
    assert_eq!(first_in_list["commentCount"], 2);
    let second_in_list = list
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["number"] == 2)
        .unwrap();
    assert_eq!(second_in_list["commentCount"], 0, "counts are per issue");
    let detail = api
        .get(jwt, &format!("/repositories/{repo_id}/issues/1"))
        .await;
    assert_eq!(detail["commentCount"], 2);

    let moved = api
        .patch(
            jwt,
            &format!("/repositories/{repo_id}/issues/1/status"),
            json!({ "status": "in_progress" }),
        )
        .await;
    assert_eq!(moved["status"], "in_progress");
    assert_user_ref(&moved["author"], "admin", "status");
    assert_user_ref(&moved["assignee"], "contributor", "status");
    assert_eq!(moved["commentCount"], 2);
    let updated = api.patch(jwt, &format!("/repositories/{repo_id}/issues/1"), json!({ "title": "Bug in login page", "description": "It fails", "kind": "bug", "milestoneId": null })).await;
    assert_eq!(updated["title"], "Bug in login page");
    assert_user_ref(&updated["author"], "admin", "update");
    assert_user_ref(&updated["assignee"], "contributor", "update");
    assert_eq!(updated["commentCount"], 2);
    let closed = api
        .post(
            jwt,
            &format!("/repositories/{repo_id}/issues/1/close"),
            json!({}),
        )
        .await;
    assert_user_ref(&closed["author"], "admin", "close");
    assert_eq!(closed["commentCount"], 2);
    let reopened = api
        .post(
            jwt,
            &format!("/repositories/{repo_id}/issues/1/reopen"),
            json!({}),
        )
        .await;
    assert_user_ref(&reopened["assignee"], "contributor", "reopen");
    assert_eq!(reopened["commentCount"], 2);
}

#[sqlx::test]
async fn merge_requests_expose_author_and_comment_count_from_every_endpoint(pool: PgPool) {
    let (api, _dirs) = spawn_app(pool).await;
    let addr = api.addr;
    let jwt = api.login("admin", "adminpassword123").await;
    let jwt = jwt.as_str();
    let plain_token = api.post(jwt, "/tokens", json!({ "name": "ci" })).await["token"]
        .as_str()
        .unwrap()
        .to_string();
    let repo_id = api
        .post(
            jwt,
            "/repositories",
            json!({ "name": "hello", "visibility": "private" }),
        )
        .await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let clone_parent = tempfile::tempdir().unwrap();
    let clone_url = format!("http://admin:{plain_token}@{addr}/admin/hello.git");
    let repo_path = clone_parent.path().join("repo");
    git(&["clone", "-q", &clone_url, "repo"], clone_parent.path()).await;
    commit_file(&repo_path, "README.md", "line one\n", "root").await;
    git(&["push", "-q", "origin", "HEAD:main"], &repo_path).await;
    let root_sha = git(&["rev-parse", "HEAD"], &repo_path).await;
    git(&["checkout", "-q", "-b", "feature"], &repo_path).await;
    commit_file(
        &repo_path,
        "README.md",
        "line one\nline two\n",
        "feature work",
    )
    .await;
    git(&["push", "-q", "origin", "HEAD:feature"], &repo_path).await;
    git(&["checkout", "-q", &root_sha], &repo_path).await;
    git(&["checkout", "-q", "-b", "docs"], &repo_path).await;
    commit_file(
        &repo_path,
        "CONTRIBUTING.md",
        "please contribute\n",
        "docs work",
    )
    .await;
    git(&["push", "-q", "origin", "HEAD:docs"], &repo_path).await;

    let created = api
        .post(jwt, &format!("/repositories/{repo_id}/merge-requests"), json!({ "sourceBranch": "feature", "targetBranch": "main", "title": "Add line two", "description": "desc" }))
        .await;
    let mr_id = created["id"].as_str().unwrap().to_string();
    let mr_id = mr_id.as_str();
    assert_user_ref(&created["author"], "admin", "create");
    assert_eq!(created["commentCount"], 0);
    assert_eq!(created["title"], "Add line two");
    assert_eq!(created["sourceBranch"], "feature");
    assert_eq!(created["status"], "open");
    assert!(created["labels"].as_array().unwrap().is_empty());

    let other = api
        .post(jwt, &format!("/repositories/{repo_id}/merge-requests"), json!({ "sourceBranch": "docs", "targetBranch": "main", "title": "Add contributing guide", "description": "" }))
        .await;
    let other_id = other["id"].as_str().unwrap().to_string();

    let list = api
        .get(jwt, &format!("/repositories/{repo_id}/merge-requests"))
        .await;
    assert_eq!(list.as_array().unwrap().len(), 2);
    for mr in list.as_array().unwrap() {
        assert_user_ref(&mr["author"], "admin", "list");
        assert_eq!(mr["commentCount"], 0);
    }
    let detail = api.get(jwt, &format!("/merge-requests/{mr_id}")).await;
    assert_user_ref(&detail["author"], "admin", "detail");
    assert_eq!(detail["commentCount"], 0);

    api.post(
        jwt,
        &format!("/merge-requests/{mr_id}/comments"),
        json!({ "body": "looks promising" }),
    )
    .await;
    let root = api
        .post(
            jwt,
            &format!("/merge-requests/{mr_id}/comments"),
            json!({ "body": "why?", "filePath": "README.md", "lineNumber": 2, "side": "new" }),
        )
        .await;
    api.post(
        jwt,
        &format!("/merge-requests/{mr_id}/comments"),
        json!({ "body": "because", "replyToId": root["id"] }),
    )
    .await;

    let list = api
        .get(jwt, &format!("/repositories/{repo_id}/merge-requests"))
        .await;
    let in_list = list
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == mr_id)
        .unwrap();
    assert_eq!(
        in_list["commentCount"], 3,
        "general + inline + reply, got {in_list:#}"
    );
    assert_user_ref(&in_list["author"], "admin", "list");
    let other_in_list = list
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == other_id.as_str())
        .unwrap();
    assert_eq!(
        other_in_list["commentCount"], 0,
        "counts are per merge request"
    );
    let detail = api.get(jwt, &format!("/merge-requests/{mr_id}")).await;
    assert_eq!(detail["commentCount"], 3);
    assert_user_ref(&detail["author"], "admin", "detail");

    let updated = api
        .patch(
            jwt,
            &format!("/merge-requests/{mr_id}"),
            json!({ "title": "Add the second line", "description": "desc", "milestoneId": null }),
        )
        .await;
    assert_eq!(updated["title"], "Add the second line");
    assert_user_ref(&updated["author"], "admin", "update");
    assert_eq!(updated["commentCount"], 3);

    let merged = api
        .post(jwt, &format!("/merge-requests/{mr_id}/merge"), json!({}))
        .await;
    assert_eq!(merged["outcome"], "merged");
    assert!(merged["mergeCommitSha"].is_string());
    assert_eq!(merged["status"], "merged");
    assert_user_ref(&merged["author"], "admin", "merge");
    assert_eq!(merged["commentCount"], 3);

    let close_status = api
        .client
        .post(format!("http://{addr}/api/merge-requests/{other_id}/close"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(close_status, 204);
    let closed = api.get(jwt, &format!("/merge-requests/{other_id}")).await;
    assert_eq!(closed["status"], "closed");
    assert_user_ref(&closed["author"], "admin", "close then detail");
    assert_eq!(closed["commentCount"], 0);
}
