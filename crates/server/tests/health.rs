//! The health endpoint is the smoke test the deploy story leans on: if this
//! answers over Tailscale, the binary is up and the frontend is reachable.

mod support;

use axum::http::StatusCode;
use support::Harness;

#[tokio::test]
async fn health_reports_ok_and_version() {
    let harness = Harness::new();
    let (status, body) = harness.get("/api/health").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
}
