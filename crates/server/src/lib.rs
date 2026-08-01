//! HTTP surface for timemd.
//!
//! The API is deliberately small: it is the same surface the CLI and the MCP
//! server drive, so every endpoint added here is an interface that has to be
//! kept working in four places at once.

mod health;

use axum::Router;
use axum::routing::get;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;

/// Builds the complete HTTP router.
///
/// Taking no arguments keeps tests cheap — they can `oneshot` this directly
/// without binding a socket.
pub fn router() -> Router {
    Router::new()
        .route("/api/health", get(health::health))
        .layer(TraceLayer::new_for_http())
}

/// Serves the app on an already-bound listener.
///
/// The caller binds so that it owns the address-resolution failure mode, and so
/// tests can serve on an ephemeral port.
pub async fn serve(listener: TcpListener) -> std::io::Result<()> {
    axum::serve(listener, router()).await
}
