mod common;

use common::http::{create_user, get, login};

use sqlx::PgPool;

/// The `AdminUser` guard rejects a non-admin on every handler.
#[sqlx::test]
async fn admin_metrics_endpoints_are_admin_gated_and_shaped_as_expected(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;

    let client = reqwest::Client::new();

    let admin_jwt = login(&client, addr, "admin", "adminpassword123").await;

    create_user(&client, addr, &admin_jwt, "regular").await;

    let regular_jwt = login(&client, addr, "regular", "password12345").await;

    for path in [
        "/api/admin/stats",
        "/api/admin/metrics/history",
        "/api/admin/health",
    ] {
        let res = client
            .get(format!("http://{addr}{path}"))
            .bearer_auth(&regular_jwt)
            .send()
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            reqwest::StatusCode::UNAUTHORIZED,
            "non-admin must be rejected from {path}, got {}",
            res.status()
        );
    }

    let stats_res = get(&client, addr, &admin_jwt, "/admin/stats").await;
    assert_eq!(stats_res.status(), reqwest::StatusCode::OK);
    let stats: serde_json::Value = stats_res.json().await.unwrap();
    assert!(
        stats.get("totalUsers").is_some(),
        "stats missing totalUsers: {stats}"
    );
    assert!(
        stats.get("totalRepositories").is_some(),
        "stats missing totalRepositories: {stats}"
    );
    assert!(
        stats.get("pipelinesLast7Days").is_some(),
        "stats missing pipelinesLast7Days: {stats}"
    );

    let health_res = get(&client, addr, &admin_jwt, "/admin/health").await;
    assert_eq!(health_res.status(), reqwest::StatusCode::OK);
    let health: serde_json::Value = health_res.json().await.unwrap();
    assert!(
        health.get("database").is_some(),
        "health missing database: {health}"
    );
    assert!(
        health.get("storage").is_some(),
        "health missing storage: {health}"
    );
    assert!(
        health.get("uptimeSeconds").is_some(),
        "health missing uptimeSeconds: {health}"
    );

    let history_res = get(&client, addr, &admin_jwt, "/admin/metrics/history").await;
    assert_eq!(history_res.status(), reqwest::StatusCode::OK);
    let history: serde_json::Value = history_res.json().await.unwrap();
    assert!(
        history.is_array(),
        "history response should be a JSON array: {history}"
    );

    let days_zero_res = get(&client, addr, &admin_jwt, "/admin/metrics/history?days=0").await;
    assert_eq!(
        days_zero_res.status(),
        reqwest::StatusCode::OK,
        "days=0 must be clamped rather than rejected"
    );

    let days_huge_res = get(
        &client,
        addr,
        &admin_jwt,
        "/admin/metrics/history?days=9999",
    )
    .await;
    assert_eq!(
        days_huge_res.status(),
        reqwest::StatusCode::OK,
        "days=9999 must be clamped rather than rejected"
    );
}
