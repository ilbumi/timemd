//! Project CRUD over HTTP, checked against the files it produces.

mod support;

use axum::http::StatusCode;
use serde_json::json;
use support::Harness;

#[tokio::test]
async fn lists_nothing_before_anything_is_created() {
    let harness = Harness::new();
    let (status, body) = harness.get("/api/projects").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));
}

#[tokio::test]
async fn creates_a_project_and_derives_its_slug() {
    let harness = Harness::new();
    let (status, body) = harness
        .post("/api/projects", json!({ "name": "Deep Work" }))
        .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["slug"], "deep-work");
    assert_eq!(body["name"], "Deep Work");
    assert_eq!(body["status"], "active");
    assert_eq!(body["created"], "2026-08-01");
    assert_eq!(body["color"], serde_json::Value::Null);
}

#[tokio::test]
async fn a_created_project_lands_on_disk_as_markdown() {
    let harness = Harness::new();
    harness
        .post("/api/projects", json!({ "name": "Deep Work" }))
        .await;

    let path = harness.store.root().join("projects/deep-work.md");
    let text = std::fs::read_to_string(&path).expect("project file exists");
    assert!(text.contains("name: Deep Work"), "{text}");
    assert!(text.contains("status: active"), "{text}");
    assert!(text.contains("created: 2026-08-01"), "{text}");
}

#[tokio::test]
async fn accepts_an_explicit_colour_on_create() {
    let harness = Harness::new();
    let (status, body) = harness
        .post(
            "/api/projects",
            json!({ "name": "timemd", "color": "#4f46e5" }),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["color"], "#4f46e5");
}

#[tokio::test]
async fn a_new_project_is_a_square_with_no_target_or_milestones() {
    let harness = Harness::new();
    let (_, body) = harness
        .post("/api/projects", json!({ "name": "timemd" }))
        .await;

    assert_eq!(body["mark"], "square");
    assert_eq!(body["target"], serde_json::Value::Null);
    assert_eq!(body["milestones"], json!([]));
    assert_eq!(body["problems"], json!([]));
}

#[tokio::test]
async fn accepts_a_mark_and_a_weekly_target_on_create() {
    let harness = Harness::new();
    let (status, body) = harness
        .post(
            "/api/projects",
            json!({ "name": "Thesis", "mark": "triangle", "target": "10h" }),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["mark"], "triangle");
    assert_eq!(body["target"], "10h");

    let path = harness.store.root().join("projects/thesis.md");
    let text = std::fs::read_to_string(&path).expect("project file exists");
    assert!(text.contains("mark: triangle"), "{text}");
    assert!(text.contains("target: 10h"), "{text}");
}

#[tokio::test]
async fn rejects_an_unknown_mark_and_an_unreadable_target() {
    let harness = Harness::new();
    for body in [
        json!({ "name": "timemd", "mark": "hexagon" }),
        json!({ "name": "timemd", "target": "loads" }),
    ] {
        let (status, _) = harness.post("/api/projects", body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn replaces_the_whole_milestone_list_on_patch() {
    let harness = Harness::new();
    harness
        .post("/api/projects", json!({ "name": "Thesis" }))
        .await;

    let (status, body) = harness
        .patch(
            "/api/projects/thesis",
            json!({ "milestones": [
                { "done": true, "title": "Ch. 1 — lit review" },
                { "done": false, "title": "Ch. 4 — first draft" },
            ] }),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["milestones"][0]["done"], true);
    assert_eq!(body["milestones"][1]["title"], "Ch. 4 — first draft");

    let path = harness.store.root().join("projects/thesis.md");
    let text = std::fs::read_to_string(&path).expect("reads");
    assert!(text.contains("## Milestones"), "{text}");
    assert!(text.contains("- [x] Ch. 1 — lit review"), "{text}");
    assert!(text.contains("- [ ] Ch. 4 — first draft"), "{text}");
}

#[tokio::test]
async fn rejects_a_milestone_with_a_blank_title() {
    let harness = Harness::new();
    harness
        .post("/api/projects", json!({ "name": "Thesis" }))
        .await;

    let (status, _) = harness
        .patch(
            "/api/projects/thesis",
            json!({ "milestones": [{ "done": false, "title": "  " }] }),
        )
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn clears_a_target_with_an_explicit_null() {
    let harness = Harness::new();
    harness
        .post(
            "/api/projects",
            json!({ "name": "Thesis", "target": "10h" }),
        )
        .await;

    let (_, body) = harness
        .patch("/api/projects/thesis", json!({ "target": null }))
        .await;

    assert_eq!(body["target"], serde_json::Value::Null);
}

#[tokio::test]
async fn reports_a_milestone_line_it_could_not_read() {
    let harness = Harness::new();
    let directory = harness.store.root().join("projects");
    std::fs::create_dir_all(&directory).expect("creates dir");
    std::fs::write(
        directory.join("thesis.md"),
        "---\nname: Thesis\n---\n\n## Milestones\n\n- [x] done\n- forgot the box\n",
    )
    .expect("writes");

    let (status, body) = harness.get("/api/projects/thesis").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["milestones"][0]["title"], "done");
    assert_eq!(body["problems"].as_array().expect("an array").len(), 1);
}

#[tokio::test]
async fn rejects_a_name_that_yields_no_slug() {
    let harness = Harness::new();
    let (status, _) = harness
        .post("/api/projects", json!({ "name": "!!!" }))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rejects_an_invalid_colour() {
    let harness = Harness::new();
    let (status, _) = harness
        .post(
            "/api/projects",
            json!({ "name": "timemd", "color": "blurple" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn refuses_to_create_the_same_project_twice() {
    let harness = Harness::new();
    harness
        .post("/api/projects", json!({ "name": "timemd" }))
        .await;
    let (status, _) = harness
        .post("/api/projects", json!({ "name": "timemd" }))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn lists_projects_ordered_by_slug() {
    let harness = Harness::new();
    for name in ["timemd", "admin", "reading"] {
        harness.post("/api/projects", json!({ "name": name })).await;
    }

    let (status, body) = harness.get("/api/projects").await;
    assert_eq!(status, StatusCode::OK);
    let slugs: Vec<_> = body
        .as_array()
        .expect("an array")
        .iter()
        .map(|project| project["slug"].as_str().expect("a slug"))
        .collect();
    assert_eq!(slugs, vec!["admin", "reading", "timemd"]);
}

#[tokio::test]
async fn reads_a_single_project() {
    let harness = Harness::new();
    harness
        .post("/api/projects", json!({ "name": "timemd" }))
        .await;

    let (status, body) = harness.get("/api/projects/timemd").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["slug"], "timemd");
}

#[tokio::test]
async fn reading_a_missing_project_is_not_found() {
    let harness = Harness::new();
    let (status, _) = harness.get("/api/projects/ghost").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_invalid_slug_in_the_path_is_not_found() {
    let harness = Harness::new();
    let (status, _) = harness.get("/api/projects/Not%20A%20Slug").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn updates_name_colour_and_status() {
    let harness = Harness::new();
    harness
        .post("/api/projects", json!({ "name": "timemd" }))
        .await;

    let (status, body) = harness
        .patch(
            "/api/projects/timemd",
            json!({ "name": "Time MD", "color": "#112233", "status": "archived" }),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "Time MD");
    assert_eq!(body["color"], "#112233");
    assert_eq!(body["status"], "archived");
}

#[tokio::test]
async fn a_patch_only_touches_the_fields_it_names() {
    let harness = Harness::new();
    harness
        .post(
            "/api/projects",
            json!({ "name": "timemd", "color": "#4f46e5" }),
        )
        .await;

    let (_, body) = harness
        .patch("/api/projects/timemd", json!({ "status": "archived" }))
        .await;

    assert_eq!(body["name"], "timemd");
    assert_eq!(body["color"], "#4f46e5");
    assert_eq!(body["status"], "archived");
}

#[tokio::test]
async fn a_patch_preserves_agent_authored_content() {
    let harness = Harness::new();
    harness
        .post("/api/projects", json!({ "name": "timemd" }))
        .await;

    let path = harness.store.root().join("projects/timemd.md");
    let original = std::fs::read_to_string(&path).expect("reads");
    let edited = original.replace(
        "---\n\n",
        "notion_id: abc-123\n---\n\nNotes an agent wrote.\n\n",
    );
    std::fs::write(&path, edited).expect("writes");

    harness
        .patch("/api/projects/timemd", json!({ "status": "archived" }))
        .await;

    let text = std::fs::read_to_string(&path).expect("reads");
    assert!(text.contains("notion_id: abc-123"), "{text}");
    assert!(text.contains("Notes an agent wrote."), "{text}");
    assert!(text.contains("status: archived"), "{text}");
}

#[tokio::test]
async fn clears_a_colour_with_an_explicit_null() {
    let harness = Harness::new();
    harness
        .post(
            "/api/projects",
            json!({ "name": "timemd", "color": "#4f46e5" }),
        )
        .await;

    let (status, body) = harness
        .patch("/api/projects/timemd", json!({ "color": null }))
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["color"], serde_json::Value::Null);
}

#[tokio::test]
async fn patching_a_missing_project_is_not_found() {
    let harness = Harness::new();
    let (status, _) = harness
        .patch("/api/projects/ghost", json!({ "name": "x" }))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deletes_a_project() {
    let harness = Harness::new();
    harness
        .post("/api/projects", json!({ "name": "timemd" }))
        .await;

    let (status, _) = harness.delete("/api/projects/timemd").await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = harness.get("/api/projects/timemd").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deleting_a_missing_project_is_not_found() {
    let harness = Harness::new();
    let (status, _) = harness.delete("/api/projects/ghost").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_project_written_by_hand_shows_up_without_a_restart() {
    let harness = Harness::new();
    let directory = harness.store.root().join("projects");
    std::fs::create_dir_all(&directory).expect("creates dir");
    std::fs::write(
        directory.join("by-agent.md"),
        "---\nname: Written by an agent\nstatus: active\n---\n\n# Written by an agent\n",
    )
    .expect("writes");

    let (status, body) = harness.get("/api/projects").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body[0]["slug"], "by-agent");
    assert_eq!(body[0]["name"], "Written by an agent");
}
