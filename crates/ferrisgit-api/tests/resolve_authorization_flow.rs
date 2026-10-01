use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use serde_json::json;
use sqlx::PgPool;

/// Regression: `/resolve` performed no authorization, so any logged-in user could enumerate repositories and groups by 200 vs 404.
/// A stranger must get 404 for all three `ResolvedPath` variants.
#[sqlx::test]
async fn resolve_endpoint_enforces_reader_access_for_every_resolved_path_variant(pool: PgPool) {
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

    let admin_login: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({ "username": "admin", "password": "adminpassword123" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let admin_jwt = admin_login["token"].as_str().unwrap();

    for username in ["owner", "stranger"] {
        client
            .post(format!("http://{addr}/api/admin/users"))
            .bearer_auth(admin_jwt)
            .json(&json!({ "username": username, "email": format!("{username}@example.com"), "password": "password12345" }))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
    }

    async fn login(client: &reqwest::Client, addr: std::net::SocketAddr, username: &str) -> String {
        let res: serde_json::Value = client
            .post(format!("http://{addr}/api/auth/login"))
            .json(&json!({ "username": username, "password": "password12345" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        res["token"].as_str().unwrap().to_string()
    }

    let owner_jwt = login(&client, addr, "owner").await;
    let stranger_jwt = login(&client, addr, "stranger").await;

    let create_personal_status = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "name": "widgets", "visibility": "private" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(create_personal_status, 200);

    let group_res = client
        .post(format!("http://{addr}/api/groups"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "name": "acme", "description": "" }))
        .send()
        .await
        .unwrap();
    assert_eq!(group_res.status(), 200);

    let create_group_repo_status = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "name": "gizmos", "visibility": "private", "groupPath": "acme" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(create_group_repo_status, 200);

    let owner_personal = client
        .get(format!("http://{addr}/api/resolve/owner/widgets"))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        owner_personal.status(),
        200,
        "owner must be able to resolve their own personal repository"
    );

    let owner_group = client
        .get(format!("http://{addr}/api/resolve/acme"))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        owner_group.status(),
        200,
        "owner must be able to resolve a group they're a maintainer of"
    );

    let owner_group_repo = client
        .get(format!("http://{addr}/api/resolve/acme/gizmos"))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        owner_group_repo.status(),
        200,
        "owner must be able to resolve a group-scoped repository they created"
    );

    let stranger_personal = client
        .get(format!("http://{addr}/api/resolve/owner/widgets"))
        .bearer_auth(&stranger_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        stranger_personal.status(),
        404,
        "a stranger must not be able to resolve someone else's personal repository"
    );
    let stranger_personal_body: serde_json::Value = stranger_personal.json().await.unwrap();

    let stranger_group = client
        .get(format!("http://{addr}/api/resolve/acme"))
        .bearer_auth(&stranger_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        stranger_group.status(),
        404,
        "a stranger must not be able to resolve a group they have no access to"
    );

    let stranger_group_repo = client
        .get(format!("http://{addr}/api/resolve/acme/gizmos"))
        .bearer_auth(&stranger_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        stranger_group_repo.status(),
        404,
        "a stranger must not be able to resolve a group-scoped repository they have no access to"
    );
    let stranger_group_repo_body: serde_json::Value = stranger_group_repo.json().await.unwrap();

    // The body must not distinguish "denied" from "absent" either (`{"error":"repository"}` vs `{"error":"path"}`): compare against nonexistent paths of the same shape.

    let nonexistent_personal = client
        .get(format!("http://{addr}/api/resolve/owner/nonexistent"))
        .bearer_auth(&stranger_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        nonexistent_personal.status(),
        404,
        "a nonexistent personal-repository path must also 404"
    );
    let nonexistent_personal_body: serde_json::Value = nonexistent_personal.json().await.unwrap();
    assert_eq!(
        stranger_personal_body, nonexistent_personal_body,
        "the body for a hidden personal repository must be identical to the body for a nonexistent path — otherwise existence leaks through the error message"
    );

    let nonexistent_group_repo = client
        .get(format!("http://{addr}/api/resolve/acme/nonexistent"))
        .bearer_auth(&stranger_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        nonexistent_group_repo.status(),
        404,
        "a nonexistent group-scoped-repository path must also 404"
    );
    let nonexistent_group_repo_body: serde_json::Value =
        nonexistent_group_repo.json().await.unwrap();
    assert_eq!(
        stranger_group_repo_body, nonexistent_group_repo_body,
        "the body for a hidden group repository must be identical to the body for a nonexistent path — otherwise existence leaks through the error message"
    );
}
