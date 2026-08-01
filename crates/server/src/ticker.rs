//! The background tick that retires finished sessions.
//!
//! Reading `/api/timer` also settles, so the API is correct without this. What
//! the ticker adds is the part a phone cannot do for itself: a pomodoro that
//! finishes while the tab is suspended still lands in the day file at the right
//! time, and — once M5 lands — still fires its notification.

use std::time::Duration;

use timemd_core::Timer;

use crate::state::AppState;

/// Fine enough that a completed pomodoro is logged within half a minute, coarse
/// enough to be invisible on a small server.
const INTERVAL: Duration = Duration::from_secs(30);

pub async fn run(state: AppState) {
    let mut ticks = tokio::time::interval(INTERVAL);
    // Skip catching up after a pause: a delayed tick should happen once, not
    // once per interval that was missed.
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        ticks.tick().await;
        settle_once(&state);
    }
}

fn settle_once(state: &AppState) {
    let Ok(now) = state.local_now() else {
        tracing::warn!("could not read settings to establish the local time; skipping tick");
        return;
    };

    match Timer::new(state.store()).settle(now) {
        Ok(Some(session)) => tracing::info!(
            duration = %session.duration(),
            project = session.project.as_ref().map_or("-", |slug| slug.as_str()),
            "logged a finished session",
        ),
        Ok(None) => {}
        // A failing tick must not kill the loop: the markdown tree may be
        // momentarily unreadable, and the next tick is 30 seconds away.
        Err(error) => tracing::error!("could not settle the timer: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Clock;
    use chrono::{TimeZone, Utc};
    use std::sync::Arc;
    use timemd_core::{Minutes, StartRequest, Store};

    #[test]
    fn a_tick_logs_a_finished_session() {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Arc::new(Store::new(directory.path()));
        let clock = Clock::fixed(
            Utc.with_ymd_and_hms(2026, 8, 1, 9, 0, 0)
                .single()
                .expect("valid instant"),
        );
        let state = AppState::new(Arc::clone(&store), clock.clone());

        let now = state.local_now().expect("reads");
        Timer::new(&store)
            .start(now, StartRequest::focus(None, "work"))
            .expect("starts");

        // Not yet due: the tick must leave it alone.
        settle_once(&state);
        assert!(store.read_active().expect("reads").is_some());

        clock.set(
            Utc.with_ymd_and_hms(2026, 8, 1, 9, 30, 0)
                .single()
                .expect("valid instant"),
        );
        settle_once(&state);

        assert_eq!(store.read_active().expect("reads"), None);
        assert_eq!(
            store.read_day(now.date()).expect("reads").total(),
            Minutes::new(25)
        );
    }

    #[test]
    fn a_tick_on_an_idle_timer_does_nothing() {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Arc::new(Store::new(directory.path()));
        let state = AppState::new(Arc::clone(&store), Clock::System);

        settle_once(&state);

        assert_eq!(store.read_active().expect("reads"), None);
    }
}
