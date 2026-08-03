//! ntfy: the notification channel a phone can rely on.
//!
//! Web Push depends on a browser being willing to wake a service worker, which
//! iOS treats as optional. ntfy is a plain HTTP POST to a topic an app is
//! subscribed to, so the phone's side of it is somebody else's problem.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use timemd_core::{NtfyConfig, NtfyPatch};

use crate::error::ApiResult;
use crate::notify::Notification;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/ntfy", get(read).put(write))
}

/// The publish body.
///
/// ntfy accepts JSON only at the server root, with the topic named in the body
/// — which is why the server and the topic are configured apart. The
/// alternative, `POST /{topic}` with an `X-Title` header, cannot carry a
/// non-ASCII block title: `HeaderValue` rejects the bytes outright, so a block
/// called "Café admin" would silently never arrive.
#[derive(Serialize)]
struct Publish<'a> {
    topic: &'a str,
    title: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    click: Option<String>,
}

/// What a publish did, as far as the caller can tell from one round trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TestOutcome {
    Delivered,
    /// The server answered, and said no.
    Rejected,
    /// Nothing answered at all.
    Unreachable,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NtfyView {
    server: String,
    topic: Option<String>,
    app_url: Option<String>,
    /// Whether a token is set. Never the token: a value the API hands back is a
    /// value that ends up in a browser's network log.
    has_token: bool,
    /// What a phone subscribes to, so nobody has to assemble it by hand.
    subscribe_url: Option<String>,
    /// What the test send did, and `None` when there was nothing to test.
    test: Option<TestOutcome>,
}

impl NtfyView {
    fn of(config: &NtfyConfig, test: Option<TestOutcome>) -> Self {
        Self {
            server: config.server.clone(),
            topic: config.topic.clone(),
            app_url: config.app_url.clone(),
            has_token: config.token.is_some(),
            subscribe_url: config.topic_url(),
            test,
        }
    }
}

/// The wire body. Every field but `server` accepts an explicit `null` to clear
/// it; an absent key leaves the value alone.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NtfyRequest {
    server: Option<String>,
    #[serde(default, deserialize_with = "crate::parse::nullable")]
    topic: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::parse::nullable")]
    token: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::parse::nullable")]
    app_url: Option<Option<String>>,
}

async fn read(State(state): State<AppState>) -> ApiResult<Json<NtfyView>> {
    let config = state.store().read_ntfy()?;
    Ok(Json(NtfyView::of(&config, None)))
}

/// Writes the config, and proves it works when it can.
///
/// The test send happens only when the write actually moved where notifications
/// go: ntfy answers 200 for any topic name, so a typo is indistinguishable from
/// success at the transport, and a save is the one moment somebody is looking at
/// a screen. Retyping a token is not a new destination and does not fire one.
///
/// The outcome is reported rather than enforced. A server that is down right
/// now is a thing to tell the user about, not a reason to refuse to remember
/// what they typed.
async fn write(
    State(state): State<AppState>,
    Json(request): Json<NtfyRequest>,
) -> ApiResult<Json<NtfyView>> {
    let patch = NtfyPatch {
        server: request.server,
        topic: request.topic,
        token: request.token,
        app_url: request.app_url,
    };

    // Compared before and after rather than read off the request: the settings
    // screen sends all three destination fields on every Save, so "did the
    // request name one" is true every time and would buzz the user's phone for
    // a Save that changed nothing.
    let (config, moved) = state.store().try_update_ntfy(|config| {
        let before = destination(config);
        config.apply(patch)?;
        Ok::<_, timemd_core::Error>((config.clone(), destination(config) != before))
    })??;

    let test = if moved && config.is_configured() {
        Some(send_test(&state, &config).await)
    } else {
        None
    };

    Ok(Json(NtfyView::of(&config, test)))
}

/// Where notifications go: what they are published to, and what they link to.
///
/// Two configs that agree here deliver the same message to the same place, so a
/// write that leaves this alone has nothing new to prove. The token is not part
/// of it — it is a credential for a destination, not the destination.
fn destination(config: &NtfyConfig) -> (Option<String>, Option<String>) {
    (config.topic_url(), config.app_url.clone())
}

/// Publishes one notification whose only purpose is to prove the setup works.
///
/// This cannot catch a typo in the topic — ntfy accepts any name — only a wrong
/// server or a wrong token. Whatever shows it to a user has to say so.
async fn send_test(state: &AppState, config: &NtfyConfig) -> TestOutcome {
    publish(
        state,
        config,
        &Notification {
            title: "timemd".to_owned(),
            body: "Notifications are set up.".to_owned(),
            url: "/".to_owned(),
        },
    )
    .await
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
    use crate::testing::{notification, ntfy_at as pointing_at, state, stub};
    use timemd_core::NtfyPatch;

    fn configure(state: &AppState, patch: NtfyPatch) {
        state
            .store()
            .try_update_ntfy(|config| config.apply(patch))
            .expect("the store call")
            .expect("the patch is usable");
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
