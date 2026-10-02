mod common;

use sqlx::PgPool;

/// index.html has no content hash, so without no-cache a browser could keep an old shell pointing at old bundles. Hashed assets are left alone.
#[sqlx::test]
async fn the_html_shell_is_never_cached_but_a_hashed_asset_is_left_alone(pool: PgPool) {
    let app = common::spawn_app(pool).await;
    std::fs::write(app.static_dir.join("main-ABC123.js"), "console.log(1)").unwrap();
    let addr = app.addr;

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
