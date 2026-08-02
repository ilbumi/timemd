//! `/api/push` — the subscription a device registers to be notified.

mod support;

use axum::http::StatusCode;
use serde_json::json;
use support::Harness;

fn subscription() -> serde_json::Value {
    json!({
        "endpoint": "https://push.example/abc",
        "p256dh": "public-key",
        "auth": "secret"
    })
}

/// The key is generated on first ask and then stays put, so a device that
/// subscribed yesterday is still reachable today.
#[tokio::test]
async fn the_public_key_is_generated_once_and_kept() {
    let harness = Harness::new();

    let (status, first) = harness.get("/api/push/key").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        first["publicKey"]
            .as_str()
            .is_some_and(|key| !key.is_empty()),
        "{first}"
    );

    let (_, again) = harness.get("/api/push/key").await;
    assert_eq!(first, again);
}

#[tokio::test]
async fn a_device_subscribes_and_unsubscribes() {
    let harness = Harness::new();

    let (status, _) = harness.post("/api/push/subscribe", subscription()).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        harness
            .store
            .read_push()
            .expect("reads")
            .subscriptions
            .len(),
        1
    );

    // The body carries the endpoint, so this goes through `request` rather than
    // the `delete` helper, which sends none.
    let (status, _) = harness
        .request(
            "DELETE",
            "/api/push/subscribe",
            Some(json!({ "endpoint": "https://push.example/abc" })),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(
        harness
            .store
            .read_push()
            .expect("reads")
            .subscriptions
            .is_empty()
    );
}

/// Subscribing twice from one device must not push to it twice.
#[tokio::test]
async fn subscribing_again_from_the_same_endpoint_replaces_it() {
    let harness = Harness::new();
    harness.post("/api/push/subscribe", subscription()).await;
    harness.post("/api/push/subscribe", subscription()).await;

    assert_eq!(
        harness
            .store
            .read_push()
            .expect("reads")
            .subscriptions
            .len(),
        1
    );
}

#[tokio::test]
async fn unsubscribing_an_endpoint_that_was_never_registered_is_not_found() {
    let harness = Harness::new();

    let (status, _) = harness
        .request(
            "DELETE",
            "/api/push/subscribe",
            Some(json!({ "endpoint": "https://push.example/nobody" })),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
