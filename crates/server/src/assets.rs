//! The web UI, compiled into the binary.
//!
//! Embedding is what makes the deploy story "copy one file": there is no asset
//! directory to keep in sync with the executable, and no static-file server to
//! configure in front of it.

use axum::body::Body;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::{Embed, EmbeddedFile};

#[derive(Embed)]
#[folder = "assets/"]
// The UI is a build artifact, so a fresh clone has no `assets/` at all. Without
// this the crate would not compile until pnpm had run; with it the binary simply
// has no shell, which `serve` below already reports.
#[allow_missing = true]
struct Assets;

/// Hashed bundles never change under a given name, so they can be cached hard.
const IMMUTABLE_PREFIX: &str = "_app/immutable/";

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    if let Some(file) = Assets::get(path) {
        return respond(path, &file);
    }

    // Anything else is a client-side route, so hand back the shell and let the
    // router sort it out. A request for a genuinely missing asset lands here
    // too; the app renders its own not-found rather than the server guessing.
    match Assets::get("index.html") {
        Some(file) => respond("index.html", &file),
        None => (
            StatusCode::NOT_FOUND,
            "The web UI was not built into this binary. Run `make frontend` and rebuild.",
        )
            .into_response(),
    }
}

/// How long a given asset may be cached.
///
/// Split out as a pure function because the branch matters — getting it backwards
/// means either a stale app after every deploy, or refetching every bundle on
/// every load over a phone connection.
fn cache_control(path: &str) -> &'static str {
    if path.starts_with(IMMUTABLE_PREFIX) {
        "public, max-age=31536000, immutable"
    } else {
        // The shell must be revalidated or a deploy would never reach the phone.
        "no-cache"
    }
}

fn respond(path: &str, file: &EmbeddedFile) -> Response {
    let cache = cache_control(path);

    (
        [
            (
                header::CONTENT_TYPE,
                mime_guess::from_path(path)
                    .first_or_octet_stream()
                    .to_string(),
            ),
            (header::CACHE_CONTROL, cache.to_owned()),
        ],
        Body::from(file.data.to_vec()),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashed_bundles_are_cached_forever() {
        let forever = cache_control("_app/immutable/chunks/entry.abc123.js");
        assert!(forever.contains("immutable"), "{forever}");
        assert!(forever.contains("max-age=31536000"), "{forever}");
    }

    #[test]
    fn the_shell_and_loose_assets_are_revalidated() {
        for path in ["index.html", "favicon.svg", "manifest.webmanifest"] {
            assert_eq!(cache_control(path), "no-cache", "{path}");
        }
    }
}
