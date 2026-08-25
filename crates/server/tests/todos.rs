//! `/api/todos`, and the two places a todo shows up elsewhere.

mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::Harness;

/// Creates one todo and hands back its id, which is the handle every other call
/// here takes.
async fn create(harness: &Harness, body: Value) -> Value {
    let (status, todo) = harness.post("/api/todos", body).await;
    assert_eq!(status, StatusCode::CREATED, "{todo}");
    todo
}

fn id_of(todo: &Value) -> String {
    todo["id"].as_str().expect("an id was minted").to_owned()
}

#[tokio::test]
async fn a_created_todo_lands_on_disk_as_obsidian_tasks() {
    let harness = Harness::new();
    harness
        .post("/api/projects", json!({ "name": "timemd" }))
        .await;

    let todo = create(
        &harness,
        json!({
            "description": "Draft the release notes",
            "project": "timemd",
            "priority": "high",
            "scheduled": "2026-08-30 14:00",
            "due": "2026-08-31",
        }),
    )
    .await;

    let text = std::fs::read_to_string(harness.store.todos_path()).expect("the file was written");
    let expected = format!(
        "- [ ] [[timemd]] Draft the release notes ⏫ 🆔 {} ➕ 2026-08-01 ⏳ 2026-08-30 14:00 📅 2026-08-31",
        id_of(&todo)
    );
    assert!(text.contains(&expected), "{text}");
}

/// The created date is stamped for the caller, because a todo with no history
/// cannot be sorted by age and every surface would otherwise invent one.
#[tokio::test]
async fn a_created_todo_is_stamped_with_today() {
    let harness = Harness::new();
    let todo = create(&harness, json!({ "description": "Something" })).await;
    assert_eq!(todo["created"], "2026-08-01");
    assert_eq!(todo["status"], "open");
    assert_eq!(todo["priority"], "normal");
}

#[tokio::test]
async fn ticks_retitles_and_reads_back_one_todo() {
    let harness = Harness::new();
    let id = id_of(&create(&harness, json!({ "description": "Draft the notes" })).await);

    let (status, updated) = harness
        .patch(
            &format!("/api/todos/{id}"),
            json!({
                "status": "done",
                "description": "Draft the release notes",
                "done": "2026-08-01",
            }),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["status"], "done");
    assert_eq!(updated["description"], "Draft the release notes");
    assert_eq!(updated["done"], "2026-08-01");

    let (status, read) = harness.get(&format!("/api/todos/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read["description"], "Draft the release notes");
}

/// An absent key means "leave it"; an explicit `null` means "clear it". Without
/// the distinction, a request meaning "drop the deadline" is a silent no-op.
#[tokio::test]
async fn clears_a_due_date_with_null_and_leaves_it_when_absent() {
    let harness = Harness::new();
    let id = id_of(
        &create(
            &harness,
            json!({ "description": "Ship it", "due": "2026-08-31" }),
        )
        .await,
    );

    let (_, untouched) = harness
        .patch(&format!("/api/todos/{id}"), json!({ "priority": "low" }))
        .await;
    assert_eq!(untouched["due"], "2026-08-31");
    assert_eq!(untouched["priority"], "low");

    let (_, cleared) = harness
        .patch(&format!("/api/todos/{id}"), json!({ "due": null }))
        .await;
    assert_eq!(cleared["due"], Value::Null);
}

#[tokio::test]
async fn rejects_a_description_it_could_not_read_back() {
    let harness = Harness::new();

    for description in ["", "   ", "has a 📅 in it", "[[timemd]] leading link"] {
        let (status, body) = harness
            .post("/api/todos", json!({ "description": description }))
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{description:?}: {body}");
    }
}

#[tokio::test]
async fn rejects_an_unreadable_date_before_touching_the_file() {
    let harness = Harness::new();
    let (status, _) = harness
        .post(
            "/api/todos",
            json!({ "description": "Ship it", "due": "next tuesday" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (_, list) = harness.get("/api/todos").await;
    assert_eq!(
        list["todos"].as_array().map(Vec::len),
        Some(0),
        "a refused create leaves nothing behind"
    );
}

#[tokio::test]
async fn an_unknown_id_is_a_404_on_every_verb() {
    let harness = Harness::new();

    assert_eq!(
        harness.get("/api/todos/nothere").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        harness
            .patch("/api/todos/nothere", json!({ "status": "done" }))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        harness.delete("/api/todos/nothere").await.0,
        StatusCode::NOT_FOUND
    );
    // Not a valid id at all is still "no resource there", not a bad request.
    assert_eq!(
        harness.get("/api/todos/not%20an%20id").await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn removes_a_todo() {
    let harness = Harness::new();
    let id = id_of(&create(&harness, json!({ "description": "Ship it" })).await);

    assert_eq!(
        harness.delete(&format!("/api/todos/{id}")).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        harness.get(&format!("/api/todos/{id}")).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn filters_by_project_status_and_dates() {
    let harness = Harness::new();
    harness
        .post("/api/projects", json!({ "name": "timemd" }))
        .await;

    create(
        &harness,
        json!({ "description": "Soon", "project": "timemd", "due": "2026-08-05" }),
    )
    .await;
    create(
        &harness,
        json!({ "description": "Later", "project": "timemd", "due": "2026-12-01" }),
    )
    .await;
    let ticked = id_of(&create(&harness, json!({ "description": "Unowned" })).await);
    harness
        .patch(&format!("/api/todos/{ticked}"), json!({ "status": "done" }))
        .await;

    let descriptions = |list: &Value| -> Vec<String> {
        list["todos"]
            .as_array()
            .expect("an array")
            .iter()
            .map(|todo| todo["description"].as_str().unwrap_or_default().to_owned())
            .collect()
    };

    let (_, mine) = harness.get("/api/todos?project=timemd").await;
    assert_eq!(descriptions(&mine), ["Soon", "Later"]);

    let (_, open) = harness.get("/api/todos?status=open").await;
    assert_eq!(descriptions(&open), ["Soon", "Later"]);

    let (_, done) = harness.get("/api/todos?status=done").await;
    assert_eq!(descriptions(&done), ["Unowned"]);

    let (_, soon) = harness.get("/api/todos?dueBefore=2026-09-01").await;
    assert_eq!(descriptions(&soon), ["Soon"]);
}

/// Reads stay lenient: a line the app cannot parse is listed as a problem
/// rather than making the whole file unreadable.
#[tokio::test]
async fn reports_a_todo_line_it_could_not_read() {
    let harness = Harness::new();
    std::fs::write(
        harness.store.todos_path(),
        "---\n---\n\n## Todos\n\n- [ ] Fine 🆔 abc123\n- [ ] Broken 📅 not-a-date\n",
    )
    .expect("writes the file");

    let (status, list) = harness.get("/api/todos").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["todos"].as_array().map(Vec::len), Some(1));
    assert_eq!(list["problems"].as_array().map(Vec::len), Some(1));
}

/// A hand-written todo has no id until the app writes that file, at which point
/// it gets one and becomes addressable.
#[tokio::test]
async fn a_hand_written_todo_is_given_an_id_on_the_next_write() {
    let harness = Harness::new();
    std::fs::write(
        harness.store.todos_path(),
        "---\n---\n\n## Todos\n\n- [ ] Typed by hand\n",
    )
    .expect("writes the file");

    let (_, before) = harness.get("/api/todos").await;
    assert_eq!(before["todos"][0]["id"], Value::Null);

    create(&harness, json!({ "description": "Typed by the app" })).await;

    let (_, after) = harness.get("/api/todos").await;
    assert!(after["todos"][0]["id"].is_string(), "{after}");
}

#[tokio::test]
async fn a_scheduled_todo_shows_on_that_day() {
    let harness = Harness::new();
    create(
        &harness,
        json!({ "description": "Afternoon", "scheduled": "2026-08-01 14:00" }),
    )
    .await;
    create(
        &harness,
        json!({ "description": "Whenever", "scheduled": "2026-08-01" }),
    )
    .await;
    create(
        &harness,
        json!({ "description": "Tomorrow", "scheduled": "2026-08-02" }),
    )
    .await;

    let (status, day) = harness.get("/api/days/2026-08-01").await;
    assert_eq!(status, StatusCode::OK, "{day}");

    let scheduled: Vec<&str> = day["todos"]
        .as_array()
        .expect("an array")
        .iter()
        .filter_map(|todo| todo["description"].as_str())
        .collect();
    assert_eq!(scheduled, ["Whenever", "Afternoon"], "untimed first");
}

/// Starting on a todo takes its project and description, so the logged session
/// is findable by the words the todo is written in.
#[tokio::test]
async fn starts_a_session_on_a_todo() {
    let harness = Harness::new();
    harness
        .post("/api/projects", json!({ "name": "timemd" }))
        .await;
    let id = id_of(
        &create(
            &harness,
            json!({ "description": "Fix the ticker drift", "project": "timemd" }),
        )
        .await,
    );

    let (status, timer) = harness
        .post("/api/timer/start", json!({ "todo": id }))
        .await;

    assert_eq!(status, StatusCode::OK, "{timer}");
    assert_eq!(timer["active"]["project"], "timemd");
    assert_eq!(timer["active"]["note"], "Fix the ticker drift");
}

#[tokio::test]
async fn refuses_to_start_on_a_todo_that_is_not_there() {
    let harness = Harness::new();
    let (status, _) = harness
        .post("/api/timer/start", json!({ "todo": "nothere" }))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
