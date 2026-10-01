use std::net::SocketAddr;
use std::path::PathBuf;

use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;

/// Detached: runs for the life of the process. A failed tick logs and the loop continues.
fn spawn_metrics_snapshot_timer(state: &AppState) {
    let record_metrics_snapshot = state.record_metrics_snapshot.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60 * 60));
        loop {
            interval.tick().await;
            if let Err(e) = record_metrics_snapshot.execute().await {
                tracing::warn!("failed to record metrics snapshot: {e}");
            }
        }
    });
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let config = Config::from_env();

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await
        .expect("failed to connect to postgres");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    let state = AppState::new(pool, config.clone()).await;

    spawn_metrics_snapshot_timer(&state);

    BootstrapAdminUseCase::new(state.users.clone(), state.hasher.clone())
        .execute(
            config.bootstrap_admin_username.clone(),
            config.bootstrap_admin_password.clone(),
        )
        .await
        .expect("bootstrap admin failed");

    // The token issuer holds the JWT TTL in memory and only gets a new value when an admin saves settings.
    // Without this, its hardcoded default would replace the configured TTL on every restart.
    let settings = state
        .system_settings
        .get()
        .await
        .expect("failed to read system settings");
    state
        .token_issuer
        .set_ttl_hours(settings.jwt_ttl_hours as i64);
    tracing::info!(
        jwt_ttl_hours = settings.jwt_ttl_hours,
        "applied the persisted JWT TTL"
    );

    let static_dir = PathBuf::from(&config.static_dir);
    let app = build_router(state, &static_dir);

    let addr: SocketAddr = config.bind_addr.parse().expect("invalid BIND_ADDR");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind");
    tracing::info!("listening on {addr}");
    // Populates `ConnectInfo<SocketAddr>`, the per-IP rate limiters' key.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("server error");
}
