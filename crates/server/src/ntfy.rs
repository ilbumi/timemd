//! ntfy: the notification channel a phone can rely on.
//!
//! Web Push depends on a browser being willing to wake a service worker, which
//! iOS treats as optional. ntfy is a plain HTTP POST to a topic an app is
//! subscribed to, so the phone's side of it is somebody else's problem.

use timemd_core::NtfyConfig;

use crate::notify::Notification;
use crate::state::AppState;

/// The publish body.
///
/// ntfy accepts JSON only at the server root, with the topic named in the body
/// — which is why the server and the topic are configured apart. The
/// alternative, `POST /{topic}` with an `X-Title` header, cannot carry a
/// non-ASCII block title: `HeaderValue` rejects the bytes outright, so a block
/// called "Café admin" would silently never arrive.
#[derive(serde::Serialize)]
struct Publish<'a> {
    topic: &'a str,
    title: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    click: Option<String>,
}

/// What a publish did, as far as the caller can tell from one round trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TestOutcome {
    Delivered,
    /// The server answered, and said no.
    Rejected,
    /// Nothing answered at all.
    Unreachable,
}

/// Publishes notifications to the configured topic.
///
/// Takes a batch so a tick that has both a finished session and a due reminder
/// reads `ntfy.md` once rather than once per message.
///
/// **Nothing here is ever removed on a failure**, which is the deliberate
/// difference from `push::send_one` — that drops a subscription the service
/// calls gone, because a browser reissues one for free. A topic is a line the
/// user typed and a token may not be recoverable, so clearing either would
/// silently undo a setting and turn a fixable error into an invisible one.
/// Every failure is logged and retried on the next tick.
pub async fn deliver(state: &AppState, notifications: &[Notification]) {
    if notifications.is_empty() {
        return;
    }

    let config = match state.store().read_ntfy() {
        Ok(config) => config,
        Err(error) => {
            tracing::error!("could not read the ntfy config: {error}");
            return;
        }
    };
    if !config.is_configured() {
        tracing::debug!("no ntfy topic; the channel is off");
        return;
    }

    // Independent round trips, so a batch goes out together rather than each
    // message waiting for the last to be acknowledged.
    futures::future::join_all(
        notifications
            .iter()
            .map(|notification| publish(state, &config, notification)),
    )
    .await;
}

/// Publishes one notification, reporting what the server made of it.
async fn publish(
    state: &AppState,
    config: &NtfyConfig,
    notification: &Notification,
) -> TestOutcome {
    let body = Publish {
        topic: config.topic.as_deref().unwrap_or_default(),
        title: &notification.title,
        message: &notification.body,
        click: config.click_for(&notification.url),
    };

    let payload = match serde_json::to_vec(&body) {
        Ok(payload) => payload,
        Err(error) => {
            tracing::error!("could not encode an ntfy publish: {error}");
            return TestOutcome::Rejected;
        }
    };

    let url = config.publish_url();
    let mut request = state
        .http()
        .post(&url)
        .header("content-type", "application/json")
        .body(payload);

    if let Some(token) = &config.token {
        request = request.header("authorization", format!("Bearer {token}"));
    }

    match request.send().await {
        Ok(response) if response.status().is_success() => {
            tracing::debug!(url, "published to ntfy");
            TestOutcome::Delivered
        }
        Ok(response) => {
            // 401 and 403 mean the topic is protected and the token is missing
            // or wrong; 404 means the server is not where it was said to be;
            // 429 is a rate limit. All of them are the user's to fix, and all
            // of them may be fixed by the next tick.
            tracing::warn!(url, status = %response.status(), "ntfy rejected a notification");
            TestOutcome::Rejected
        }
        Err(error) => {
            tracing::warn!(url, "could not reach ntfy: {error}");
            TestOutcome::Unreachable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Clock;
    use crate::testing::stub;
    use std::sync::Arc;
    use timemd_core::{NtfyPatch, Store};

    fn state() -> (tempfile::TempDir, AppState) {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Arc::new(Store::new(directory.path()));
        (directory, AppState::new(store, Clock::System))
    }

    fn configure(state: &AppState, patch: NtfyPatch) {
        state
            .store()
            .try_update_ntfy(|config| config.apply(patch))
            .expect("the store call")
            .expect("the patch is usable");
    }

    fn pointing_at(state: &AppState, server: &str) {
        configure(
            state,
            NtfyPatch {
                server: Some(server.to_owned()),
                topic: Some(Some("timemd-a7f3".to_owned())),
                ..NtfyPatch::default()
            },
        );
    }

    fn notification(title: &str) -> Notification {
        Notification {
            title: title.to_owned(),
            body: "09:00".to_owned(),
            url: "/today".to_owned(),
        }
    }

    /// Both channels are enabled on their own; an unconfigured one must cost
    /// nothing at all.
    #[tokio::test]
    async fn delivering_with_no_topic_is_a_no_op() {
        let (_directory, state) = state();

        deliver(&state, &[notification("Deep work")]).await;
        deliver(&state, &[]).await;

        assert!(!state.store().ntfy_path().exists());
    }

    #[tokio::test]
    async fn a_notification_is_published_as_json_to_the_server_root() {
        let (_directory, state) = state();
        let (base, served) = stub(200, 1).await;
        pointing_at(&state, &base);

        deliver(&state, &[notification("Deep work")]).await;

        let served = served.await.expect("the stub ran");
        let request = served.first().expect("a request arrived");
        assert!(request.starts_with("POST / HTTP/1.1"), "{request}");
        assert!(
            request
                .to_lowercase()
                .contains("content-type: application/json"),
            "{request}"
        );
        assert!(request.contains(r#""topic":"timemd-a7f3""#), "{request}");
        assert!(request.contains(r#""title":"Deep work""#), "{request}");
        assert!(request.contains(r#""message":"09:00""#), "{request}");
    }

    #[tokio::test]
    async fn a_token_travels_as_a_bearer_header() {
        let (_directory, state) = state();
        let (base, served) = stub(200, 1).await;
        pointing_at(&state, &base);

        deliver(&state, &[notification("Deep work")]).await;
        let without = served.await.expect("the stub ran");
        assert!(
            !without[0].to_lowercase().contains("authorization"),
            "a public topic must not send an empty credential: {}",
            without[0]
        );

        let (base, served) = stub(200, 1).await;
        configure(
            &state,
            NtfyPatch {
                server: Some(base),
                token: Some(Some("tk_secret".to_owned())),
                ..NtfyPatch::default()
            },
        );

        deliver(&state, &[notification("Deep work")]).await;
        let with = served.await.expect("the stub ran");
        assert!(
            with[0].contains("authorization: Bearer tk_secret"),
            "{}",
            with[0]
        );
    }

    /// The test that earns the JSON body over the header shape: a title with
    /// non-ASCII in it cannot be built into a header value at all, so a block
    /// named this way would silently never be announced.
    #[tokio::test]
    async fn a_non_ascii_title_survives_the_wire() {
        let (_directory, state) = state();
        let (base, served) = stub(200, 1).await;
        pointing_at(&state, &base);

        deliver(&state, &[notification("Café ☕")]).await;

        let served = served.await.expect("the stub ran");
        let request = served.first().expect("a request arrived");
        assert!(request.contains(r#""title":"Café ☕""#), "{request}");
    }

    #[tokio::test]
    async fn a_click_is_sent_only_when_an_app_url_is_set() {
        let (_directory, state) = state();
        let (base, served) = stub(200, 1).await;
        pointing_at(&state, &base);

        deliver(&state, &[notification("Deep work")]).await;
        let without = served.await.expect("the stub ran");
        assert!(
            !without[0].contains("click"),
            "the server must not invent an origin: {}",
            without[0]
        );

        let (base, served) = stub(200, 1).await;
        configure(
            &state,
            NtfyPatch {
                server: Some(base),
                app_url: Some(Some("https://box.ts.net".to_owned())),
                ..NtfyPatch::default()
            },
        );

        deliver(&state, &[notification("Deep work")]).await;
        let with = served.await.expect("the stub ran");
        assert!(
            with[0].contains(r#""click":"https://box.ts.net/today""#),
            "{}",
            with[0]
        );
    }

    /// Pins the deliberate difference from push, which drops a subscription the
    /// service calls gone. A topic is a line the user typed: clearing it would
    /// turn a fixable error into an invisible one.
    #[tokio::test]
    async fn a_rejected_topic_is_logged_and_the_config_is_kept() {
        let (_directory, state) = state();
        let (base, served) = stub(403, 1).await;
        pointing_at(&state, &base);

        deliver(&state, &[notification("Deep work")]).await;
        served.await.expect("the stub ran");

        assert_eq!(
            state.store().read_ntfy().expect("reads").topic.as_deref(),
            Some("timemd-a7f3"),
            "a refusal is not a reason to forget where notifications go"
        );
    }

    #[tokio::test]
    async fn an_unreachable_server_leaves_the_config_alone() {
        let (_directory, state) = state();
        // Nothing is listening here, so the request fails at the transport.
        pointing_at(&state, "http://127.0.0.1:1");

        deliver(&state, &[notification("Deep work")]).await;

        assert_eq!(
            state.store().read_ntfy().expect("reads").topic.as_deref(),
            Some("timemd-a7f3")
        );
    }

    /// A tick can carry a finished session and a due reminder at once. One that
    /// only published the first would drop the reminder on the floor.
    #[tokio::test]
    async fn every_notification_in_a_batch_is_published() {
        let (_directory, state) = state();
        let (base, served) = stub(200, 2).await;
        pointing_at(&state, &base);

        deliver(
            &state,
            &[notification("Deep work"), notification("Session complete")],
        )
        .await;

        let served = served.await.expect("the stub ran");
        assert_eq!(served.len(), 2);
        assert!(served.iter().any(|request| request.contains("Deep work")));
        assert!(
            served
                .iter()
                .any(|request| request.contains("Session complete"))
        );
    }

    /// A publish tells apart "the server said no" from "nothing answered",
    /// because they are two different things for a user to go and fix.
    #[tokio::test]
    async fn a_publish_reports_what_the_server_said() {
        let (_directory, state) = state();

        let (base, served) = stub(200, 1).await;
        pointing_at(&state, &base);
        assert_eq!(outcome_of(&state).await, TestOutcome::Delivered);
        served.await.expect("the stub ran");

        let (base, served) = stub(403, 1).await;
        pointing_at(&state, &base);
        assert_eq!(outcome_of(&state).await, TestOutcome::Rejected);
        served.await.expect("the stub ran");

        pointing_at(&state, "http://127.0.0.1:1");
        assert_eq!(outcome_of(&state).await, TestOutcome::Unreachable);
    }

    async fn outcome_of(state: &AppState) -> TestOutcome {
        let config = state.store().read_ntfy().expect("reads");
        publish(state, &config, &notification("Deep work")).await
    }
}
