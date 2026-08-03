//! `/api/ntfy` — where notifications go when a browser cannot take them.
//!
//! Every write here points the server at a closed loopback port. A write that
//! moves where notifications go also sends a test notification, and a test
//! suite must not put a packet on the internet. `127.0.0.1:1` refuses instantly,
//! which asserts the `unreachable` outcome for free.

mod support;

use axum::http::StatusCode;
use serde_json::json;
use support::Harness;

/// Nowhere to reach, so a test send fails at the transport rather than leaving.
const NOWHERE: &str = "http://127.0.0.1:1";

#[tokio::test]
async fn reads_the_defaults_before_anything_is_written() {
    let harness = Harness::new();
    let (status, body) = harness.get("/api/ntfy").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["server"], "https://ntfy.sh");
    assert_eq!(body["topic"], serde_json::Value::Null);
    assert_eq!(body["appUrl"], serde_json::Value::Null);
    assert_eq!(body["hasToken"], false);
    assert_eq!(body["subscribeUrl"], serde_json::Value::Null);
    assert_eq!(body["test"], serde_json::Value::Null);

    assert!(
        !harness.store.ntfy_path().exists(),
        "a read must not create a credential file the tree did not have"
    );
}

#[tokio::test]
async fn writes_a_topic_and_reads_it_back() {
    let harness = Harness::new();
    let (status, body) = harness
        .put(
            "/api/ntfy",
            json!({ "server": NOWHERE, "topic": "timemd-a7f3" }),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["topic"], "timemd-a7f3");
    assert_eq!(body["subscribeUrl"], format!("{NOWHERE}/timemd-a7f3"));

    let (_, reread) = harness.get("/api/ntfy").await;
    assert_eq!(reread["topic"], "timemd-a7f3");
    assert_eq!(reread["server"], NOWHERE);
}

/// A value the API hands back is a value that ends up in a browser's network
/// log. The file at mode 0600 is the only copy there should be.
#[tokio::test]
async fn never_answers_with_the_token() {
    let harness = Harness::new();
    harness
        .put(
            "/api/ntfy",
            json!({ "server": NOWHERE, "topic": "timemd-a7f3", "token": "tk_secret" }),
        )
        .await;

    let (_, body) = harness.get("/api/ntfy").await;
    assert_eq!(body["hasToken"], true);
    assert!(
        !body.to_string().contains("tk_secret"),
        "the token must not travel back: {body}"
    );
}

#[tokio::test]
async fn clearing_the_topic_turns_the_channel_off() {
    let harness = Harness::new();
    harness
        .put(
            "/api/ntfy",
            json!({ "server": NOWHERE, "topic": "timemd-a7f3" }),
        )
        .await;

    let (status, body) = harness.put("/api/ntfy", json!({ "topic": null })).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["topic"], serde_json::Value::Null);
    assert_eq!(body["subscribeUrl"], serde_json::Value::Null);
    assert_eq!(
        body["server"], NOWHERE,
        "turning it off must not forget where it pointed"
    );
}

#[tokio::test]
async fn rejects_a_topic_it_could_not_publish_to() {
    let harness = Harness::new();
    for body in [
        json!({ "topic": "alpha/beta" }),
        json!({ "topic": "my topic" }),
        json!({ "server": "ntfy.sh" }),
        json!({ "appUrl": "box.ts.net" }),
    ] {
        let (status, _) = harness.put("/api/ntfy", body.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }

    assert!(
        !harness.store.ntfy_path().exists(),
        "a refused write must not leave a half-made file behind"
    );
}

#[tokio::test]
async fn a_write_preserves_agent_authored_keys() {
    let harness = Harness::new();
    let path = harness.store.ntfy_path();
    std::fs::create_dir_all(path.parent().expect("has a parent")).expect("creates");
    std::fs::write(&path, "---\ntopic: alpha\npriority: 5\n---\n\n# ntfy\n").expect("writes");

    harness
        .put("/api/ntfy", json!({ "server": NOWHERE, "topic": "beta" }))
        .await;

    let text = std::fs::read_to_string(&path).expect("reads");
    assert!(text.contains("priority: 5"), "{text}");
    assert!(text.contains("topic: beta"), "{text}");
}

/// A save must not fire a network call every time a token is retyped — the
/// point of the test send is to prove a *new destination* works.
#[tokio::test]
async fn writing_only_the_token_does_not_send_a_test() {
    let harness = Harness::new();
    harness
        .put(
            "/api/ntfy",
            json!({ "server": NOWHERE, "topic": "timemd-a7f3" }),
        )
        .await;

    let (status, body) = harness.put("/api/ntfy", json!({ "token": "tk_new" })).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["test"], serde_json::Value::Null);
    assert_eq!(body["hasToken"], true);
}

/// ntfy answers 200 for any topic name, so the write is the one moment somebody
/// is looking at a screen and can be told the setup does not work.
#[tokio::test]
async fn moving_the_topic_reports_what_the_test_send_did() {
    let harness = Harness::new();
    let (status, body) = harness
        .put(
            "/api/ntfy",
            json!({ "server": NOWHERE, "topic": "timemd-a7f3" }),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["test"], "unreachable");
    assert_eq!(
        body["topic"], "timemd-a7f3",
        "a failed test send is not a reason to refuse the write"
    );
}

/// The settings screen sends all three destination fields on every Save, so a
/// guard that only asked "did the request name one" would buzz the user's phone
/// every time they pressed it.
#[tokio::test]
async fn saving_the_same_destination_again_sends_no_test() {
    let harness = Harness::new();
    let destination =
        json!({ "server": NOWHERE, "topic": "timemd-a7f3", "appUrl": "https://box.ts.net" });

    let (_, first) = harness.put("/api/ntfy", destination.clone()).await;
    assert_eq!(first["test"], "unreachable");

    let (status, again) = harness.put("/api/ntfy", destination).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(again["test"], serde_json::Value::Null);
}

/// Turning the channel off has nowhere to send a test to.
#[tokio::test]
async fn clearing_the_topic_sends_no_test() {
    let harness = Harness::new();
    harness
        .put(
            "/api/ntfy",
            json!({ "server": NOWHERE, "topic": "timemd-a7f3" }),
        )
        .await;

    let (_, body) = harness.put("/api/ntfy", json!({ "topic": null })).await;

    assert_eq!(body["test"], serde_json::Value::Null);
}
