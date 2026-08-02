//! Web Push: VAPID keys, subscriptions and delivery.
//!
//! `web-push` is used for message building and payload encryption only — its
//! bundled clients are built on hyper 0.14, and dragging a second major version
//! of hyper into a tree that already has axum's is not worth it. Delivery goes
//! through `reqwest`, which shares hyper 1.x with the rest of the server.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
// `Generate` draws from the operating system directly, so no `rand` version
// has to be kept in step with the one p256 was built against.
use p256::SecretKey;
use p256::elliptic_curve::Generate;
use serde::{Deserialize, Serialize};
use timemd_core::push::Subscription;
use timemd_core::{Error, Result};
use web_push::{
    ContentEncoding, SubscriptionInfo, VapidSignatureBuilder, WebPushMessage, WebPushMessageBuilder,
};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// How long a push service should hold a notification for a phone that is
/// offline. A reminder is worthless once its block has begun, and a completed
/// pomodoro is stale news the next morning.
const TTL_SECONDS: u32 = 30 * 60;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/push/key", get(public_key))
        .route("/push/subscribe", post(subscribe).delete(unsubscribe))
}

/// What a notification says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Notification {
    pub title: String,
    pub body: String,
    /// Where tapping it should land.
    pub url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KeyView {
    /// The VAPID public key, base64url — what `pushManager.subscribe` needs.
    public_key: String,
}

#[derive(Deserialize)]
struct Unsubscribe {
    endpoint: String,
}

async fn public_key(State(state): State<AppState>) -> ApiResult<Json<KeyView>> {
    let private = ensure_keypair(&state)?;
    Ok(Json(KeyView {
        public_key: derive_public_key(&private)?,
    }))
}

async fn subscribe(
    State(state): State<AppState>,
    Json(subscription): Json<Subscription>,
) -> ApiResult<StatusCode> {
    if subscription.endpoint.is_empty() {
        return Err(ApiError::bad_request("a subscription needs an endpoint"));
    }
    state
        .store()
        .update_push(|push| push.subscribe(subscription))?;
    Ok(StatusCode::CREATED)
}

async fn unsubscribe(
    State(state): State<AppState>,
    Json(body): Json<Unsubscribe>,
) -> ApiResult<StatusCode> {
    let removed = state
        .store()
        .update_push(|push| push.unsubscribe(&body.endpoint))?;

    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("no such subscription"))
    }
}

/// Returns the VAPID private key, generating one on first use.
pub fn ensure_keypair(state: &AppState) -> Result<String> {
    // The key almost always exists already, and generating is the rare path — so
    // read first rather than making every call pay for a durable write and a
    // chmod it does not need.
    if let Some(existing) = state.store().read_push()?.private_key {
        return Ok(existing);
    }

    state.store().update_push(|push| {
        push.private_key
            .get_or_insert_with(|| {
                let secret = SecretKey::generate();
                URL_SAFE_NO_PAD.encode(secret.to_bytes())
            })
            .clone()
    })
}

/// The public half, base64url-encoded, as the browser expects it.
fn derive_public_key(private: &str) -> Result<String> {
    let builder = VapidSignatureBuilder::from_base64_no_sub(private)
        .map_err(|error| Error::UnknownProject(format!("unusable VAPID key: {error}")))?;
    Ok(URL_SAFE_NO_PAD.encode(builder.get_public_key()))
}

/// Delivers notifications to every subscribed browser.
///
/// Takes a batch so a tick that has both a finished session and a due reminder
/// reads `push.md` once rather than once per message.
///
/// Subscriptions the push service reports as gone are dropped, so an uninstalled
/// app stops being retried forever. Every other failure is logged and skipped:
/// one unreachable device must not stop the others being told.
pub async fn deliver(state: &AppState, notifications: &[Notification]) {
    if notifications.is_empty() {
        return;
    }

    let (private, subscriptions) = match state.store().read_push() {
        Ok(push) => (push.private_key, push.subscriptions),
        Err(error) => {
            tracing::error!("could not read push state: {error}");
            return;
        }
    };

    let Some(private) = private else {
        tracing::debug!("no VAPID key yet; nothing is subscribed");
        return;
    };
    if subscriptions.is_empty() {
        return;
    }

    let mut messages = Vec::new();
    for notification in notifications {
        let payload = match serde_json::to_vec(notification) {
            Ok(payload) => payload,
            Err(error) => {
                tracing::error!("could not encode notification: {error}");
                continue;
            }
        };

        for subscription in &subscriptions {
            match build(&private, subscription, &payload) {
                Ok(message) => messages.push(message),
                Err(error) => tracing::error!(
                    endpoint = subscription.endpoint,
                    "could not build push message: {error}"
                ),
            }
        }
    }

    // Independent round trips, so they go out together rather than each waiting
    // for the last device to answer.
    let sent = futures::future::join_all(
        messages
            .into_iter()
            .map(|message| send_one(state.http(), message)),
    )
    .await;
    let gone: Vec<String> = sent.into_iter().flatten().collect();

    if !gone.is_empty() {
        let result = state.store().update_push(|push| {
            for endpoint in &gone {
                push.unsubscribe(endpoint);
            }
        });
        if let Err(error) = result {
            tracing::error!("could not drop dead subscriptions: {error}");
        } else {
            tracing::info!(count = gone.len(), "dropped expired push subscriptions");
        }
    }
}

fn build(private: &str, subscription: &Subscription, payload: &[u8]) -> Result<WebPushMessage> {
    let failed = |error: web_push::WebPushError| Error::UnknownProject(error.to_string());

    let info = SubscriptionInfo::new(
        subscription.endpoint.clone(),
        subscription.p256dh.clone(),
        subscription.auth.clone(),
    );

    let signature = VapidSignatureBuilder::from_base64(private, &info)
        .map_err(failed)?
        .build()
        .map_err(failed)?;

    let mut builder = WebPushMessageBuilder::new(&info);
    builder.set_payload(ContentEncoding::Aes128Gcm, payload);
    builder.set_vapid_signature(signature);
    builder.set_ttl(TTL_SECONDS);
    builder.build().map_err(failed)
}

/// Sends one message. Returns the endpoint if the push service says it is gone.
async fn send_one(client: &reqwest::Client, message: WebPushMessage) -> Option<String> {
    let endpoint = message.endpoint.to_string();
    let mut request = client
        .post(&endpoint)
        .header("TTL", message.ttl.to_string());

    if let Some(payload) = message.payload {
        for (name, value) in payload.crypto_headers {
            request = request.header(name, value);
        }
        request = request
            .header("Content-Encoding", payload.content_encoding.to_str())
            .body(payload.content);
    }

    match request.send().await {
        // 404 and 410 are the push protocol's way of saying the subscription is
        // dead; anything else may well work next time.
        Ok(response) if matches!(response.status().as_u16(), 404 | 410) => {
            tracing::info!(endpoint, "push subscription is gone");
            Some(endpoint)
        }
        Ok(response) if !response.status().is_success() => {
            tracing::warn!(endpoint, status = %response.status(), "push rejected");
            None
        }
        Ok(_) => None,
        Err(error) => {
            tracing::warn!(endpoint, "push failed: {error}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Clock;
    use std::sync::Arc;
    use timemd_core::Store;

    fn state() -> (tempfile::TempDir, AppState) {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Arc::new(Store::new(directory.path()));
        (directory, AppState::new(store, Clock::System))
    }

    #[test]
    fn a_keypair_is_generated_once_and_then_reused() {
        let (_directory, state) = state();

        let first = ensure_keypair(&state).expect("generates");
        let second = ensure_keypair(&state).expect("reuses");

        assert_eq!(first, second, "a second call must not rotate the key");
        assert!(!first.is_empty());
    }

    /// If the public key changed between calls, every already-subscribed browser
    /// would silently stop receiving notifications.
    #[test]
    fn the_public_key_is_stable_and_browser_shaped() {
        let (_directory, state) = state();
        let private = ensure_keypair(&state).expect("generates");

        let public = derive_public_key(&private).expect("derives");
        assert_eq!(public, derive_public_key(&private).expect("derives"));

        let decoded = URL_SAFE_NO_PAD.decode(&public).expect("is base64url");
        // An uncompressed P-256 point: a 0x04 tag and two 32-byte coordinates.
        assert_eq!(decoded.len(), 65);
        assert_eq!(decoded[0], 0x04);
    }

    #[test]
    fn an_unusable_private_key_is_an_error_rather_than_a_panic() {
        assert!(derive_public_key("not-a-key").is_err());
    }

    #[test]
    fn the_key_file_is_readable_only_by_its_owner() {
        let (_directory, state) = state();
        ensure_keypair(&state).expect("generates");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(state.store().push_path())
                .expect("the file exists")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "push.md holds a private key");
        }
    }

    #[test]
    fn a_message_is_built_for_a_subscription() {
        let (_directory, state) = state();
        let private = ensure_keypair(&state).expect("generates");

        // A well-formed but fabricated subscription: p256dh is an uncompressed
        // point and auth is 16 bytes, which is what the encryption expects.
        let subscription = Subscription {
            endpoint: "https://push.example/abc".to_owned(),
            p256dh: URL_SAFE_NO_PAD.encode(SecretKey::generate().public_key().to_sec1_bytes()),
            auth: URL_SAFE_NO_PAD.encode([7_u8; 16]),
        };

        let message = build(&private, &subscription, b"{\"title\":\"hi\"}").expect("builds");

        assert_eq!(message.endpoint.to_string(), "https://push.example/abc");
        assert_eq!(message.ttl, TTL_SECONDS);
        assert!(message.payload.is_some());
    }

    #[test]
    fn building_for_a_malformed_subscription_fails_cleanly() {
        let (_directory, state) = state();
        let private = ensure_keypair(&state).expect("generates");

        let subscription = Subscription {
            endpoint: "https://push.example/abc".to_owned(),
            p256dh: "not-a-key".to_owned(),
            auth: "not-a-secret".to_owned(),
        };

        assert!(build(&private, &subscription, b"{}").is_err());
    }

    /// A stand-in push service: answers one request with `status`, then reports
    /// what it was sent. Enough to exercise delivery without the network.
    async fn stub_service(status: u16) -> (String, tokio::task::JoinHandle<Option<String>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("binds");
        let endpoint = format!(
            "http://{}/push",
            listener.local_addr().expect("has an address")
        );

        let served = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.ok()?;
            let mut buffer = vec![0_u8; 4096];
            let read = stream.read(&mut buffer).await.ok()?;
            let request = String::from_utf8_lossy(&buffer[..read]).into_owned();

            let reason = if status == 201 { "Created" } else { "Gone" };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(response.as_bytes()).await.ok()?;
            stream.shutdown().await.ok()?;
            Some(request)
        });

        (endpoint, served)
    }

    fn well_formed(endpoint: &str) -> Subscription {
        Subscription {
            endpoint: endpoint.to_owned(),
            p256dh: URL_SAFE_NO_PAD.encode(SecretKey::generate().public_key().to_sec1_bytes()),
            auth: URL_SAFE_NO_PAD.encode([7_u8; 16]),
        }
    }

    fn notification() -> Notification {
        Notification {
            title: "Deep work".to_owned(),
            body: "09:00".to_owned(),
            url: "/today".to_owned(),
        }
    }

    #[tokio::test]
    async fn a_delivered_notification_is_encrypted_and_carries_its_headers() {
        let (_directory, state) = state();
        ensure_keypair(&state).expect("generates");
        let (endpoint, served) = stub_service(201).await;

        state
            .store()
            .update_push(|push| push.subscribe(well_formed(&endpoint)))
            .expect("subscribes");

        deliver(&state, &[notification()]).await;

        let request = served
            .await
            .expect("the stub ran")
            .expect("a request arrived");
        assert!(request.starts_with("POST /push"), "{request}");
        assert!(request.contains("content-encoding: aes128gcm"), "{request}");
        assert!(
            request.to_lowercase().contains("authorization: vapid"),
            "{request}"
        );
        // The payload must not appear in the clear.
        assert!(!request.contains("Deep work"), "{request}");

        // A successful delivery leaves the subscription in place.
        assert_eq!(
            state
                .store()
                .read_push()
                .expect("reads")
                .subscriptions
                .len(),
            1
        );
    }

    /// Without this, an uninstalled app is retried on every reminder forever.
    #[tokio::test]
    async fn a_subscription_the_service_calls_gone_is_dropped() {
        let (_directory, state) = state();
        ensure_keypair(&state).expect("generates");
        let (endpoint, served) = stub_service(410).await;

        state
            .store()
            .update_push(|push| push.subscribe(well_formed(&endpoint)))
            .expect("subscribes");

        deliver(&state, &[notification()]).await;
        served.await.expect("the stub ran");

        assert!(
            state
                .store()
                .read_push()
                .expect("reads")
                .subscriptions
                .is_empty(),
            "a gone subscription must not be retried forever"
        );
    }

    #[tokio::test]
    async fn an_unreachable_service_leaves_the_subscription_alone() {
        let (_directory, state) = state();
        ensure_keypair(&state).expect("generates");

        // Nothing is listening here, so the request fails at the transport.
        state
            .store()
            .update_push(|push| push.subscribe(well_formed("http://127.0.0.1:1/push")))
            .expect("subscribes");

        deliver(&state, &[notification()]).await;

        assert_eq!(
            state
                .store()
                .read_push()
                .expect("reads")
                .subscriptions
                .len(),
            1,
            "a transient failure is not a reason to forget the device"
        );
    }

    #[tokio::test]
    async fn a_malformed_subscription_does_not_stop_delivery() {
        let (_directory, state) = state();
        ensure_keypair(&state).expect("generates");
        let (endpoint, served) = stub_service(201).await;

        state
            .store()
            .update_push(|push| {
                push.subscribe(Subscription {
                    endpoint: "https://push.example/broken".to_owned(),
                    p256dh: "not-a-key".to_owned(),
                    auth: "not-a-secret".to_owned(),
                });
                push.subscribe(well_formed(&endpoint));
            })
            .expect("subscribes");

        deliver(&state, &[notification()]).await;

        // The good subscription was still served despite the bad one first.
        assert!(served.await.expect("the stub ran").is_some());
    }

    #[tokio::test]
    async fn delivering_with_nothing_subscribed_is_a_no_op() {
        let (_directory, state) = state();
        let notification = Notification {
            title: "Deep work".to_owned(),
            body: "09:00".to_owned(),
            url: "/".to_owned(),
        };

        // No key, then a key but no subscribers: neither should panic or block.
        deliver(&state, std::slice::from_ref(&notification)).await;
        ensure_keypair(&state).expect("generates");
        deliver(&state, &[notification]).await;
        // An empty batch must not read anything or panic.
        deliver(&state, &[]).await;
    }
}
