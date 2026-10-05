//! `/api/version`: the running release, which the app's navigation shows at its foot.

mod common;

use serde_json::{Value, json};
use sqlx::PgPool;

#[sqlx::test]
async fn the_version_is_the_crate_version_and_needs_no_session(pool: PgPool) {
    let server = common::spawn_server(pool).await;

    let res = server
        .client
        .get(server.url("/version"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    assert_eq!(
        res.json::<Value>().await.unwrap(),
        json!({ "version": env!("CARGO_PKG_VERSION") })
    );
}
