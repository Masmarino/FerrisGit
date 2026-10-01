use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test]
async fn nested_group_repository_access_inherits_across_two_levels_and_enforces_namespace_uniqueness(
    pool: PgPool,
) {
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

    for username in ["owner", "alice", "stranger"] {
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
    let alice_jwt = login(&client, addr, "alice").await;
    let stranger_jwt = login(&client, addr, "stranger").await;

    let acme_res = client
        .post(format!("http://{addr}/api/groups"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "name": "acme", "description": "Acme Corp" }))
        .send()
        .await
        .unwrap();
    assert_eq!(acme_res.status(), 200);
    let acme_body: serde_json::Value = acme_res.json().await.unwrap();
    let acme_id = acme_body["id"].as_str().unwrap();

    let acme_members: serde_json::Value = client
        .get(format!("http://{addr}/api/groups/{acme_id}/members"))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let acme_members = acme_members.as_array().unwrap();
    let owner_member = acme_members
        .iter()
        .find(|m| m["username"] == "owner")
        .expect("owner must be listed as a member of the group they created");
    assert_eq!(
        owner_member["role"], "maintainer",
        "the creator of a root group must be auto-added as maintainer"
    );

    let backend_res = client
        .post(format!("http://{addr}/api/groups/{acme_id}/subgroups"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "name": "backend", "description": "Backend team" }))
        .send()
        .await
        .unwrap();
    assert_eq!(backend_res.status(), 200);
    let backend_body: serde_json::Value = backend_res.json().await.unwrap();
    let backend_id = backend_body["id"].as_str().unwrap();
    assert_eq!(backend_body["parentGroupId"], acme_id);

    let add_alice_status = client
        .post(format!("http://{addr}/api/groups/{backend_id}/members"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "username": "alice", "role": "contributor" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(add_alice_status, 200);

    let create_repo_res = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "name": "terraform-modules", "visibility": "private", "groupPath": "acme/backend" }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_repo_res.status(), 200);

    let resolve_res: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/resolve/acme/backend/terraform-modules"
        ))
        .bearer_auth(&alice_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(resolve_res["type"], "groupRepository");
    let repository_id = resolve_res["repositoryId"].as_str().unwrap().to_string();

    let alice_read_res = client
        .get(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}"
        ))
        .bearer_auth(&alice_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        alice_read_res.status(),
        200,
        "alice's contributor grant on the ancestor `backend` group must inherit down to read access on the repository"
    );
    let alice_read_body: serde_json::Value = alice_read_res.json().await.unwrap();
    assert_eq!(
        alice_read_body["role"], "contributor",
        "alice's resolved role on the repository must reflect her group-inherited role, not a default"
    );
    assert_eq!(
        alice_read_body["path"],
        json!(["acme", "backend", "terraform-modules"]),
        "a group repository's resolvable path is [...ancestor_group_names, name] — {{owner}}/{{name}} can never resolve it"
    );

    let stranger_read_status = client
        .get(format!(
            "http://{addr}/api/repositories/by-id/{repository_id}"
        ))
        .bearer_auth(&stranger_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        stranger_read_status, 404,
        "a caller with no access anywhere in the group chain must see 404, not 401 — no existence leak"
    );

    // Regression: the group listing hardcoded role "member". The reported role must be the caller's resolved one.

    let alice_list_res = client
        .get(format!("http://{addr}/api/repositories"))
        .bearer_auth(&alice_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(alice_list_res.status(), 200);
    let alice_list_body: serde_json::Value = alice_list_res.json().await.unwrap();
    let alice_list = alice_list_body.as_array().unwrap();
    let alice_listed_repo = alice_list.iter().find(|r| r["id"] == repository_id).expect("alice's GET /repositories must include a repository reachable only through group membership");
    assert_eq!(
        alice_listed_repo["role"], "contributor",
        "a group-inherited repository listing must report the caller's real resolved role, not a hardcoded placeholder"
    );
    assert_eq!(
        alice_list
            .iter()
            .filter(|r| r["id"] == repository_id)
            .count(),
        1,
        "the group-inherited repository must appear exactly once, not duplicated"
    );
    assert_eq!(
        alice_listed_repo["path"],
        json!(["acme", "backend", "terraform-modules"]),
        "GET /repositories must also carry the resolvable path for a group repository, not just {{owner}}/{{name}}"
    );

    let backend_repos_res = client
        .get(format!(
            "http://{addr}/api/groups/{backend_id}/repositories"
        ))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(backend_repos_res.status(), 200);
    let backend_repos_body: serde_json::Value = backend_repos_res.json().await.unwrap();
    let backend_repos = backend_repos_body.as_array().unwrap();
    let backend_listed_repo = backend_repos
        .iter()
        .find(|r| r["id"] == repository_id)
        .expect("the group repository must be listed under its own group");
    assert_eq!(
        backend_listed_repo["path"],
        json!(["acme", "backend", "terraform-modules"])
    );

    let alice_member_groups_res = client
        .get(format!("http://{addr}/api/groups/member"))
        .bearer_auth(&alice_jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(alice_member_groups_res.status(), 200);
    let alice_member_groups_body: serde_json::Value = alice_member_groups_res.json().await.unwrap();
    let alice_member_groups = alice_member_groups_body.as_array().unwrap();
    assert_eq!(
        alice_member_groups.len(),
        1,
        "alice should see exactly the one group she's a direct member of, got {alice_member_groups:?}"
    );
    assert_eq!(alice_member_groups[0]["id"], backend_id);
    assert_eq!(alice_member_groups[0]["path"], "acme/backend");

    let register_acme_status = client
        .post(format!("http://{addr}/api/admin/users"))
        .bearer_auth(admin_jwt)
        .json(&json!({ "username": "acme", "email": "acme@example.com", "password": "password12345" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        register_acme_status, 409,
        "a username colliding with an existing root group must be rejected as a conflict"
    );

    let create_group_named_alice_status = client
        .post(format!("http://{addr}/api/groups"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "name": "alice", "description": "" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        create_group_named_alice_status, 409,
        "a root group name colliding with an existing username must be rejected as a conflict"
    );
}

/// Regression: group listings hardcoded role "member" instead of each caller's resolved role.
#[sqlx::test]
async fn a_group_maintainer_and_a_group_reader_see_their_real_resolved_role_on_the_same_group_repository_listing(
    pool: PgPool,
) {
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

    for username in ["owner", "maintainer", "reader"] {
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
    let maintainer_jwt = login(&client, addr, "maintainer").await;
    let reader_jwt = login(&client, addr, "reader").await;

    let acme_res = client
        .post(format!("http://{addr}/api/groups"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "name": "acme", "description": "" }))
        .send()
        .await
        .unwrap();
    assert_eq!(acme_res.status(), 200);
    let acme_body: serde_json::Value = acme_res.json().await.unwrap();
    let acme_id = acme_body["id"].as_str().unwrap();

    let create_repo_res = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "name": "widget", "visibility": "private", "groupPath": "acme" }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_repo_res.status(), 200);
    let repository_id = create_repo_res.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let add_maintainer_status = client
        .post(format!("http://{addr}/api/groups/{acme_id}/members"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "username": "maintainer", "role": "maintainer" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(add_maintainer_status, 200);

    let add_reader_status = client
        .post(format!("http://{addr}/api/groups/{acme_id}/members"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "username": "reader", "role": "reader" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(add_reader_status, 200);

    let maintainer_list: serde_json::Value = client
        .get(format!("http://{addr}/api/repositories"))
        .bearer_auth(&maintainer_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let maintainer_entry = maintainer_list
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == repository_id)
        .expect(
            "the group Maintainer must see the repository in their own GET /repositories listing",
        );
    assert_eq!(
        maintainer_entry["role"], "maintainer",
        "a group Maintainer must see their real resolved role, not a hardcoded placeholder"
    );

    let reader_list: serde_json::Value = client
        .get(format!("http://{addr}/api/repositories"))
        .bearer_auth(&reader_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let reader_entry = reader_list
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == repository_id)
        .expect("the group Reader must see the repository in their own GET /repositories listing");
    assert_eq!(
        reader_entry["role"], "reader",
        "a group Reader must see their real resolved role, not the Maintainer's role and not a hardcoded placeholder"
    );

    let maintainer_group_list: serde_json::Value = client
        .get(format!("http://{addr}/api/groups/{acme_id}/repositories"))
        .bearer_auth(&maintainer_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let maintainer_group_entry = maintainer_group_list
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == repository_id)
        .expect("the repository must be listed under its own group");
    assert_eq!(
        maintainer_group_entry["role"], "maintainer",
        "GET /groups/{{id}}/repositories must also report the caller's real resolved role"
    );

    let reader_group_list: serde_json::Value = client
        .get(format!("http://{addr}/api/groups/{acme_id}/repositories"))
        .bearer_auth(&reader_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let reader_group_entry = reader_group_list
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == repository_id)
        .expect("the repository must be listed under its own group");
    assert_eq!(
        reader_group_entry["role"], "reader",
        "GET /groups/{{id}}/repositories must also report the caller's real resolved role, not the Maintainer's"
    );
}
