//! Routing between the JSON API and the embedded single-page app.
//!
//! These assertions avoid depending on whether the frontend has been built into
//! this particular binary: what matters is which of the two fallbacks a path
//! reaches, not what the shell contains.

mod support;

use axum::http::StatusCode;
use support::Harness;

#[tokio::test]
async fn an_unknown_api_path_answers_as_json() {
    let harness = Harness::new();
    let (status, body) = harness.get("/api/nope").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|message| message.contains("/api/nope")),
        "expected a JSON error naming the path, got {body}"
    );
}

#[tokio::test]
async fn an_unknown_api_path_under_a_real_prefix_still_answers_as_json() {
    let harness = Harness::new();
    let (status, body) = harness.get("/api/projects/timemd/extra").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        body["error"].is_string(),
        "expected a JSON error, got {body}"
    );
}

#[tokio::test]
async fn a_client_route_falls_through_to_the_app_shell() {
    let harness = Harness::new();
    let (_, body) = harness.get("/projects").await;

    // The shell is HTML, so the harness cannot parse it as JSON. Either way it
    // must not be the API's error envelope.
    assert!(
        body.get("error").is_none(),
        "a client route reached the API fallback: {body}"
    );
}
