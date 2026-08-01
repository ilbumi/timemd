//! `/api/settings`, and what it refuses to touch.

mod support;

use axum::http::StatusCode;
use serde_json::json;
use support::Harness;

#[tokio::test]
async fn reads_the_defaults_before_anything_is_written() {
    let harness = Harness::new();
    let (status, body) = harness.get("/api/settings").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["focus"], "25m");
    assert_eq!(body["shortBreak"], "5m");
    assert_eq!(body["longBreak"], "15m");
    assert_eq!(body["longBreakEvery"], 4);
    assert_eq!(body["remindBefore"], "5m");
}

#[tokio::test]
async fn writes_only_the_durations_it_is_given() {
    let harness = Harness::new();
    let (status, body) = harness
        .put("/api/settings", json!({ "focus": "50m" }))
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["focus"], "50m");
    assert_eq!(
        body["shortBreak"], "5m",
        "an omitted key must not be cleared"
    );

    let (_, reread) = harness.get("/api/settings").await;
    assert_eq!(reread["focus"], "50m");
}

#[tokio::test]
async fn rejects_a_length_it_could_not_run() {
    let harness = Harness::new();
    for body in [json!({ "focus": "0m" }), json!({ "longBreak": "ages" })] {
        let (status, _) = harness.put("/api/settings", body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}

/// The timezone reinterprets every time already stored in the tree, so it is
/// not something the API lets a phone change by accident.
#[tokio::test]
async fn leaves_the_timezone_alone() {
    let harness = Harness::new();
    let (_, before) = harness.get("/api/settings").await;

    let (status, after) = harness
        .put("/api/settings", json!({ "timezone": "Pacific/Kiritimati" }))
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(after["timezone"], before["timezone"]);
}

#[tokio::test]
async fn a_write_preserves_agent_authored_keys() {
    let harness = Harness::new();
    let path = harness.store.root().join("settings.md");
    std::fs::write(
        &path,
        "---\nfocus: 25m\nagent_key: kept\n---\n\n# Settings\n",
    )
    .expect("writes");

    harness
        .put("/api/settings", json!({ "focus": "30m" }))
        .await;

    let text = std::fs::read_to_string(&path).expect("reads");
    assert!(text.contains("agent_key: kept"), "{text}");
    assert!(text.contains("focus: 30m"), "{text}");
}
