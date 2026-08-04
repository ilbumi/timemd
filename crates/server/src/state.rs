//! Shared handler state: the store, and the clock everything reads time from.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use timemd_core::{Result, Store};

/// The source of "now".
///
/// Injectable because the timer is server-authoritative: without a clock a test
/// can move, verifying that a session retires on time would mean sleeping.
#[derive(Clone, Debug)]
pub enum Clock {
    System,
    /// A settable instant, shared so a test can advance it mid-run.
    Fixed(Arc<Mutex<DateTime<Utc>>>),
}

impl Clock {
    pub fn fixed(instant: DateTime<Utc>) -> Self {
        Self::Fixed(Arc::new(Mutex::new(instant)))
    }

    pub fn now_utc(&self) -> DateTime<Utc> {
        match self {
            Self::System => Utc::now(),
            Self::Fixed(instant) => *instant
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        }
    }

    /// Moves a fixed clock. A no-op on the system clock.
    pub fn set(&self, instant: DateTime<Utc>) {
        if let Self::Fixed(current) = self {
            *current
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = instant;
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    store: Arc<Store>,
    clock: Clock,
    /// Shared so notification delivery reuses TLS connections instead of paying
    /// a fresh handshake per message. Cloning is cheap — the client is
    /// internally reference-counted.
    http: reqwest::Client,
}

/// Comfortably inside the ticker's interval, so a host that accepts a
/// connection and then says nothing costs one tick's latency rather than
/// stalling the loop for however long the OS is prepared to wait.
const HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

impl AppState {
    pub fn new(store: Arc<Store>, clock: Clock) -> Self {
        Self {
            store,
            clock,
            http: reqwest::Client::builder()
                .timeout(HTTP_TIMEOUT)
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    /// Now, as wall-clock time in the configured timezone.
    ///
    /// Files carry no offsets, so this conversion is the single point where an
    /// instant becomes the local time the grammar stores — and it lives on the
    /// store, so the server reaches it the same way the CLI and MCP do.
    pub fn local_now(&self) -> Result<NaiveDateTime> {
        self.store.wall_clock(self.clock.now_utc())
    }

    /// The same conversion, for a caller holding the settings already.
    ///
    /// The ticker and the timer handlers both need another field of the same
    /// file, so they read it once and come here rather than paying for a second
    /// parse — without inventing a second spelling of what "now" means.
    pub fn local_now_with(&self, settings: &timemd_core::Settings) -> NaiveDateTime {
        settings.wall_clock(self.clock.now_utc())
    }

    pub fn today(&self) -> Result<NaiveDate> {
        Ok(self.local_now()?.date())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::path::Path;
    use timemd_core::Minutes;

    fn instant(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 1, hour, 0, 0)
            .single()
            .expect("valid instant")
    }

    fn state_on(root: &Path, clock: Clock) -> AppState {
        AppState::new(Arc::new(Store::new(root)), clock)
    }

    #[test]
    fn a_fixed_clock_reports_and_accepts_a_new_instant() {
        let clock = Clock::fixed(instant(9));
        assert_eq!(clock.now_utc(), instant(9));

        clock.set(instant(14));
        assert_eq!(clock.now_utc(), instant(14));
    }

    #[test]
    fn the_system_clock_advances_and_ignores_being_set() {
        let clock = Clock::System;
        let before = clock.now_utc();
        clock.set(instant(9));

        // Setting it is a no-op rather than an error: the caller does not need to
        // know which kind of clock it holds.
        assert!(clock.now_utc() >= before);
        assert_ne!(clock.now_utc(), instant(9));
    }

    #[test]
    fn local_time_follows_the_configured_timezone() {
        let directory = tempfile::tempdir().expect("temp dir");
        let state = state_on(directory.path(), Clock::fixed(instant(23)));
        state
            .store()
            .update_settings(|settings| settings.timezone = chrono_tz::Europe::Berlin)
            .expect("writes settings");

        // 23:00 UTC on 1 August is 01:00 the next day in Berlin, so both the time
        // and the date must shift — this is the conversion every stored wall-clock
        // time depends on.
        let now = state.local_now().expect("reads");
        assert_eq!(now.format("%Y-%m-%d %H:%M").to_string(), "2026-08-02 01:00");
        assert_eq!(
            state.today().expect("reads"),
            chrono::NaiveDate::from_ymd_opt(2026, 8, 2).expect("valid date")
        );
    }

    #[test]
    fn an_empty_tree_still_yields_a_clock_and_a_store() {
        let directory = tempfile::tempdir().expect("temp dir");
        let state = state_on(directory.path(), Clock::fixed(instant(9)));

        assert_eq!(state.clock().now_utc(), instant(9));
        assert_eq!(
            state.store().read_settings().expect("reads").focus,
            Minutes::new(25)
        );
    }
}
