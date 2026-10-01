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

    async fn post_no_body(&self, jwt: &str, path: &str) -> reqwest::StatusCode {
        self.client
            .post(format!("http://{}/api{path}", self.addr))
            .bearer_auth(jwt)
            .send()
            .await
            .unwrap()
            .status()
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

    async fn get_status(&self, jwt: &str, path: &str) -> reqwest::StatusCode {
        self.client
            .get(format!("http://{}/api{path}", self.addr))
            .bearer_auth(jwt)
            .send()
            .await
            .unwrap()
            .status()
    }

    async fn timeline(&self, jwt: &str, mr_id: &str) -> Value {
        self.get(jwt, &format!("/merge-requests/{mr_id}/timeline"))
            .await
    }
}

fn items_of<'a>(timeline: &'a Value, item_type: &str) -> Vec<&'a Value> {
    timeline["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["type"] == item_type)
        .collect()
}

fn events_of<'a>(timeline: &'a Value, kind: &str) -> Vec<&'a Value> {
    timeline["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["type"] == "event" && i["kind"] == kind)
        .collect()
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

#[sqlx::test]
async fn the_timeline_merges_comments_threads_and_recorded_activity_in_order(pool: PgPool) {
    let (api, _dirs) = spawn_app(pool).await;
    let addr = api.addr;
    let jwt = api.login("admin", "adminpassword123").await;
    let jwt = jwt.as_str();

    let token_res = api.post(jwt, "/tokens", json!({ "name": "ci" })).await;
    let plain_token = token_res["token"].as_str().unwrap().to_string();

    let repo_res = api
        .post(
            jwt,
            "/repositories",
            json!({ "name": "hello", "visibility": "private" }),
        )
        .await;
    let repo_id = repo_res["id"].as_str().unwrap().to_string();

    for (username, email) in [
        ("reviewer", "reviewer@example.com"),
        ("outsider", "outsider@example.com"),
    ] {
        api.post(
            jwt,
            "/admin/users",
            json!({ "username": username, "email": email, "password": "password12345" }),
        )
        .await;
    }
    api.post(
        jwt,
        &format!("/repositories/{repo_id}/collaborators"),
        json!({ "username": "reviewer", "role": "contributor" }),
    )
    .await;
    let reviewer_jwt = api.login("reviewer", "password12345").await;
    let reviewer_jwt = reviewer_jwt.as_str();
    let outsider_jwt = api.login("outsider", "password12345").await;
    let outsider_jwt = outsider_jwt.as_str();

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
    let feature_tip_1 = git(&["rev-parse", "HEAD"], &repo_path).await;

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
    git(&["checkout", "-q", "feature"], &repo_path).await;

    let mr = api
        .post(jwt, &format!("/repositories/{repo_id}/merge-requests"), json!({ "sourceBranch": "feature", "targetBranch": "main", "title": "Add line two", "description": "" }))
        .await;
    let mr_id = mr["id"].as_str().unwrap().to_string();
    let mr_id = mr_id.as_str();

    let timeline = api.timeline(jwt, mr_id).await;
    assert_eq!(timeline["author"]["username"], "admin");
    assert!(timeline["author"]["id"].is_string());
    assert!(
        timeline["items"].as_array().unwrap().is_empty(),
        "a fresh merge request has no activity yet, got: {timeline:#}"
    );

    let label = api
        .post(
            jwt,
            &format!("/repositories/{repo_id}/labels"),
            json!({ "name": "bug", "color": "#dc2626" }),
        )
        .await;
    let label_id = label["id"].as_str().unwrap().to_string();
    let put_labels = api
        .client
        .put(format!("http://{addr}/api/merge-requests/{mr_id}/labels"))
        .bearer_auth(jwt)
        .json(&json!({ "labelIds": [label_id] }))
        .send()
        .await
        .unwrap();
    assert_eq!(put_labels.status(), 200);

    let timeline = api.timeline(jwt, mr_id).await;
    let label_events = events_of(&timeline, "labels_changed");
    assert_eq!(label_events.len(), 1);
    assert_eq!(label_events[0]["actor"]["username"], "admin");
    assert_eq!(label_events[0]["payload"]["added"][0]["name"], "bug");
    assert_eq!(
        label_events[0]["payload"]["added"][0]["id"],
        label_id.as_str()
    );
    assert!(
        label_events[0]["payload"]["removed"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let milestone = api
        .post(
            jwt,
            &format!("/repositories/{repo_id}/milestones"),
            json!({ "title": "v1", "description": "", "dueDate": null }),
        )
        .await;
    let patch = api
        .client
        .patch(format!("http://{addr}/api/merge-requests/{mr_id}"))
        .bearer_auth(jwt)
        .json(&json!({ "title": "Add the second line", "description": "", "milestoneId": milestone["id"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(patch.status(), 200);

    let timeline = api.timeline(jwt, mr_id).await;
    let title_events = events_of(&timeline, "title_changed");
    assert_eq!(title_events.len(), 1);
    assert_eq!(
        title_events[0]["payload"],
        json!({ "from": "Add line two", "to": "Add the second line" })
    );
    let milestone_events = events_of(&timeline, "milestone_changed");
    assert_eq!(milestone_events.len(), 1);
    assert_eq!(
        milestone_events[0]["payload"],
        json!({ "from": null, "to": "v1" })
    );

    let general = api
        .post(
            jwt,
            &format!("/merge-requests/{mr_id}/comments"),
            json!({ "body": "looks promising" }),
        )
        .await;
    assert_eq!(
        general["author"]["username"], "admin",
        "POST comment must return the author, got: {general:#}"
    );
    let timeline = api.timeline(jwt, mr_id).await;
    let comment_items = items_of(&timeline, "comment");
    assert_eq!(comment_items.len(), 1);
    assert_eq!(comment_items[0]["body"], "looks promising");
    assert_eq!(comment_items[0]["author"]["username"], "admin");

    let root = api.post(jwt, &format!("/merge-requests/{mr_id}/comments"), json!({ "body": "why do we need this?", "filePath": "README.md", "lineNumber": 2, "side": "new" })).await;
    let root_id = root["id"].as_str().unwrap().to_string();
    api.post(
        reviewer_jwt,
        &format!("/merge-requests/{mr_id}/comments"),
        json!({ "body": "it explains step two", "replyToId": root_id }),
    )
    .await;

    let timeline = api.timeline(jwt, mr_id).await;
    let threads = items_of(&timeline, "thread");
    assert_eq!(
        threads.len(),
        1,
        "an inline comment and its reply are one thread, got: {timeline:#}"
    );
    assert_eq!(
        items_of(&timeline, "comment").len(),
        1,
        "only the general comment stays a plain comment item"
    );
    let thread = threads[0];
    assert_eq!(thread["id"], root_id.as_str());
    assert_eq!(thread["filePath"], "README.md");
    assert_eq!(thread["lineNumber"], 2);
    assert_eq!(thread["side"], "new");
    assert_eq!(thread["outdated"], false);
    assert_eq!(thread["resolved"], false);
    assert_eq!(thread["root"]["author"]["username"], "admin");
    assert_eq!(thread["replies"].as_array().unwrap().len(), 1);
    assert_eq!(thread["replies"][0]["author"]["username"], "reviewer");
    let excerpt = thread["excerpt"].as_array().unwrap();
    assert!(
        !excerpt.is_empty(),
        "a thread on a live diff line must carry an excerpt"
    );
    let last_excerpt_line = excerpt.last().unwrap();
    assert_eq!(last_excerpt_line["line"], 2);
    assert_eq!(last_excerpt_line["kind"], "added");
    assert_eq!(last_excerpt_line["content"], "line two\n");

    assert_eq!(
        api.post_no_body(
            jwt,
            &format!("/merge-requests/{mr_id}/comments/{root_id}/resolve")
        )
        .await,
        204
    );
    let timeline = api.timeline(jwt, mr_id).await;
    let thread = items_of(&timeline, "thread")[0];
    assert_eq!(thread["resolved"], true);
    assert_eq!(thread["resolvedBy"]["username"], "admin");
    assert!(thread["resolvedAt"].is_string());
    assert!(
        timeline["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["kind"] != "thread_resolved" && i["kind"] != "thread_reopened"),
        "resolution events feed the thread and never appear as items"
    );

    assert_eq!(
        api.post_no_body(
            jwt,
            &format!("/merge-requests/{mr_id}/comments/{root_id}/unresolve")
        )
        .await,
        204
    );
    let timeline = api.timeline(jwt, mr_id).await;
    let thread = items_of(&timeline, "thread")[0];
    assert_eq!(thread["resolved"], false);
    assert!(thread["resolvedBy"].is_null());
    assert!(thread["resolvedAt"].is_null());

    api.post(
        reviewer_jwt,
        &format!("/merge-requests/{mr_id}/reviews"),
        json!({ "decision": "approved" }),
    )
    .await;
    api.post(
        reviewer_jwt,
        &format!("/merge-requests/{mr_id}/reviews"),
        json!({ "decision": "changes_requested" }),
    )
    .await;
    let timeline = api.timeline(jwt, mr_id).await;
    let review_events = events_of(&timeline, "review_submitted");
    assert_eq!(
        review_events.len(),
        2,
        "both submissions are kept even though the reviewer's live review was replaced"
    );
    assert_eq!(review_events[0]["payload"]["decision"], "approved");
    assert_eq!(review_events[1]["payload"]["decision"], "changes_requested");
    assert_eq!(review_events[0]["actor"]["username"], "reviewer");

    assert!(
        events_of(&timeline, "commits_pushed").is_empty(),
        "no push has happened since the merge request was opened"
    );

    commit_file(
        &repo_path,
        "README.md",
        "line one\nline two\nline three\n",
        "more work",
    )
    .await;
    git(&["push", "-q", "origin", "HEAD:feature"], &repo_path).await;
    let feature_tip_2 = git(&["rev-parse", "HEAD"], &repo_path).await;
    let timeline = api.timeline(jwt, mr_id).await;
    let pushes = events_of(&timeline, "commits_pushed");
    assert_eq!(pushes.len(), 1);
    assert_ne!(
        pushes[0]["payload"]["fromSha"],
        pushes[0]["payload"]["toSha"]
    );
    assert_eq!(pushes[0]["payload"]["fromSha"], feature_tip_1.as_str());
    assert_eq!(pushes[0]["payload"]["toSha"], feature_tip_2.as_str());
    assert_eq!(pushes[0]["actor"]["username"], "admin");

    commit_file(
        &repo_path,
        "README.md",
        "line one\nline two\nline three\nline four\n",
        "even more work",
    )
    .await;
    git(&["push", "-q", "origin", "HEAD:feature"], &repo_path).await;
    let feature_tip_3 = git(&["rev-parse", "HEAD"], &repo_path).await;
    let timeline = api.timeline(jwt, mr_id).await;
    let pushes = events_of(&timeline, "commits_pushed");
    assert_eq!(pushes.len(), 2);
    assert_eq!(pushes[1]["payload"]["fromSha"], feature_tip_2.as_str());
    assert_eq!(pushes[1]["payload"]["toSha"], feature_tip_3.as_str());

    git(
        &["push", "-q", "origin", "HEAD:refs/heads/unrelated"],
        &repo_path,
    )
    .await;
    let timeline = api.timeline(jwt, mr_id).await;
    assert_eq!(
        events_of(&timeline, "commits_pushed").len(),
        2,
        "a push to another branch is not activity on this merge request"
    );

    let comments = api
        .get(jwt, &format!("/merge-requests/{mr_id}/comments"))
        .await;
    let comments = comments.as_array().unwrap();
    assert_eq!(comments.len(), 3);
    assert!(
        comments.iter().all(|c| c["author"]["username"].is_string()),
        "every listed comment carries its author, got: {comments:#?}"
    );

    let detail_status = api
        .get_status(outsider_jwt, &format!("/merge-requests/{mr_id}"))
        .await;
    assert_eq!(detail_status, 404);
    assert_eq!(
        api.get_status(outsider_jwt, &format!("/merge-requests/{mr_id}/timeline"))
            .await,
        detail_status
    );
    assert_eq!(
        api.get_status(reviewer_jwt, &format!("/merge-requests/{mr_id}/timeline"))
            .await,
        200
    );

    let mr2 = api
        .post(jwt, &format!("/repositories/{repo_id}/merge-requests"), json!({ "sourceBranch": "docs", "targetBranch": "main", "title": "Add contributing guide", "description": "" }))
        .await;
    let mr2_id = mr2["id"].as_str().unwrap().to_string();
    assert_eq!(
        api.post_no_body(jwt, &format!("/merge-requests/{mr2_id}/close"))
            .await,
        204
    );
    let timeline2 = api.timeline(jwt, &mr2_id).await;
    let closed = events_of(&timeline2, "closed");
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0]["actor"]["username"], "admin");

    let merge_res = api
        .post(jwt, &format!("/merge-requests/{mr_id}/merge"), json!({}))
        .await;
    assert_eq!(merge_res["outcome"], "merged");
    let merge_commit_sha = merge_res["mergeCommitSha"].as_str().unwrap().to_string();

    let timeline = api.timeline(jwt, mr_id).await;
    let merged = events_of(&timeline, "merged");
    assert_eq!(merged.len(), 1);
    assert_eq!(
        merged[0]["payload"]["mergeCommitSha"],
        merge_commit_sha.as_str()
    );
    assert_eq!(merged[0]["actor"]["username"], "admin");
    let last = timeline["items"].as_array().unwrap().last().unwrap();
    assert_eq!(last["kind"], "merged", "the merge is the newest activity");
    // Merged merge requests have no live diff: threads are never outdated and carry no excerpt.
    let thread = items_of(&timeline, "thread")[0];
    assert_eq!(thread["outdated"], false);
    assert!(thread["excerpt"].as_array().unwrap().is_empty());

    let created: Vec<chrono::DateTime<chrono::Utc>> = timeline["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["createdAt"].as_str().unwrap().parse().unwrap())
        .collect();
    assert!(
        created.windows(2).all(|pair| pair[0] <= pair[1]),
        "timeline items must be in non-decreasing createdAt order, got: {created:?}"
    );
}

/// The live diff only feeds excerpts and the outdated flag, so the timeline must still answer when it cannot be computed (source branch deleted).
#[sqlx::test]
async fn the_timeline_still_answers_when_the_source_branch_is_gone(pool: PgPool) {
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
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
    git(&["checkout", "-q", "-b", "feature"], &repo_path).await;
    commit_file(
        &repo_path,
        "README.md",
        "line one\nline two\n",
        "feature work",
    )
    .await;
    git(&["push", "-q", "origin", "HEAD:feature"], &repo_path).await;

    let mr = api
        .post(jwt, &format!("/repositories/{repo_id}/merge-requests"), json!({ "sourceBranch": "feature", "targetBranch": "main", "title": "Add line two", "description": "" }))
        .await;
    let mr_id = mr["id"].as_str().unwrap().to_string();
    let mr_id = mr_id.as_str();
    api.post(
        jwt,
        &format!("/merge-requests/{mr_id}/comments"),
        json!({ "body": "looks promising" }),
    )
    .await;
    api.post(
        jwt,
        &format!("/merge-requests/{mr_id}/comments"),
        json!({ "body": "why?", "filePath": "README.md", "lineNumber": 2, "side": "new" }),
    )
    .await;
    let timeline = api.timeline(jwt, mr_id).await;
    assert!(
        !items_of(&timeline, "thread")[0]["excerpt"]
            .as_array()
            .unwrap()
            .is_empty(),
        "sanity: the excerpt exists while the branch does"
    );

    git(&["push", "-q", "origin", "--delete", "feature"], &repo_path).await;
    assert_eq!(
        api.get_status(jwt, &format!("/merge-requests/{mr_id}/diff"))
            .await,
        500,
        "sanity: the diff can no longer be computed"
    );

    assert_eq!(
        api.get_status(jwt, &format!("/merge-requests/{mr_id}/timeline"))
            .await,
        200,
        "the timeline must not fail with the diff"
    );
    let timeline = api.timeline(jwt, mr_id).await;
    assert_eq!(items_of(&timeline, "comment").len(), 1);
    let threads = items_of(&timeline, "thread");
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0]["outdated"], false);
    assert!(threads[0]["excerpt"].as_array().unwrap().is_empty());
}
