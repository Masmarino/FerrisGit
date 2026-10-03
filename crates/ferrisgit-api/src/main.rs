use std::net::SocketAddr;
use std::path::PathBuf;

use tokio::sync::watch;

use ferrisgit_api::{
    build_router, config::Config, log_retention_sweep, pending_account_sweep, state::AppState,
};
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;

/// Runs for the life of the process. A failed tick is logged and the loop carries on.
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

/// Resolves on Ctrl-C or, on Unix, SIGTERM (what `docker stop` and Kubernetes send).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for Ctrl-C");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to listen for SIGTERM")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

/// Long-lived connections (the pipeline event streams) never end by themselves, so after this delay the server
/// stops waiting for them.
const SHUTDOWN_GRACE: std::time::Duration = std::time::Duration::from_secs(10);

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
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let log_retention_sweep = log_retention_sweep::spawn(
        state.purge_expired_job_logs.clone(),
        log_retention_sweep::SWEEP_EVERY,
        shutdown_rx.clone(),
    );
    let pending_account_sweep = pending_account_sweep::spawn(
        state.sweep_pending_accounts(),
        pending_account_sweep::SWEEP_EVERY,
        shutdown_rx.clone(),
    );

    BootstrapAdminUseCase::new(state.users.clone(), state.hasher.clone())
        .execute(
            config.bootstrap_admin_username.clone(),
            config.bootstrap_admin_password.clone(),
        )
        .await
        .expect("bootstrap admin failed");

    // The token issuer only learns the JWT TTL when an admin saves settings, so seed it here or every restart
    // falls back to its hardcoded default.
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
    tokio::spawn(async move {
        shutdown_signal().await;
        tracing::info!("shutting down");
        shutdown_tx.send_replace(true);
    });
    let mut graceful = shutdown_rx.clone();
    // Fills ConnectInfo<SocketAddr>, which the per-IP limiters key on.
    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        graceful.wait_for(|stopped| *stopped).await.ok();
    });
    let mut grace_deadline = shutdown_rx;
    tokio::select! {
        result = async { server.await } => result.expect("server error"),
        _ = async {
            grace_deadline.wait_for(|stopped| *stopped).await.ok();
            tokio::time::sleep(SHUTDOWN_GRACE).await;
        } => tracing::warn!("closing with connections still open"),
    }
    log_retention_sweep.await.ok();
    pending_account_sweep.await.ok();
}
