//! What a notification says, and the fan-out that gets it to every channel.
//!
//! The payload lives here rather than in a transport because more than one
//! transport reads it and neither owns it.

use serde::Serialize;

use crate::push;
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
pub async fn deliver(state: &AppState, notifications: &[Notification]) {
    if notifications.is_empty() {
        return;
    }

    push::deliver(state, notifications).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Clock;
    use std::sync::Arc;
    use timemd_core::Store;

    /// The guard every transport leans on: a tick with nothing to say must not
    /// read a config file, open a socket or panic.
    #[tokio::test]
    async fn an_empty_batch_reaches_no_channel() {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Arc::new(Store::new(directory.path()));
        let state = AppState::new(store, Clock::System);

        deliver(&state, &[]).await;

        assert!(
            !state.store().push_path().exists(),
            "an empty batch must not touch the tree"
        );
    }
}
