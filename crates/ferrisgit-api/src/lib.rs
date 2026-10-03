pub mod auth_middleware;
pub mod authz;
pub mod client_ip;
pub mod config;
pub mod error;
pub mod first_setup_lock;
pub mod git_auth;
pub mod log_retention_sweep;
pub mod login_rate_limiter;
pub mod mfa_rate_limiter;
pub mod pending_account_sweep;
pub mod routes;
pub mod state;

use std::path::Path;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{HeaderName, HeaderValue};
use axum::response::IntoResponse;
use axum::routing::get;
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

/// The JWT lives in localStorage, so an XSS bug could read it; the CSP limits the damage by locking down
/// object-src, base-uri and frame-ancestors. script-src falls back to default-src 'self'. style-src needs
/// 'unsafe-inline' because Angular injects component styles as <style> tags. X-Frame-Options repeats
/// frame-ancestors for browsers without CSP.
fn security_headers() -> [(HeaderName, HeaderValue); 4] {
    [
        (
            HeaderName::from_static("content-security-policy"),
            HeaderValue::from_static(
                "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; object-src 'none'; base-uri 'self'; frame-ancestors 'none'",
            ),
        ),
        (
            HeaderName::from_static("x-frame-options"),
            HeaderValue::from_static("DENY"),
        ),
        (
            HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ),
        (
            HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("no-referrer"),
        ),
    ]
}

/// Git smart-HTTP and the SPA share the root URL space and only a `.git` path segment tells them apart. A wildcard
/// route would win over the SPA fallback and swallow its URLs, hence the manual dispatch in the fallback.
pub fn build_router(state: AppState, static_dir: &Path) -> Router {
    // No trailing-slash redirect: /docs and /docs/<section> are asset folders but also SPA routes, and with no
    // index.html in them they have to fall through to the shell.
    let spa_fallback = ServeDir::new(static_dir)
        .redirect_to_trailing_slash(false)
        .not_found_service(ServeFile::new(static_dir.join("index.html")));

    let mut router = Router::new()
        .route("/health", get(routes::health))
        .route("/robots.txt", get(routes::public::robots_txt))
        .nest("/api", routes::api_router(state.clone()))
        .fallback(move |State(state): State<AppState>, req: Request| {
            let spa_fallback = spa_fallback.clone();
            async move {
                if routes::git_http::is_git_request_path(req.uri().path()) {
                    routes::git_http::git_smart_http(state, req).await
                } else {
                    // The docs files have no content hash either.
                    let is_docs_asset = req.uri().path().starts_with("/docs/");
                    match spa_fallback.oneshot(req).await {
                        Ok(mut res) => {
                            // The shell has no content hash in its name. Without Cache-Control, heuristic caching can keep
                            // serving an old shell that points at bundles from a previous deploy.
                            let is_html = res
                                .headers()
                                .get(axum::http::header::CONTENT_TYPE)
                                .is_some_and(|v| v.as_bytes().starts_with(b"text/html"));
                            if is_html || is_docs_asset {
                                res.headers_mut().insert(
                                    axum::http::header::CACHE_CONTROL,
                                    HeaderValue::from_static("no-cache"),
                                );
                            }
                            res.into_response()
                        }
                        Err(never) => match never {},
                    }
                }
            }
        })
        .layer(TraceLayer::new_for_http());
    for (name, value) in security_headers() {
        router = router.layer(SetResponseHeaderLayer::overriding(name, value));
    }
    router
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            routes::public::robots_tag,
        ))
        // Merged after the layer on purpose: robots_tag hits the database on every response, and liveness shouldn't
        // fail during a database outage.
        .merge(probe_routes())
        .with_state(state)
}

/// Kubernetes probes: `/healthz` says the process runs, `/readyz` that the database answers.
fn probe_routes() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(routes::health))
        .route("/readyz", get(routes::ready))
}
