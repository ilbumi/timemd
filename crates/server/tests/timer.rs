//! The timer over HTTP, driven by a clock the test controls.

mod support;

use axum::http::StatusCode;
use serde_json::json;
use support::{Harness, instant};

#[tokio::test]
async fn an_idle_timer_reports_nothing_running() {
    let harness = Harness::new();
    let (status, body) = harness.get("/api/timer").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["active"].is_null());
    assert_eq!(body["completedToday"], 0);
    assert_eq!(body["trackedToday"], "0m");
}

#[tokio::test]
async fn starts_a_focus_session_against_a_project() {
    let harness = Harness::new();
    harness
        .post("/api/projects", json!({ "name": "timemd" }))
        .await;

    let (status, body) = harness
        .post(
            "/api/timer/start",
            json!({ "project": "timemd", "note": "file store" }),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["active"]["kind"], "focus");
    assert_eq!(body["active"]["project"], "timemd");
    assert_eq!(body["active"]["note"], "file store");
    assert_eq!(body["active"]["duration"], "25m");
    assert_eq!(body["active"]["remainingSeconds"], 25 * 60);
}

#[tokio::test]
async fn a_started_session_is_visible_in_the_state_file() {
    let harness = Harness::new();
    harness
        .post("/api/timer/start", json!({ "note": "reading" }))
        .await;

    let text = std::fs::read_to_string(harness.store.root().join("state/active.md"))
        .expect("state file exists");

    assert!(text.contains("kind: focus"), "{text}");
    assert!(text.contains("duration: 25m"), "{text}");
    assert!(text.contains("note: reading"), "{text}");
}

#[tokio::test]
async fn the_countdown_shrinks_as_the_clock_advances() {
    let harness = Harness::new();
    harness.post("/api/timer/start", json!({})).await;

    harness.clock.set(instant(2026, 8, 1, 9, 10));
    let (_, body) = harness.get("/api/timer").await;

    assert_eq!(body["active"]["remainingSeconds"], 15 * 60);
}

#[tokio::test]
async fn reading_the_timer_settles_a_finished_session() {
    let harness = Harness::new();
    harness
        .post("/api/timer/start", json!({ "note": "done" }))
        .await;

    harness.clock.set(instant(2026, 8, 1, 9, 30));
    let (_, body) = harness.get("/api/timer").await;

    assert!(
        body["active"].is_null(),
        "a due session must not read as running"
    );
    assert_eq!(body["completedToday"], 1);
    assert_eq!(body["trackedToday"], "25m");

    let text = std::fs::read_to_string(harness.store.root().join("days/2026/2026-08-01.md"))
        .expect("day file exists");
    assert!(text.contains("- 09:00-09:25 (25m) done"), "{text}");
}

#[tokio::test]
async fn stopping_early_logs_only_the_time_worked() {
    let harness = Harness::new();
    harness
        .post("/api/timer/start", json!({ "note": "partial" }))
        .await;

    harness.clock.set(instant(2026, 8, 1, 9, 10));
    let (status, body) = harness.post("/api/timer/stop", json!({})).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["active"].is_null());
    assert_eq!(body["trackedToday"], "10m");
    assert_eq!(body["stopped"], "logged");
}

#[tokio::test]
async fn cancelling_leaves_nothing_behind() {
    let harness = Harness::new();
    harness
        .post("/api/timer/start", json!({ "note": "mistake" }))
        .await;

    harness.clock.set(instant(2026, 8, 1, 9, 10));
    let (status, body) = harness.post("/api/timer/cancel", json!({})).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["active"].is_null());
    assert_eq!(body["trackedToday"], "0m");
    assert!(
        !harness
            .store
            .root()
            .join("days/2026/2026-08-01.md")
            .exists()
    );
}

/// The web app starts a break against the project it is a break *from*, so that
/// the same block can be offered again when the break ends. That must not turn
/// the break into tracked time.
#[tokio::test]
async fn a_break_runs_but_is_never_logged() {
    let harness = Harness::new();
    harness
        .post("/api/projects", json!({ "name": "timemd" }))
        .await;

    let (status, body) = harness
        .post(
            "/api/timer/start",
            json!({ "kind": "short_break", "project": "timemd", "note": "file store" }),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["active"]["kind"], "short_break");
    assert_eq!(body["active"]["duration"], "5m");
    assert_eq!(body["active"]["project"], "timemd");
    assert_eq!(body["active"]["note"], "file store");

    harness.clock.set(instant(2026, 8, 1, 9, 10));
    let (_, body) = harness.get("/api/timer").await;
    assert!(body["active"].is_null());
    assert_eq!(body["trackedToday"], "0m");
    assert!(
        !harness
            .store
            .root()
            .join("days/2026/2026-08-01.md")
            .exists()
    );
}

#[tokio::test]
async fn starting_again_logs_what_was_interrupted() {
    let harness = Harness::new();
    harness
        .post("/api/timer/start", json!({ "note": "first" }))
        .await;

    harness.clock.set(instant(2026, 8, 1, 9, 10));
    let (_, body) = harness
        .post("/api/timer/start", json!({ "note": "second" }))
        .await;

    assert_eq!(body["active"]["note"], "second");
    assert_eq!(body["trackedToday"], "10m");
}

#[tokio::test]
async fn accepts_an_explicit_duration() {
    let harness = Harness::new();
    let (status, body) = harness
        .post("/api/timer/start", json!({ "duration": "50m" }))
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["active"]["duration"], "50m");
}

#[tokio::test]
async fn rejects_a_malformed_duration_or_project() {
    let harness = Harness::new();

    let (status, _) = harness
        .post("/api/timer/start", json!({ "duration": "ages" }))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = harness
        .post("/api/timer/start", json!({ "duration": "0m" }))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = harness
        .post("/api/timer/start", json!({ "project": "Not A Slug" }))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn stopping_under_a_minute_does_not_log() {
    let harness = Harness::new();
    harness
        .post("/api/timer/start", json!({ "note": "too soon" }))
        .await;

    let (status, body) = harness.post("/api/timer/stop", json!({})).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["active"].is_null());
    assert_eq!(body["trackedToday"], "0m");
    assert_eq!(body["stopped"], "tooShort");
    assert!(
        !harness
            .store
            .root()
            .join("days/2026/2026-08-01.md")
            .exists()
    );
}

#[tokio::test]
async fn stopping_when_nothing_runs_is_harmless() {
    let harness = Harness::new();
    let (status, body) = harness.post("/api/timer/stop", json!({})).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["active"].is_null());
    assert_eq!(body["stopped"], "idle");
}

/// The property the whole server-authoritative design exists for: a phone that
/// was asleep for hours must not turn a 25-minute pomodoro into a six-hour one.
#[tokio::test]
async fn a_session_finished_hours_ago_is_logged_at_its_planned_length() {
    let harness = Harness::new();
    harness
        .post("/api/timer/start", json!({ "note": "slept through it" }))
        .await;

    harness.clock.set(instant(2026, 8, 1, 15, 0));
    let (_, body) = harness.get("/api/timer").await;

    assert_eq!(body["trackedToday"], "25m");

    let text = std::fs::read_to_string(harness.store.root().join("days/2026/2026-08-01.md"))
        .expect("day file exists");
    assert!(
        text.contains("- 09:00-09:25 (25m) slept through it"),
        "{text}"
    );
}

#[tokio::test]
async fn suggests_a_long_break_after_the_fourth_pomodoro() {
    let harness = Harness::new();

    for hour in 9..13 {
        harness.clock.set(instant(2026, 8, 1, hour, 0));
        harness.post("/api/timer/start", json!({})).await;
        harness.clock.set(instant(2026, 8, 1, hour, 30));
        harness.get("/api/timer").await;
    }

    let (_, body) = harness.get("/api/timer").await;
    assert_eq!(body["completedToday"], 4);
    assert_eq!(body["nextBreak"], "15m");
    assert_eq!(body["nextBreakKind"], "long_break");
}
