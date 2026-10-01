use ferrisgit_api::{build_router, config::Config, state::AppState};
use sqlx::PgPool;

/// index.html has no content hash, so heuristic caching could serve a stale shell that references old bundles. It must be `no-cache`, while a hashed asset is left alone.
#[sqlx::test]
async fn the_html_shell_is_never_cached_but_a_hashed_asset_is_left_alone(pool: PgPool) {
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

    let storage_dir = tempfile::tempdir().unwrap();
    let static_dir = tempfile::tempdir().unwrap();
    std::fs::write(static_dir.path().join("index.html"), "<html></html>").unwrap();
    std::fs::write(static_dir.path().join("main-ABC123.js"), "console.log(1)").unwrap();

    let config = Config {
        database_url: String::new(),
        jwt_secret: "test-secret-that-is-at-least-32-characters-long".to_string(),
        storage_root: storage_dir.path().to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:0".to_string(),
        static_dir: static_dir.path().to_string_lossy().to_string(),
        bootstrap_admin_username: None,
        bootstrap_admin_password: None,
        settings_encryption_key: [b'k'; 32],
        public_url: "http://localhost:4200".to_string(),
        trusted_proxy_cidrs: vec![],
    };

    let mut state = AppState::new(pool, config.clone()).await;
    state.mfa_enforced = false; // these tests are not about MFA: they log in with a plain session
    let app = build_router(state, static_dir.path());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();

    let shell_res = client.get(format!("http://{addr}/")).send().await.unwrap();
    assert_eq!(
        shell_res.headers().get("cache-control").unwrap(),
        "no-cache"
    );

    let unmatched_route_res = client
        .get(format!("http://{addr}/some/deep/spa/route"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        unmatched_route_res.headers().get("cache-control").unwrap(),
        "no-cache",
        "the not-found fallback also serves the html shell and must not be cached either"
    );

    let asset_res = client
        .get(format!("http://{addr}/main-ABC123.js"))
        .send()
        .await
        .unwrap();
    assert!(
        asset_res.headers().get("cache-control").is_none(),
        "a real hashed asset must be left alone, not forced to no-cache"
    );
}
