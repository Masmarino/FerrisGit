use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use serde_json::json;
use sqlx::PgPool;
use std::net::SocketAddr;

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

/// The reader can see the repo but is below the Contributor+ gate on the mutating routes.
async fn setup_repo_with_reader(
    client: &reqwest::Client,
    addr: SocketAddr,
    admin_jwt: &str,
) -> (String, String) {
    let repo_res: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(admin_jwt)
        .json(&json!({ "name": "hello", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let repo_id = repo_res["id"].as_str().unwrap().to_string();

    client
        .post(format!("http://{addr}/api/admin/users"))
        .bearer_auth(admin_jwt)
        .json(&json!({ "username": "reader", "email": "reader@example.com", "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/collaborators"
        ))
        .bearer_auth(admin_jwt)
        .json(&json!({ "username": "reader", "role": "reader" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let reader_jwt = login(client, addr, "reader", "password12345").await;

    (repo_id, reader_jwt)
}

#[sqlx::test]
async fn a_label_can_be_updated_and_deleted_over_http_and_a_reader_is_rejected_from_both(
    pool: PgPool,
) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let admin_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repo_id, reader_jwt) = setup_repo_with_reader(&client, addr, &admin_jwt).await;

    let created: serde_json::Value = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/labels"))
        .bearer_auth(&admin_jwt)
        .json(&json!({ "name": "bug", "color": "#ff0000" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let label_id = created["id"].as_str().unwrap().to_string();

    let reader_create_attempt = client
        .post(format!("http://{addr}/api/repositories/{repo_id}/labels"))
        .bearer_auth(&reader_jwt)
        .json(&json!({ "name": "wontfix", "color": "#00ff00" }))
        .send()
        .await
        .unwrap();
    assert_eq!(reader_create_attempt.status(), 404);

    let reader_update_attempt = client
        .patch(format!("http://{addr}/api/labels/{label_id}"))
        .bearer_auth(&reader_jwt)
        .json(&json!({ "name": "renamed-by-reader", "color": "#000000" }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        reader_update_attempt.status(),
        404,
        "a Reader must not be able to update a label"
    );

    let updated: serde_json::Value = client
        .patch(format!("http://{addr}/api/labels/{label_id}"))
        .bearer_auth(&admin_jwt)
        .json(&json!({ "name": "critical-bug", "color": "#123456" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(updated["name"], "critical-bug");
    assert_eq!(updated["color"], "#123456");

    let listing: serde_json::Value = client
        .get(format!("http://{addr}/api/repositories/{repo_id}/labels"))
        .bearer_auth(&admin_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let names: Vec<&str> = listing
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        vec!["critical-bug"],
        "the update must be visible in a subsequent listing"
    );

    let reader_delete_attempt = client
        .delete(format!("http://{addr}/api/labels/{label_id}"))
        .bearer_auth(&reader_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        reader_delete_attempt.status(),
        404,
        "a Reader must not be able to delete a label"
    );

    let delete_res = client
        .delete(format!("http://{addr}/api/labels/{label_id}"))
        .bearer_auth(&admin_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(delete_res.status(), 204);

    let listing_after: serde_json::Value = client
        .get(format!("http://{addr}/api/repositories/{repo_id}/labels"))
        .bearer_auth(&admin_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        listing_after.as_array().unwrap().is_empty(),
        "a deleted label must no longer appear in the listing"
    );
}

#[sqlx::test]
async fn a_milestone_can_be_updated_and_deleted_over_http_and_a_reader_is_rejected_from_both(
    pool: PgPool,
) {
    let addr = spawn_server(pool).await;
    let client = reqwest::Client::new();
    let admin_jwt = login(&client, addr, "admin", "adminpassword123").await;
    let (repo_id, reader_jwt) = setup_repo_with_reader(&client, addr, &admin_jwt).await;

    let created: serde_json::Value = client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/milestones"
        ))
        .bearer_auth(&admin_jwt)
        .json(&json!({ "title": "v1.0", "description": "first release", "dueDate": null }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let milestone_id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["state"], "open");

    let reader_create_attempt = client
        .post(format!(
            "http://{addr}/api/repositories/{repo_id}/milestones"
        ))
        .bearer_auth(&reader_jwt)
        .json(&json!({ "title": "v2.0", "description": "", "dueDate": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(reader_create_attempt.status(), 404);

    let reader_update_attempt = client
        .patch(format!("http://{addr}/api/milestones/{milestone_id}"))
        .bearer_auth(&reader_jwt)
        .json(&json!({ "title": "renamed-by-reader", "description": "", "dueDate": null, "state": "open" }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        reader_update_attempt.status(),
        404,
        "a Reader must not be able to update a milestone"
    );

    let updated: serde_json::Value = client
        .patch(format!("http://{addr}/api/milestones/{milestone_id}"))
        .bearer_auth(&admin_jwt)
        .json(&json!({ "title": "v1.0-final", "description": "shipped", "dueDate": null, "state": "closed" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(updated["title"], "v1.0-final");
    assert_eq!(updated["description"], "shipped");
    assert_eq!(updated["state"], "closed");

    let listing: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_id}/milestones"
        ))
        .bearer_auth(&admin_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let titles: Vec<&str> = listing
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["title"].as_str().unwrap())
        .collect();
    assert_eq!(
        titles,
        vec!["v1.0-final"],
        "the update must be visible in a subsequent listing"
    );

    let reader_delete_attempt = client
        .delete(format!("http://{addr}/api/milestones/{milestone_id}"))
        .bearer_auth(&reader_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        reader_delete_attempt.status(),
        404,
        "a Reader must not be able to delete a milestone"
    );

    let delete_res = client
        .delete(format!("http://{addr}/api/milestones/{milestone_id}"))
        .bearer_auth(&admin_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(delete_res.status(), 204);

    let listing_after: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/repositories/{repo_id}/milestones"
        ))
        .bearer_auth(&admin_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        listing_after.as_array().unwrap().is_empty(),
        "a deleted milestone must no longer appear in the listing"
    );
}
