//! HTTP surface for timemd.
//!
//! The API is deliberately small: it is the same surface the CLI and the MCP
//! server drive, so every endpoint added here is an interface that has to be
//! kept working in four places at once.
//!
//! Handlers call the synchronous store directly rather than through
//! `spawn_blocking`. On files this size the reads are sub-millisecond, and the
//! indirection would buy nothing but noise.

mod assets;
mod error;
mod health;
mod parse;
mod projects;
pub mod push;
mod report;
mod schedule;
mod settings;
pub mod state;
mod ticker;
mod timer;

use axum::Router;
use axum::extract::OriginalUri;
use axum::routing::get;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

/// Builds the complete HTTP router.
///
/// The API is nested with its own fallback so an unknown `/api` path answers
/// with a JSON 404 rather than the single-page shell — a mistyped endpoint
/// should look like a mistake, not like a route the client forgot to handle.
pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(health::health))
        .merge(projects::routes())
        .merge(timer::routes())
        .merge(schedule::routes())
        .merge(report::routes())
        .merge(settings::routes())
        .merge(push::routes())
        .fallback(unknown_endpoint);

    Router::new()
        .nest("/api", api)
        .fallback(assets::serve)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Nesting strips the `/api` prefix before the fallback sees it, so the error
/// is built from the original URI — it should name what the client asked for.
async fn unknown_endpoint(OriginalUri(uri): OriginalUri) -> crate::error::ApiError {
    crate::error::ApiError::not_found(format!("no endpoint at {}", uri.path()))
}

/// Serves the app on an already-bound listener.
///
/// The caller binds so that it owns the address-resolution failure mode, and so
/// tests can serve on an ephemeral port.
pub async fn serve(listener: TcpListener, state: AppState) -> std::io::Result<()> {
    // Owned here rather than detached, so the ticker's lifetime is exactly the
    // server's — a stray task outliving its store would be writing into a
    // directory nobody is watching any more.
    let ticker = tokio::spawn(ticker::run(state.clone()));
    let outcome = axum::serve(listener, router(state)).await;
    ticker.abort();
    outcome
}
