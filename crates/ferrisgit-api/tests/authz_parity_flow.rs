use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use serde_json::json;
use sqlx::PgPool;

/// `require_role_by_id` (JSON API) and `AuthenticateGitRequestUseCase::effective_role` (git smart-HTTP) are two copies of one policy:
/// for the same hierarchy and grants, a by-id read and a git clone must give the same allow/deny outcome.
#[sqlx::test]
async fn require_role_by_id_and_git_effective_role_agree_on_every_scenario(pool: PgPool) {
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

    let storage_dir = tempfile::tempdir().unwrap();
    let static_dir = tempfile::tempdir().unwrap();
    std::fs::write(static_dir.path().join("index.html"), "<html>spa</html>").unwrap();

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

    for username in ["owner", "reader", "contributor", "stranger", "colead"] {
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

    async fn api_token(client: &reqwest::Client, addr: std::net::SocketAddr, jwt: &str) -> String {
        let res: serde_json::Value = client
            .post(format!("http://{addr}/api/tokens"))
            .bearer_auth(jwt)
            .json(&json!({ "name": "ci" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        res["token"].as_str().unwrap().to_string()
    }

    let owner_jwt = login(&client, addr, "owner").await;
    let reader_jwt = login(&client, addr, "reader").await;
    let contributor_jwt = login(&client, addr, "contributor").await;
    let stranger_jwt = login(&client, addr, "stranger").await;
    let colead_jwt = login(&client, addr, "colead").await;

    let owner_token = api_token(&client, addr, &owner_jwt).await;
    let reader_token = api_token(&client, addr, &reader_jwt).await;
    let contributor_token = api_token(&client, addr, &contributor_jwt).await;
    let stranger_token = api_token(&client, addr, &stranger_jwt).await;

    let root_res = client
        .post(format!("http://{addr}/api/groups"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "name": "roleco", "description": "" }))
        .send()
        .await
        .unwrap();
    assert_eq!(root_res.status(), 200);
    let root_id = root_res.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let sub_res = client
        .post(format!("http://{addr}/api/groups/{root_id}/subgroups"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "name": "eng", "description": "" }))
        .send()
        .await
        .unwrap();
    assert_eq!(sub_res.status(), 200);
    let sub_id = sub_res.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let add_colead = client
        .post(format!("http://{addr}/api/groups/{root_id}/members"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "username": "colead", "role": "maintainer" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(add_colead, 200);

    let add_reader = client
        .post(format!("http://{addr}/api/groups/{sub_id}/members"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "username": "reader", "role": "reader" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(add_reader, 200);

    let add_contributor = client
        .post(format!("http://{addr}/api/groups/{sub_id}/members"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "username": "contributor", "role": "contributor" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(add_contributor, 200);

    let create_repo_res = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "name": "svc", "visibility": "private", "groupPath": "roleco/eng" }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_repo_res.status(), 200);

    let resolve_res: serde_json::Value = client
        .get(format!("http://{addr}/api/resolve/roleco/eng/svc"))
        .bearer_auth(&colead_jwt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let repository_id = resolve_res["repositoryId"].as_str().unwrap().to_string();

    #[allow(clippy::too_many_arguments)]
    async fn assert_parity(
        client: &reqwest::Client,
        addr: std::net::SocketAddr,
        jwt: &str,
        username: &str,
        token: &str,
        repository_id: &str,
        git_path: &str,
        expect_allowed: bool,
        scenario: &str,
    ) {
        let by_id_status = client
            .get(format!(
                "http://{addr}/api/repositories/by-id/{repository_id}"
            ))
            .bearer_auth(jwt)
            .send()
            .await
            .unwrap()
            .status();
        let by_id_allowed = by_id_status == 200;
        assert_eq!(
            by_id_allowed, expect_allowed,
            "[{scenario}] require_role_by_id (GET /api/repositories/by-id) disagreed with the expected outcome: got status {by_id_status}"
        );

        let info_refs_status = client
            .get(format!(
                "http://{addr}/{git_path}.git/info/refs?service=git-upload-pack"
            ))
            .basic_auth(username, Some(token))
            .send()
            .await
            .unwrap()
            .status();
        let git_allowed = info_refs_status == 200;
        assert_eq!(
            git_allowed, expect_allowed,
            "[{scenario}] effective_role (git info/refs) disagreed with the expected outcome: got status {info_refs_status}"
        );

        assert_eq!(
            by_id_allowed, git_allowed,
            "[{scenario}] require_role_by_id and effective_role produced DIFFERENT outcomes for the same caller and repository — the two policies have drifted out of agreement"
        );
    }

    assert_parity(
        &client,
        addr,
        &reader_jwt,
        "reader",
        &reader_token,
        &repository_id,
        "roleco/eng/svc",
        true,
        "group Reader",
    )
    .await;

    assert_parity(
        &client,
        addr,
        &contributor_jwt,
        "contributor",
        &contributor_token,
        &repository_id,
        "roleco/eng/svc",
        true,
        "group Contributor",
    )
    .await;

    assert_parity(
        &client,
        addr,
        &stranger_jwt,
        "stranger",
        &stranger_token,
        &repository_id,
        "roleco/eng/svc",
        false,
        "stranger with no grant",
    )
    .await;

    // `owner` holds two direct memberships (root and `eng`, both auto-added on group creation). Both must go for "no group role" to hold.
    let remove_owner_from_root_status = client
        .delete(format!("http://{addr}/api/groups/{root_id}/members/owner"))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(remove_owner_from_root_status, 200);
    let remove_owner_from_sub_status = client
        .delete(format!("http://{addr}/api/groups/{sub_id}/members/owner"))
        .bearer_auth(&owner_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(remove_owner_from_sub_status, 200);
    assert_parity(
        &client,
        addr,
        &owner_jwt,
        "owner",
        &owner_token,
        &repository_id,
        "roleco/eng/svc",
        false,
        "creator with no group role",
    )
    .await;

    // A revoked role must be denied on the very next request (no cached allow). `colead` performs the removal since `owner` has no permission left.
    let remove_reader_status = client
        .delete(format!("http://{addr}/api/groups/{sub_id}/members/reader"))
        .bearer_auth(&colead_jwt)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(remove_reader_status, 200);
    assert_parity(
        &client,
        addr,
        &reader_jwt,
        "reader",
        &reader_token,
        &repository_id,
        "roleco/eng/svc",
        false,
        "reader after role removal",
    )
    .await;

    // Public visibility: a stranger may read a public repository through both the API and git.
    let create_public_repo_res = client
        .post(format!("http://{addr}/api/repositories"))
        .bearer_auth(&owner_jwt)
        .json(&json!({ "name": "open-project", "visibility": "public" }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_public_repo_res.status(), 200);
    let public_repo_id = create_public_repo_res
        .json::<serde_json::Value>()
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    assert_parity(
        &client,
        addr,
        &stranger_jwt,
        "stranger",
        &stranger_token,
        &public_repo_id,
        "owner/open-project",
        true,
        "public repo, total stranger",
    )
    .await;

    // The public bypass is read-only: writes stay denied on both sides.
    let write_info_refs_status = client
        .get(format!(
            "http://{addr}/owner/open-project.git/info/refs?service=git-receive-pack"
        ))
        .basic_auth("stranger", Some(&stranger_token))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        write_info_refs_status,
        reqwest::StatusCode::UNAUTHORIZED,
        "a stranger must not gain WRITE access to a public repo over git protocol"
    );

    let write_settings_status = client
        .put(format!(
            "http://{addr}/api/repositories/{public_repo_id}/settings"
        ))
        .bearer_auth(&stranger_jwt)
        .json(&json!({ "ciEnabled": true }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        write_settings_status,
        reqwest::StatusCode::NOT_FOUND,
        "a stranger must not gain WRITE access to a public repo's settings via the web API (require_role_by_id masks denial as NotFound, same as every other access check in this codebase)"
    );

    // Reader already includes issue creation, so a stranger can open an issue on a public repo (public visibility changes who qualifies as Reader, not what Reader authorizes).
    let create_issue_status = client
        .post(format!(
            "http://{addr}/api/repositories/{public_repo_id}/issues"
        ))
        .bearer_auth(&stranger_jwt)
        .json(&json!({ "title": "found a typo", "description": "", "kind": "bug" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        create_issue_status,
        reqwest::StatusCode::OK,
        "Reader-level access to a public repo must include issue creation, matching the pre-existing meaning of Reader for any explicitly-granted collaborator"
    );
}
