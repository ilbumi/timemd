//! Fixtures shared by the transports' test suites.
//!
//! Delivery is the one part of the server that talks *out*, so exercising it
//! means standing up something to talk to. A real HTTP server is more than the
//! assertions need and a mock-server dependency is more than the tree wants; a
//! socket that answers a fixed status and hands back what it was sent is
//! exactly enough.

use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use p256::elliptic_curve::Generate;
use timemd_core::{NtfyPatch, Store, Subscription};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::task::JoinHandle;

use crate::notify::Notification;
use crate::state::{AppState, Clock};

/// A server over an empty tree. The directory comes back because dropping it
/// deletes the tree the state is still pointing at.
pub fn state() -> (tempfile::TempDir, AppState) {
    let directory = tempfile::tempdir().expect("temp dir");
    let store = Arc::new(Store::new(directory.path()));
    (directory, AppState::new(store, Clock::System))
}

pub fn notification(title: &str) -> Notification {
    Notification {
        title: title.to_owned(),
        body: "09:00".to_owned(),
        url: "/today".to_owned(),
    }
}

/// Points the ntfy channel at `server`, with a topic, so it is configured.
pub fn ntfy_at(state: &AppState, server: &str) {
    state
        .store()
        .try_update_ntfy(|config| {
            config.apply(NtfyPatch {
                server: Some(server.to_owned()),
                topic: Some(Some("timemd-a7f3".to_owned())),
                ..NtfyPatch::default()
            })
        })
        .expect("the store call")
        .expect("the patch is usable");
}

/// A fabricated but well-formed subscription: `p256dh` is an uncompressed point
/// and `auth` is 16 bytes, which is what the payload encryption expects.
pub fn subscription(endpoint: &str) -> Subscription {
    Subscription {
        endpoint: endpoint.to_owned(),
        p256dh: URL_SAFE_NO_PAD.encode(p256::SecretKey::generate().public_key().to_sec1_bytes()),
        auth: URL_SAFE_NO_PAD.encode([7_u8; 16]),
    }
}

/// A stand-in HTTP service.
///
/// Answers `count` requests with `status`, then reports what each one said.
/// Returns the base URL with no path, so a caller appends whatever path the
/// transport under test expects.
///
/// The reason phrase is a constant: no client in this tree parses it, and
/// deriving one from the status was a source of confusion rather than realism.
pub async fn stub(status: u16, count: usize) -> (String, JoinHandle<Vec<String>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("binds");
    let base = format!("http://{}", listener.local_addr().expect("has an address"));

    let served = tokio::spawn(async move {
        let mut requests = Vec::with_capacity(count);

        for _ in 0..count {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let mut buffer = vec![0_u8; 4096];
            let Ok(read) = stream.read(&mut buffer).await else {
                break;
            };
            requests.push(String::from_utf8_lossy(&buffer[..read]).into_owned());

            let response =
                format!("HTTP/1.1 {status} OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            if stream.write_all(response.as_bytes()).await.is_err() {
                break;
            }
            let _ = stream.shutdown().await;
        }

        requests
    });

    (base, served)
}
