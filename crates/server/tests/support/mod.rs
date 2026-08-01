//! Shared harness: an app wired to a throwaway markdown tree and a frozen clock.

// Cargo compiles this module separately into every integration-test binary, so
// a helper only some of them use reads as dead code in the rest. The alternative
// is duplicating the harness per file, which is worse.
#![allow(dead_code)]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{DateTime, TimeZone, Utc};
use http_body_util::BodyExt;
use serde_json::Value;
use tempfile::TempDir;
use timemd_core::Store;
use timemd_server::state::{AppState, Clock};
use tower::ServiceExt;

pub struct Harness {
    pub router: Router,
    pub clock: Clock,
    pub store: Arc<Store>,
    _directory: TempDir,
}

impl Harness {
    /// Starts at 2026-08-01T09:00:00Z, with the timezone pinned to UTC.
    ///
    /// Settings otherwise default to the host's timezone, which would make every
    /// wall-clock assertion here depend on where the machine running the tests
    /// happens to be.
    pub fn new() -> Self {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Arc::new(Store::new(directory.path()));
        store
            .update_settings(|settings| settings.timezone = chrono_tz::UTC)
            .expect("writes settings");
        let clock = Clock::fixed(
            Utc.with_ymd_and_hms(2026, 8, 1, 9, 0, 0)
                .single()
                .expect("valid instant"),
        );
        let state = AppState::new(Arc::clone(&store), clock.clone());
        Self {
            router: timemd_server::router(state),
            clock,
            store,
            _directory: directory,
        }
    }

    pub async fn request(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let builder = Request::builder().method(method).uri(uri);
        let request = match body {
            Some(payload) => builder
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .expect("request builds"),
            None => builder.body(Body::empty()).expect("request builds"),
        };

        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("router responds");
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body collects")
            .to_bytes();
        let json = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        (status, json)
    }

    pub async fn get(&self, uri: &str) -> (StatusCode, Value) {
        self.request("GET", uri, None).await
    }

    pub async fn post(&self, uri: &str, body: Value) -> (StatusCode, Value) {
        self.request("POST", uri, Some(body)).await
    }

    pub async fn patch(&self, uri: &str, body: Value) -> (StatusCode, Value) {
        self.request("PATCH", uri, Some(body)).await
    }

    pub async fn delete(&self, uri: &str) -> (StatusCode, Value) {
        self.request("DELETE", uri, None).await
    }
}

/// A UTC instant. The harness pins the timezone to UTC, so these are also the
/// wall-clock times that end up in the files.
pub fn instant(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
        .single()
        .expect("valid instant")
}
