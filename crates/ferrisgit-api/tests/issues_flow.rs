use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test]
async fn issues_support_kanban_assignment_comments_and_notifications(pool: PgPool) {
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

    let client = reqwest::Client::new();

    let owner_login_res = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "admin", "password": "adminpassword123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(owner_login_res.status(), 200);
    let owner_login: serde_json::Value = owner_login_res.json().await.unwrap();
    let owner_jwt = owner_login["token"].as_str().unwrap();

    let repo_res = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "name": "hello", "visibility": "private" }))
        .send()
        .await
        .unwrap();
    assert_eq!(repo_res.status(), 200);
    let repo_body: serde_json::Value = repo_res.json().await.unwrap();
    assert_eq!(repo_body["name"], "hello");
    assert_eq!(repo_body["owner"], "admin");
    let repo_id = repo_body["id"].as_str().unwrap().to_string();

    let create_contributor_res = client
        .post(format!("http://{addr}/api/admin/users"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "username": "contributor", "email": "contributor@example.com", "password": "password12345" }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_contributor_res.status(), 200);
    let contributor_user: serde_json::Value = create_contributor_res.json().await.unwrap();
    let contributor_id = contributor_user["id"].as_str().unwrap().to_string();

    let contributor_login_res = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "contributor", "password": "password12345" }))
        .send()
        .await
        .unwrap();
    assert_eq!(contributor_login_res.status(), 200);
    let contributor_login: serde_json::Value = contributor_login_res.json().await.unwrap();
    let contributor_jwt = contributor_login["token"].as_str().unwrap();

    let add_collaborator_res = client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/collaborators"
        ))
        .bearer_auth(owner_jwt)
        .json(&json!({ "username": "contributor", "role": "contributor" }))
        .send()
        .await
        .unwrap();
    assert_eq!(add_collaborator_res.status(), 204);

    let create_issue_res = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/issues"))
        .bearer_auth(owner_jwt)
        .json(&json!({ "title": "Bug in login", "description": "Login fails with a 500 on bad credentials.", "kind": "bug" }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_issue_res.status(), 200);
    let created_issue: serde_json::Value = create_issue_res.json().await.unwrap();
    assert_eq!(created_issue["number"], 1);
    assert_eq!(created_issue["status"], "todo");
    assert_eq!(created_issue["kind"], "bug");
    assert_eq!(created_issue["title"], "Bug in login");
    assert_eq!(
        created_issue["description"],
        "Login fails with a 500 on bad credentials."
    );
    assert_eq!(created_issue["assigneeId"], serde_json::Value::Null);
    assert_eq!(created_issue["closedAt"], serde_json::Value::Null);
    let owner_author_id = created_issue["authorId"].as_str().unwrap().to_string();

    let list_issues_res = client
        .get(format!("http://{addr}/api/repositories/{repo_id}/issues"))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(list_issues_res.status(), 200);
    let issues: serde_json::Value = list_issues_res.json().await.unwrap();
    let issues = issues.as_array().unwrap();
    assert_eq!(
        issues.len(),
        1,
        "expected exactly one issue after creation, got: {issues:#?}"
    );
    assert_eq!(issues[0]["number"], 1);
    assert_eq!(issues[0]["title"], "Bug in login");
    assert_eq!(issues[0]["status"], "todo");
    assert_eq!(issues[0]["kind"], "bug");
    assert_eq!(issues[0]["authorId"], owner_author_id.as_str());

    let assign_res = client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/issues/1/assign"
        ))
        .bearer_auth(owner_jwt)
        .json(&json!({ "assigneeId": contributor_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(assign_res.status(), 200);
    let assigned_issue: serde_json::Value = assign_res.json().await.unwrap();
    assert_eq!(assigned_issue["assigneeId"], contributor_id.as_str());
    assert_eq!(assigned_issue["number"], 1);
    assert_eq!(assigned_issue["status"], "todo");

    // Only look at `issue_assigned`, so the earlier `collaborator_added` notification is not mixed in.
    let contributor_notifications_after_assign_res = client
        .get(format!("http://{addr}/api/notifications"))
        .bearer_auth(contributor_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(contributor_notifications_after_assign_res.status(), 200);
    let contributor_notifications_after_assign: serde_json::Value =
        contributor_notifications_after_assign_res
            .json()
            .await
            .unwrap();
    let contributor_notifications_after_assign =
        contributor_notifications_after_assign.as_array().unwrap();
    let contributor_assigned_notifications: Vec<&serde_json::Value> =
        contributor_notifications_after_assign
            .iter()
            .filter(|n| n["kind"] == "issue_assigned")
            .collect();
    assert_eq!(
        contributor_assigned_notifications.len(),
        1,
        "expected exactly one issue_assigned notification for the contributor after being assigned, got: {contributor_notifications_after_assign:#?}"
    );
    let assigned_notification = contributor_assigned_notifications[0];
    assert_eq!(assigned_notification["kind"], "issue_assigned");
    assert_eq!(assigned_notification["repositoryOwner"], "admin");
    assert_eq!(assigned_notification["repositoryName"], "hello");
    assert_eq!(assigned_notification["actorUsername"], "admin");
    assert_eq!(assigned_notification["read"], false);

    let move_status_res = client
        .patch(format!(
            "http://{addr}/api/repositories/{repo_id}/issues/1/status"
        ))
        .bearer_auth(contributor_jwt)
        .json(&json!({ "status": "in_progress" }))
        .send()
        .await
        .unwrap();
    assert_eq!(move_status_res.status(), 200);
    let moved_issue: serde_json::Value = move_status_res.json().await.unwrap();
    assert_eq!(moved_issue["status"], "in_progress");
    assert_eq!(moved_issue["number"], 1);

    let comment_res = client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/issues/1/comments"
        ))
        .bearer_auth(contributor_jwt)
        .json(&json!({ "body": "Working on it" }))
        .send()
        .await
        .unwrap();
    assert_eq!(comment_res.status(), 200);
    let comment: serde_json::Value = comment_res.json().await.unwrap();
    assert_eq!(comment["body"], "Working on it");
    assert_eq!(comment["authorId"], contributor_id.as_str());

    let owner_notifications_after_comment_res = client
        .get(format!("http://{addr}/api/notifications"))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(owner_notifications_after_comment_res.status(), 200);
    let owner_notifications_after_comment: serde_json::Value =
        owner_notifications_after_comment_res.json().await.unwrap();
    let owner_notifications_after_comment = owner_notifications_after_comment.as_array().unwrap();
    let owner_commented_notifications: Vec<&serde_json::Value> = owner_notifications_after_comment
        .iter()
        .filter(|n| n["kind"] == "issue_commented")
        .collect();
    assert_eq!(
        owner_commented_notifications.len(),
        1,
        "expected exactly one issue_commented notification for the author, got: {owner_notifications_after_comment:#?}"
    );
    let commented_notification = owner_commented_notifications[0];
    assert_eq!(commented_notification["repositoryOwner"], "admin");
    assert_eq!(commented_notification["repositoryName"], "hello");
    assert_eq!(commented_notification["actorUsername"], "contributor");
    assert_eq!(commented_notification["read"], false);

    // The commenter is also the assignee: no notification for their own comment, so the list still holds only the two earlier ones.
    let contributor_notifications_after_comment_res = client
        .get(format!("http://{addr}/api/notifications"))
        .bearer_auth(contributor_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(contributor_notifications_after_comment_res.status(), 200);
    let contributor_notifications_after_comment: serde_json::Value =
        contributor_notifications_after_comment_res
            .json()
            .await
            .unwrap();
    let contributor_notifications_after_comment =
        contributor_notifications_after_comment.as_array().unwrap();
    let contributor_commented_notifications: Vec<&serde_json::Value> =
        contributor_notifications_after_comment
            .iter()
            .filter(|n| n["kind"] == "issue_commented")
            .collect();
    assert!(
        contributor_commented_notifications.is_empty(),
        "the contributor is both the commenter and the assignee — they must never receive an issue_commented notification for their own comment, got: {contributor_commented_notifications:#?}"
    );
    assert_eq!(
        contributor_notifications_after_comment.len(),
        2,
        "expected the contributor's notifications to still be just the collaborator_added and issue_assigned notifications, with nothing new from their own comment, got: {contributor_notifications_after_comment:#?}"
    );
    let contributor_kinds: std::collections::HashSet<&str> =
        contributor_notifications_after_comment
            .iter()
            .map(|n| n["kind"].as_str().unwrap())
            .collect();
    assert_eq!(
        contributor_kinds,
        std::collections::HashSet::from(["collaborator_added", "issue_assigned"])
    );

    let close_res = client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/issues/1/close"
        ))
        .bearer_auth(contributor_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(close_res.status(), 200);
    let closed_issue: serde_json::Value = close_res.json().await.unwrap();
    assert_eq!(closed_issue["status"], "done");
    assert_eq!(closed_issue["number"], 1);
    assert_ne!(
        closed_issue["closedAt"],
        serde_json::Value::Null,
        "closedAt must be set once the issue is closed"
    );

    let owner_notifications_after_close_res = client
        .get(format!("http://{addr}/api/notifications"))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(owner_notifications_after_close_res.status(), 200);
    let owner_notifications_after_close: serde_json::Value =
        owner_notifications_after_close_res.json().await.unwrap();
    let owner_notifications_after_close = owner_notifications_after_close.as_array().unwrap();
    let owner_closed_notifications: Vec<&serde_json::Value> = owner_notifications_after_close
        .iter()
        .filter(|n| n["kind"] == "issue_closed")
        .collect();
    assert_eq!(
        owner_closed_notifications.len(),
        1,
        "expected exactly one issue_closed notification for the author, got: {owner_notifications_after_close:#?}"
    );
    let closed_notification = owner_closed_notifications[0];
    assert_eq!(closed_notification["repositoryOwner"], "admin");
    assert_eq!(closed_notification["repositoryName"], "hello");
    assert_eq!(closed_notification["actorUsername"], "contributor");
    assert_eq!(closed_notification["read"], false);

    assert_eq!(
        owner_notifications_after_close.len(),
        2,
        "expected the owner to have exactly the issue_commented and issue_closed notifications, got: {owner_notifications_after_close:#?}"
    );

    let reopen_res = client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/issues/1/reopen"
        ))
        .bearer_auth(owner_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(reopen_res.status(), 200);
    let reopened_issue: serde_json::Value = reopen_res.json().await.unwrap();
    assert_eq!(reopened_issue["status"], "todo");
    assert_eq!(reopened_issue["number"], 1);
    assert_eq!(reopened_issue["closedAt"], serde_json::Value::Null);
}

#[sqlx::test]
async fn issues_carry_labels_and_milestones_and_the_list_can_be_filtered_by_both(pool: PgPool) {
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

    let client = reqwest::Client::new();

    let login: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "admin", "password": "adminpassword123" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let jwt = login["token"].as_str().unwrap();

    let repo: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(jwt)
        .json(&json!({ "name": "hello", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let repo_id = repo["id"].as_str().unwrap().to_string();

    // A milestone scoped to another repository must never be assignable here.
    let other_repo: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(jwt)
        .json(&json!({ "name": "other", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let other_repo_id = other_repo["id"].as_str().unwrap().to_string();

    let issue_one: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/issues"))
        .bearer_auth(jwt)
        .json(&json!({ "title": "Tagged issue", "description": "d", "kind": "bug" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(issue_one["number"], 1);
    assert_eq!(issue_one["milestoneId"], serde_json::Value::Null);
    assert_eq!(issue_one["labels"].as_array().unwrap().len(), 0);

    let issue_two: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/issues"))
        .bearer_auth(jwt)
        .json(&json!({ "title": "Untagged issue", "description": "d", "kind": "task" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(issue_two["number"], 2);

    // A bare date value is rejected: only RFC 3339 is accepted.
    let bare_date_status = client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/milestones"
        ))
        .bearer_auth(jwt)
        .json(&json!({ "title": "bare-date", "description": "", "dueDate": "2026-03-01" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        bare_date_status, 422,
        "a bare calendar date is not RFC 3339 and must be rejected"
    );

    let milestone: serde_json::Value = client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/milestones"
        ))
        .bearer_auth(jwt)
        .json(&json!({ "title": "v1", "description": "", "dueDate": "2026-03-01T00:00:00.000Z" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let milestone_id = milestone["id"].as_str().unwrap().to_string();
    assert_eq!(
        milestone["dueDate"], "2026-03-01T00:00:00Z",
        "the RFC 3339 instant must round-trip, preserving the chosen day"
    );

    let other_milestone: serde_json::Value = client
        .post(format!(
            "http://{addr}/api/repositories/{other_repo_id}/milestones"
        ))
        .bearer_auth(jwt)
        .json(&json!({ "title": "out-of-scope", "description": "", "dueDate": null }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let other_milestone_id = other_milestone["id"].as_str().unwrap().to_string();

    let out_of_scope_status = client
        .patch(format!("http://{addr}/api/repositories/{repo_id}/issues/1"))
        .bearer_auth(jwt)
        .json(&json!({ "title": "Tagged issue", "description": "d", "kind": "bug", "milestoneId": other_milestone_id }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        out_of_scope_status, 400,
        "a milestone scoped to a different repository must be rejected"
    );

    let with_milestone: serde_json::Value = client
        .patch(format!("http://{addr}/api/repositories/{repo_id}/issues/1"))
        .bearer_auth(jwt)
        .json(&json!({ "title": "Tagged issue", "description": "d", "kind": "bug", "milestoneId": milestone_id }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(with_milestone["milestoneId"], milestone_id.as_str());

    // Full-replace PATCH: an omitted `milestoneId` is refused rather than clearing the milestone.
    let omitted_res = client
        .patch(format!("http://{addr}/api/repositories/{repo_id}/issues/1"))
        .bearer_auth(jwt)
        .json(&json!({ "title": "Tagged issue", "description": "d", "kind": "bug" }))
        .send()
        .await
        .unwrap();
    assert_eq!(omitted_res.status(), 400);
    let omitted_body: serde_json::Value = omitted_res.json().await.unwrap();
    assert!(
        omitted_body["error"]
            .as_str()
            .unwrap()
            .contains("milestoneId is required"),
        "the rejection must say what is missing, got: {omitted_body:#}"
    );

    let after_rejected: serde_json::Value = client
        .get(format!("http://{addr}/api/repositories/{repo_id}/issues/1"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        after_rejected["milestoneId"],
        milestone_id.as_str(),
        "a rejected update must not have cleared the milestone"
    );

    let label: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/labels"))
        .bearer_auth(jwt)
        .json(&json!({ "name": "Bug", "color": "#dc2626" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let label_id = label["id"].as_str().unwrap().to_string();

    let set_labels: serde_json::Value = client
        .put(format!(
            "http://{addr}/api/repositories/{repo_id}/issues/1/labels"
        ))
        .bearer_auth(jwt)
        .json(&json!({ "labelIds": [label_id] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let set_labels = set_labels.as_array().unwrap();
    assert_eq!(set_labels.len(), 1);
    assert_eq!(set_labels[0]["id"], label_id.as_str());

    // Repeating a label id in one request must not blow up on the join table's primary key.
    let duplicate_labels: serde_json::Value = client
        .put(format!(
            "http://{addr}/api/repositories/{repo_id}/issues/1/labels"
        ))
        .bearer_auth(jwt)
        .json(&json!({ "labelIds": [label_id, label_id] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        duplicate_labels.as_array().unwrap().len(),
        1,
        "a repeated label id must collapse, not 500"
    );

    let other_label: serde_json::Value = client
        .post(format!(
            "http://{addr}/api/repositories/{other_repo_id}/labels"
        ))
        .bearer_auth(jwt)
        .json(&json!({ "name": "Elsewhere", "color": "#2563eb" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let other_label_id = other_label["id"].as_str().unwrap().to_string();
    let out_of_scope_label_status = client
        .put(format!(
            "http://{addr}/api/repositories/{repo_id}/issues/1/labels"
        ))
        .bearer_auth(jwt)
        .json(&json!({ "labelIds": [other_label_id] }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        out_of_scope_label_status, 400,
        "a label scoped to a different repository must be rejected"
    );

    let detail: serde_json::Value = client
        .get(format!("http://{addr}/api/repositories/{repo_id}/issues/1"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(detail["milestoneId"], milestone_id.as_str());
    assert_eq!(detail["labels"].as_array().unwrap().len(), 1);
    assert_eq!(detail["labels"][0]["id"], label_id.as_str());

    let by_label: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_id}/issues?labelIds={label_id}"
        ))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let by_label_numbers: Vec<u64> = by_label
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["number"].as_u64().unwrap())
        .collect();
    assert_eq!(by_label_numbers, vec![1]);

    let by_milestone: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_id}/issues?milestoneId={milestone_id}"
        ))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let by_milestone_numbers: Vec<u64> = by_milestone
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["number"].as_u64().unwrap())
        .collect();
    assert_eq!(by_milestone_numbers, vec![1]);

    let by_both: serde_json::Value = client
        .get(format!("http://{addr}/api/repositories/{repo_id}/issues?labelIds={label_id}&milestoneId={milestone_id}"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let by_both_numbers: Vec<u64> = by_both
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["number"].as_u64().unwrap())
        .collect();
    assert_eq!(
        by_both_numbers,
        vec![1],
        "both filters together must still match the one issue that satisfies each"
    );

    let by_other_milestone: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_id}/issues?milestoneId={other_milestone_id}"
        ))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        by_other_milestone.as_array().unwrap().is_empty(),
        "filtering by a milestone from another repository must match nothing"
    );

    let unfiltered: serde_json::Value = client
        .get(format!("http://{addr}/api/repositories/{repo_id}/issues"))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let unfiltered_numbers: Vec<u64> = unfiltered
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["number"].as_u64().unwrap())
        .collect();
    assert!(unfiltered_numbers.contains(&1));
    assert!(
        unfiltered_numbers.contains(&2),
        "the untagged issue must still appear when not filtering"
    );

    let bad_label_filter_status = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_id}/issues?labelIds=not-a-uuid"
        ))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        bad_label_filter_status, 400,
        "a malformed labelIds filter must be a clean 400, not a 500"
    );
}
