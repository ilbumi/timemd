//! The background tick: retires finished sessions and fires reminders.
//!
//! Reading `/api/timer` also settles, so the API is correct without this. What
//! the ticker adds is the part a phone cannot do for itself — a pomodoro that
//! finishes while the tab is suspended still lands in the day file at the right
//! time and still notifies, and a schedule block still warns you before it
//! starts even if nobody has the app open.

use std::time::Duration;

use chrono::NaiveDateTime;
use timemd_core::{Reminder, Timer, reminders};

use crate::push::{self, Notification};
use crate::state::AppState;

/// Fine enough that a completed pomodoro is logged within half a minute, coarse
/// enough to be invisible on a small server. Reminder lead times are whole
/// minutes, so this is comfortably inside any window.
const INTERVAL: Duration = Duration::from_secs(30);

pub async fn run(state: AppState) {
    let mut ticks = tokio::time::interval(INTERVAL);
    // Skip catching up after a pause: a delayed tick should happen once, not
    // once per interval that was missed.
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        ticks.tick().await;

        let Ok(now) = state.local_now() else {
            tracing::warn!("could not read settings to establish the local time; skipping tick");
            continue;
        };

        if let Some(notification) = settle_once(&state, now) {
            push::deliver(&state, &notification).await;
        }
        for notification in reminders_due(&state, now) {
            push::deliver(&state, &notification).await;
        }
    }
}

/// Retires a finished session, returning the notification it deserves.
fn settle_once(state: &AppState, now: NaiveDateTime) -> Option<Notification> {
    match Timer::new(state.store()).settle(now) {
        Ok(Some(session)) => {
            tracing::info!(
                duration = %session.duration(),
                project = session.project.as_ref().map_or("-", |slug| slug.as_str()),
                "logged a finished session",
            );
            Some(Notification {
                title: "Session complete".to_owned(),
                body: format!(
                    "{} logged to {}",
                    session.duration(),
                    session
                        .project
                        .as_ref()
                        .map_or("no project", |slug| slug.as_str()),
                ),
                url: "/".to_owned(),
            })
        }
        Ok(None) => None,
        // A failing tick must not kill the loop: the markdown tree may be
        // momentarily unreadable, and the next tick is thirty seconds away.
        Err(error) => {
            tracing::error!("could not settle the timer: {error}");
            None
        }
    }
}

/// Finds reminders whose lead window has opened and records them as sent.
///
/// Recording happens before delivery, and deliberately: sending twice because a
/// push failed is worse than missing one, since the phone would buzz again on
/// every tick until the block began.
fn reminders_due(state: &AppState, now: NaiveDateTime) -> Vec<Notification> {
    let due = match collect(state, now) {
        Ok(due) => due,
        Err(error) => {
            tracing::error!("could not work out which reminders are due: {error}");
            return Vec::new();
        }
    };

    if due.is_empty() {
        return Vec::new();
    }

    let recorded = state.store().update_sent_reminders(now, |sent| {
        for reminder in &due {
            sent.record(reminder.key.clone());
        }
    });
    if let Err(error) = recorded {
        tracing::error!("could not record sent reminders: {error}");
        return Vec::new();
    }

    due.into_iter()
        .map(|reminder| {
            tracing::info!(block = reminder.title, "reminder due");
            Notification {
                title: reminder.title.clone(),
                body: reminder.body(),
                url: "/today".to_owned(),
            }
        })
        .collect()
}

fn collect(state: &AppState, now: NaiveDateTime) -> timemd_core::Result<Vec<Reminder>> {
    let store = state.store();
    Ok(reminders::due(
        &store.read_day(now.date())?,
        &store.read_recurring()?,
        &store.read_settings()?,
        now,
        &store.read_sent_reminders()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Clock;
    use chrono::{TimeZone, Utc};
    use std::sync::Arc;
    use timemd_core::{Minutes, StartRequest, Store};

    fn instant(hour: u32, minute: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 5, hour, minute, 0)
            .single()
            .expect("valid instant")
    }

    fn state() -> (tempfile::TempDir, Arc<Store>, Clock, AppState) {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Arc::new(Store::new(directory.path()));
        store
            .update_settings(|settings| settings.timezone = chrono_tz::UTC)
            .expect("writes settings");
        let clock = Clock::fixed(instant(9, 0));
        let state = AppState::new(Arc::clone(&store), clock.clone());
        (directory, store, clock, state)
    }

    fn with_deep_work(store: &Store) {
        store
            .update_recurring(|recurring| {
                recurring.upsert(timemd_core::RecurringBlock {
                    id: timemd_core::BlockId::new("deep-work").expect("valid id"),
                    days: timemd_core::DaySet::ALL,
                    start: chrono::NaiveTime::from_hms_opt(10, 0, 0).expect("valid time"),
                    end: chrono::NaiveTime::from_hms_opt(11, 0, 0).expect("valid time"),
                    project: None,
                    title: "Deep work".to_owned(),
                    remind_before: Some(Minutes::new(5)),
                });
            })
            .expect("writes");
    }

    #[test]
    fn a_tick_logs_a_finished_session_and_says_so() {
        let (_directory, store, clock, state) = state();
        let now = state.local_now().expect("reads");
        Timer::new(&store)
            .start(now, StartRequest::focus(None, "work"))
            .expect("starts");

        // Not yet due: the tick must leave it alone.
        assert_eq!(settle_once(&state, state.local_now().expect("reads")), None);
        assert!(store.read_active().expect("reads").is_some());

        clock.set(instant(9, 30));
        let notification = settle_once(&state, state.local_now().expect("reads"))
            .expect("a finished session is worth a notification");

        assert_eq!(notification.title, "Session complete");
        assert!(notification.body.contains("25m"), "{}", notification.body);
        assert_eq!(store.read_active().expect("reads"), None);
        assert_eq!(
            store.read_day(now.date()).expect("reads").total(),
            Minutes::new(25)
        );
    }

    #[test]
    fn a_tick_on_an_idle_timer_does_nothing() {
        let (_directory, store, _clock, state) = state();
        assert_eq!(settle_once(&state, state.local_now().expect("reads")), None);
        assert_eq!(store.read_active().expect("reads"), None);
    }

    #[test]
    fn a_reminder_fires_once_inside_its_window() {
        let (_directory, _store, clock, state) = state();
        with_deep_work(state.store());

        // Well before the window.
        assert!(reminders_due(&state, state.local_now().expect("reads")).is_empty());

        clock.set(instant(9, 56));
        let due = reminders_due(&state, state.local_now().expect("reads"));
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].title, "Deep work");
        assert_eq!(due[0].url, "/today");
    }

    /// The property that stops the phone buzzing every thirty seconds for five
    /// minutes, and that survives a restart mid-window.
    #[test]
    fn a_reminder_does_not_fire_again_on_the_next_tick() {
        let (_directory, store, clock, state) = state();
        with_deep_work(state.store());

        clock.set(instant(9, 56));
        assert_eq!(
            reminders_due(&state, state.local_now().expect("reads")).len(),
            1
        );

        clock.set(instant(9, 57));
        assert!(reminders_due(&state, state.local_now().expect("reads")).is_empty());

        // A restart re-reads the log from disk rather than trusting memory.
        let restarted = AppState::new(Arc::clone(&store), clock.clone());
        assert!(reminders_due(&restarted, restarted.local_now().expect("reads")).is_empty());
    }

    #[test]
    fn a_skipped_block_never_reminds() {
        let (_directory, store, clock, state) = state();
        with_deep_work(state.store());
        store
            .update_day(instant(9, 0).date_naive(), |day| {
                day.skip(timemd_core::BlockId::new("deep-work").expect("valid id"));
            })
            .expect("writes");

        clock.set(instant(9, 56));
        assert!(reminders_due(&state, state.local_now().expect("reads")).is_empty());
    }

    #[test]
    fn nothing_fires_once_the_block_has_begun() {
        let (_directory, _store, clock, state) = state();
        with_deep_work(state.store());

        clock.set(instant(10, 1));
        assert!(reminders_due(&state, state.local_now().expect("reads")).is_empty());
    }
}
