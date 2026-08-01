//! The health endpoint is the smoke test the deploy story leans on: if this
//! answers over Tailscale, the binary is up and the frontend is reachable.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn health_reports_ok_and_version() {
    let response = timemd_server::router(state())
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);

    let body = response
        .into_body()
        .collect()
        .await
        .expect("body collects")
        .to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("body is json");

    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn unknown_api_route_is_not_found() {
    let response = timemd_server::router(state())
        .oneshot(
            Request::builder()
                .uri("/api/nope")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// A router needs a store; these tests do not care what is in it.
fn state() -> timemd_server::state::AppState {
    let directory = Box::leak(Box::new(tempfile::tempdir().expect("temp dir")));
    timemd_server::state::AppState::new(
        std::sync::Arc::new(timemd_core::Store::new(directory.path())),
        timemd_server::state::Clock::System,
    )
}
