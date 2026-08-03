//! What a notification says, and the fan-out that gets it to every channel.
//!
//! The payload lives here rather than in a transport because more than one
//! transport reads it and neither owns it.

use serde::Serialize;

use crate::{ntfy, push};

use crate::state::AppState;

/// What a notification says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Notification {
    pub title: String,
    pub body: String,
    /// Where tapping it should land — a path, not a URL.
    ///
    /// The server cannot make it absolute: `--addr` is `0.0.0.0:8080` by
    /// default, and behind a reverse proxy the external origin never reaches
    /// the process. A browser resolves it against the page it is already on; a
    /// transport that needs an absolute URL has to be told the origin.
    pub url: String,
}

/// Sends a batch to every configured channel.
///
/// Takes a batch so a tick that has both a finished session and a due reminder
/// reads each channel's config once rather than once per message.
///
/// Concurrent, so the tick costs one round trip rather than the sum of two to
/// unrelated hosts. Both channels resolve to `()` and swallow their own
/// failures, which is what makes "an unreachable channel cannot stop the other"
/// a property of the types rather than of anyone remembering it.
pub async fn deliver(state: &AppState, notifications: &[Notification]) {
    if notifications.is_empty() {
        return;
    }

    tokio::join!(
        push::deliver(state, notifications),
        ntfy::deliver(state, notifications),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{notification, ntfy_at, state, stub, subscription};

    fn push_at(state: &AppState, endpoint: &str) {
        crate::push::ensure_keypair(state).expect("generates");
        state
            .store()
            .update_push(|push| push.subscribe(subscription(endpoint)))
            .expect("subscribes");
    }

    /// The guard every transport leans on: a tick with nothing to say must not
    /// read a config file, open a socket or panic.
    #[tokio::test]
    async fn an_empty_batch_reaches_no_channel() {
        let (_directory, state) = state();

        deliver(&state, &[]).await;

        assert!(
            !state.store().push_path().exists(),
            "an empty batch must not touch the tree"
        );
        assert!(!state.store().ntfy_path().exists());
    }

    /// The requirement, in one test: the channels are independent, so a topic
    /// pointed at a host that is down must not cost the browsers their
    /// notification.
    #[tokio::test]
    async fn a_dead_ntfy_server_does_not_stop_the_browsers() {
        let (_directory, state) = state();
        let (base, served) = stub(201, 1).await;
        push_at(&state, &format!("{base}/push"));
        ntfy_at(&state, "http://127.0.0.1:1");

        deliver(&state, &[notification("Deep work")]).await;

        assert_eq!(served.await.expect("the stub ran").len(), 1);
    }

    #[tokio::test]
    async fn a_dead_push_service_does_not_stop_ntfy() {
        let (_directory, state) = state();
        let (base, served) = stub(200, 1).await;
        push_at(&state, "http://127.0.0.1:1/push");
        ntfy_at(&state, &base);

        deliver(&state, &[notification("Deep work")]).await;

        let served = served.await.expect("the stub ran");
        assert_eq!(served.len(), 1);
        assert!(served[0].contains("Deep work"), "{}", served[0]);
    }
}
