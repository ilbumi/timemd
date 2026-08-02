//! Reports over HTTP.

mod support;

use axum::http::StatusCode;
use serde_json::json;
use support::Harness;

async fn logged(harness: &Harness, date: &str, start: &str, end: &str, project: Option<&str>) {
    harness
        .post(
            &format!("/api/days/{date}/sessions"),
            json!({ "start": start, "end": end, "project": project, "note": "work" }),
        )
        .await;
}

#[tokio::test]
async fn groups_by_project_largest_first_by_default() {
    let harness = Harness::new();
    logged(
        &harness,
        "2026-08-01",
        "09:00:00",
        "11:00:00",
        Some("timemd"),
    )
    .await;
    logged(
        &harness,
        "2026-08-01",
        "11:30:00",
        "12:00:00",
        Some("admin"),
    )
    .await;
    logged(
        &harness,
        "2026-08-03",
        "09:00:00",
        "10:00:00",
        Some("timemd"),
    )
    .await;

    let (status, body) = harness
        .get("/api/reports?from=2026-08-01&to=2026-08-31")
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], "3h30m");
    assert_eq!(body["groupBy"], "project");
    assert_eq!(body["buckets"][0]["key"], "timemd");
    assert_eq!(body["buckets"][0]["tracked"], "3h");
    // Nothing was scheduled, so the plan is a zero rather than absent or null.
    assert_eq!(body["planned"], "0m");
    assert_eq!(body["buckets"][0]["planned"], "0m");
    assert_eq!(body["buckets"][0]["sessions"], 2);
    assert_eq!(body["buckets"][1]["key"], "admin");
}

async fn planned(harness: &Harness, date: &str, start: &str, end: &str, project: Option<&str>) {
    harness
        .post(
            &format!("/api/days/{date}/blocks"),
            json!({ "start": start, "end": end, "project": project, "title": "Deep work" }),
        )
        .await;
}

#[tokio::test]
async fn a_report_carries_what_was_planned_beside_what_was_tracked() {
    let harness = Harness::new();
    planned(
        &harness,
        "2026-08-01",
        "09:00:00",
        "11:00:00",
        Some("timemd"),
    )
    .await;
    logged(
        &harness,
        "2026-08-01",
        "09:00:00",
        "10:00:00",
        Some("timemd"),
    )
    .await;

    let (status, body) = harness
        .get("/api/reports?from=2026-08-01&to=2026-08-31")
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], "1h");
    assert_eq!(body["planned"], "2h");
    assert_eq!(body["buckets"][0]["key"], "timemd");
    assert_eq!(body["buckets"][0]["tracked"], "1h");
    assert_eq!(body["buckets"][0]["planned"], "2h");
}

#[tokio::test]
async fn a_project_that_was_only_planned_still_gets_a_row() {
    let harness = Harness::new();
    planned(
        &harness,
        "2026-08-01",
        "09:00:00",
        "11:00:00",
        Some("russian"),
    )
    .await;

    let (status, body) = harness
        .get("/api/reports?from=2026-08-01&to=2026-08-31")
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], "0m");
    assert_eq!(body["buckets"][0]["key"], "russian");
    assert_eq!(body["buckets"][0]["tracked"], "0m");
    assert_eq!(body["buckets"][0]["planned"], "2h");
    assert_eq!(body["buckets"][0]["sessions"], 0);
}

#[tokio::test]
async fn groups_by_day_in_date_order() {
    let harness = Harness::new();
    logged(
        &harness,
        "2026-08-03",
        "09:00:00",
        "10:00:00",
        Some("timemd"),
    )
    .await;
    logged(
        &harness,
        "2026-08-01",
        "09:00:00",
        "11:00:00",
        Some("timemd"),
    )
    .await;

    let (status, body) = harness
        .get("/api/reports?from=2026-08-01&to=2026-08-31&groupBy=day")
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["buckets"][0]["key"], "2026-08-01");
    assert_eq!(body["buckets"][1]["key"], "2026-08-03");
}

#[tokio::test]
async fn time_against_no_project_is_its_own_bucket() {
    let harness = Harness::new();
    logged(&harness, "2026-08-01", "09:00:00", "09:45:00", None).await;

    let (_, body) = harness
        .get("/api/reports?from=2026-08-01&to=2026-08-31")
        .await;

    assert_eq!(body["buckets"][0]["key"], serde_json::Value::Null);
    assert_eq!(body["buckets"][0]["tracked"], "45m");
}

#[tokio::test]
async fn an_empty_range_totals_zero() {
    let harness = Harness::new();
    let (status, body) = harness
        .get("/api/reports?from=2026-08-01&to=2026-08-31")
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], "0m");
    assert_eq!(body["planned"], "0m");
    assert!(body["buckets"].as_array().expect("an array").is_empty());
}

#[tokio::test]
async fn a_backwards_or_oversized_range_is_rejected() {
    let harness = Harness::new();

    let (status, _) = harness
        .get("/api/reports?from=2026-08-31&to=2026-08-01")
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = harness
        .get("/api/reports?from=2020-01-01&to=2026-08-01")
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn an_unknown_grouping_is_rejected() {
    let harness = Harness::new();
    let (status, _) = harness
        .get("/api/reports?from=2026-08-01&to=2026-08-31&groupBy=colour")
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
