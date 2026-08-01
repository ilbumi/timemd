//! Schedule expansion and day editing over HTTP.

mod support;

use axum::http::StatusCode;
use serde_json::json;
use support::Harness;

/// 2026-08-05 is a Wednesday; 2026-08-08 a Saturday.
const WEDNESDAY: &str = "2026-08-05";
const SATURDAY: &str = "2026-08-08";

async fn with_deep_work(harness: &Harness) {
    let (status, _) = harness
        .request(
            "PUT",
            "/api/schedule/recurring",
            Some(json!([{
                "id": "deep-work",
                "days": "mon-fri",
                "start": "09:00:00",
                "end": "11:00:00",
                "project": "timemd",
                "title": "Deep work",
                "remindBefore": "5m"
            }])),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn recurring_blocks_round_trip_through_the_api() {
    let harness = Harness::new();
    with_deep_work(&harness).await;

    let (status, body) = harness.get("/api/schedule/recurring").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body[0]["id"], "deep-work");
    assert_eq!(body[0]["days"], "mon-fri");
    assert_eq!(body[0]["title"], "Deep work");
    assert_eq!(body[0]["remindBefore"], "5m");
}

#[tokio::test]
async fn a_recurring_block_lands_in_its_own_file() {
    let harness = Harness::new();
    with_deep_work(&harness).await;

    let text = std::fs::read_to_string(harness.store.root().join("schedule/recurring.md"))
        .expect("recurring file exists");
    assert!(
        text.contains("- `deep-work` mon-fri 09:00-11:00 [[timemd]] Deep work !5m"),
        "{text}"
    );
}

#[tokio::test]
async fn expansion_covers_only_matching_weekdays() {
    let harness = Harness::new();
    with_deep_work(&harness).await;

    let (status, body) = harness
        .get("/api/schedule?from=2026-08-05&to=2026-08-08")
        .await;

    assert_eq!(status, StatusCode::OK);
    let dates: Vec<&str> = body
        .as_array()
        .expect("an array")
        .iter()
        .map(|occurrence| occurrence["date"].as_str().expect("a date"))
        .collect();
    assert_eq!(dates, vec!["2026-08-05", "2026-08-06", "2026-08-07"]);
    assert_eq!(body[0]["duration"], "2h");
}

#[tokio::test]
async fn a_skip_removes_one_occurrence_and_can_be_undone() {
    let harness = Harness::new();
    with_deep_work(&harness).await;

    let (status, _) = harness
        .post(
            &format!("/api/days/{WEDNESDAY}/skips"),
            json!({ "id": "deep-work" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (_, body) = harness.get(&format!("/api/days/{WEDNESDAY}")).await;
    assert!(body["planned"].as_array().expect("an array").is_empty());
    assert_eq!(body["skipped"][0], "deep-work");

    let (status, _) = harness
        .delete(&format!("/api/days/{WEDNESDAY}/skips/deep-work"))
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, body) = harness.get(&format!("/api/days/{WEDNESDAY}")).await;
    assert_eq!(body["planned"].as_array().expect("an array").len(), 1);
}

#[tokio::test]
async fn unskipping_something_that_was_not_skipped_is_not_found() {
    let harness = Harness::new();
    let (status, _) = harness
        .delete(&format!("/api/days/{WEDNESDAY}/skips/deep-work"))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn one_off_blocks_merge_with_repeats_in_start_order() {
    let harness = Harness::new();
    with_deep_work(&harness).await;

    let (status, _) = harness
        .post(
            &format!("/api/days/{WEDNESDAY}/blocks"),
            json!({ "start": "12:00:00", "end": "12:30:00", "title": "Lunch" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (_, body) = harness.get(&format!("/api/days/{WEDNESDAY}")).await;
    let titles: Vec<&str> = body["planned"]
        .as_array()
        .expect("an array")
        .iter()
        .map(|block| block["title"].as_str().expect("a title"))
        .collect();
    assert_eq!(titles, vec!["Deep work", "Lunch"]);
    assert_eq!(body["planned"][1]["block"], serde_json::Value::Null);
}

#[tokio::test]
async fn a_one_off_block_can_be_removed() {
    let harness = Harness::new();
    harness
        .post(
            &format!("/api/days/{SATURDAY}/blocks"),
            json!({ "start": "12:00:00", "end": "12:30:00", "title": "Lunch" }),
        )
        .await;

    let (status, _) = harness
        .delete(&format!("/api/days/{SATURDAY}/blocks/0"))
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = harness
        .delete(&format!("/api/days/{SATURDAY}/blocks/0"))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sessions_can_be_added_edited_and_removed_by_hand() {
    let harness = Harness::new();

    let (status, _) = harness
        .post(
            &format!("/api/days/{WEDNESDAY}/sessions"),
            json!({ "start": "14:00:00", "end": "15:30:00", "project": "timemd", "note": "meeting" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (_, body) = harness.get(&format!("/api/days/{WEDNESDAY}")).await;
    assert_eq!(body["tracked"], "1h30m");
    assert_eq!(body["sessions"][0]["duration"], "1h30m");
    assert_eq!(body["sessions"][0]["note"], "meeting");

    let (status, _) = harness
        .patch(
            &format!("/api/days/{WEDNESDAY}/sessions/0"),
            json!({ "start": "14:00:00", "end": "14:30:00", "note": "shorter" }),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, body) = harness.get(&format!("/api/days/{WEDNESDAY}")).await;
    assert_eq!(body["tracked"], "30m");

    let (status, _) = harness
        .delete(&format!("/api/days/{WEDNESDAY}/sessions/0"))
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, body) = harness.get(&format!("/api/days/{WEDNESDAY}")).await;
    assert_eq!(body["tracked"], "0m");
}

#[tokio::test]
async fn editing_a_session_that_is_not_there_is_not_found() {
    let harness = Harness::new();
    let (status, _) = harness
        .delete(&format!("/api/days/{WEDNESDAY}/sessions/3"))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = harness
        .patch(
            &format!("/api/days/{WEDNESDAY}/sessions/3"),
            json!({ "start": "14:00:00", "end": "15:00:00" }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_backwards_or_oversized_range_is_rejected() {
    let harness = Harness::new();

    let (status, _) = harness
        .get("/api/schedule?from=2026-08-08&to=2026-08-01")
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = harness
        .get("/api/schedule?from=2020-01-01&to=2026-08-01")
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_malformed_recurring_block_is_rejected() {
    let harness = Harness::new();

    let (status, _) = harness
        .request(
            "PUT",
            "/api/schedule/recurring",
            Some(json!([{
                "id": "deep-work",
                "days": "funday",
                "start": "09:00:00",
                "end": "11:00:00",
                "title": "Deep work"
            }])),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_day_reports_lines_it_could_not_parse() {
    let harness = Harness::new();
    let path = harness.store.root().join("days/2026/2026-08-05.md");
    std::fs::create_dir_all(path.parent().expect("has a parent")).expect("creates dir");
    std::fs::write(
        &path,
        "---\ndate: 2026-08-05\n---\n\n## Sessions\n\n- utter nonsense\n",
    )
    .expect("writes");

    let (_, body) = harness.get(&format!("/api/days/{WEDNESDAY}")).await;
    assert_eq!(body["problems"].as_array().expect("an array").len(), 1);
}
