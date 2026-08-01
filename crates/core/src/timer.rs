//! The pomodoro timer.
//!
//! The server owns elapsed time, not the phone. A focus block therefore
//! completes, gets logged and becomes notifiable even while the tab is suspended
//! or the device is asleep — and an agent can start or stop a session while the
//! user is nowhere near a browser.
//!
//! Two consequences show up throughout:
//!
//! - A block that is already due is logged at its **planned end**, never at
//!   "now". If the server was off for three hours, a 25-minute pomodoro is still
//!   twenty-five minutes.
//! - A session lands on the date it **started**, so one that crosses midnight
//!   stays a single line with `end < start`, matching the file grammar.

use chrono::NaiveDateTime;

use crate::active::{ActiveSession, SessionKind};
use crate::day::Session;
use crate::error::{Error, Result};
use crate::ids::ProjectSlug;
use crate::minutes::Minutes;
use crate::store::{Store, Tx};

/// What to start. Absent values fall back to the stored settings.
#[derive(Debug, Clone)]
pub struct StartRequest {
    pub kind: SessionKind,
    pub duration: Option<Minutes>,
    pub project: Option<ProjectSlug>,
    pub note: String,
}

impl StartRequest {
    pub fn focus(project: Option<ProjectSlug>, note: impl Into<String>) -> Self {
        Self {
            kind: SessionKind::Focus,
            duration: None,
            project,
            note: note.into(),
        }
    }
}

/// What stopping the timer did.
///
/// "Nothing was running" and "ran, but rounded to zero minutes" are different
/// events and read very differently to a user, so they are different variants
/// rather than a shared `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stopped {
    /// Nothing was running.
    Idle,
    /// A block was running, but was too short to be worth a line in the log.
    TooShort,
    /// The session that was written.
    Logged(Session),
}

/// What the timer looks like right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimerState {
    pub active: Option<ActiveSession>,
    /// Focus sessions already logged today.
    pub completed_today: u32,
    pub tracked_today: Minutes,
    /// Length of the break the settings would pick next, so the client can label
    /// its button without a second request.
    pub next_break: Minutes,
    pub next_break_kind: SessionKind,
}

pub struct Timer<'store> {
    store: &'store Store,
}

impl<'store> Timer<'store> {
    pub fn new(store: &'store Store) -> Self {
        Self { store }
    }

    /// Logs the running block if its time is up, and clears it.
    ///
    /// Safe to call from anywhere and as often as you like: it is a no-op unless
    /// something is actually due. Both the background tick and every read of the
    /// timer go through it, so the API stays correct even if the tick is late.
    pub fn settle(&self, now: NaiveDateTime) -> Result<Option<Session>> {
        self.store.transaction(|tx| {
            let Some(active) = tx.read_active()? else {
                return Ok(None);
            };
            if !active.is_due(now) {
                return Ok(None);
            }
            retire(tx, &active, active.ends_at())
        })
    }

    /// Starts a block, first settling or stopping whatever was running.
    pub fn start(&self, now: NaiveDateTime, request: StartRequest) -> Result<ActiveSession> {
        self.store.transaction(|tx| {
            stop_within(tx, now)?;

            let settings = tx.read_settings()?;
            let duration = request.duration.unwrap_or(match request.kind {
                SessionKind::Focus => settings.focus,
                SessionKind::ShortBreak => settings.short_break,
                SessionKind::LongBreak => settings.long_break,
            });

            // A zero-length block can never log anything, so refusing it here —
            // rather than in one front door — keeps the CLI and MCP honest too.
            if duration.is_zero() {
                return Err(Error::Invalid(
                    "a session must last at least a minute".to_owned(),
                ));
            }

            let active =
                ActiveSession::new(now, request.kind, duration, request.project, request.note);
            tx.set_active(Some(&active))?;
            Ok(active)
        })
    }

    /// Stops early, logging the part that was actually worked.
    pub fn stop(&self, now: NaiveDateTime) -> Result<Stopped> {
        self.store.transaction(|tx| stop_within(tx, now))
    }

    /// Drops the running block without logging it. Returns whether one was running.
    pub fn cancel(&self) -> Result<bool> {
        self.store.transaction(|tx| {
            let running = tx.read_active()?.is_some();
            if running {
                tx.set_active(None)?;
            }
            Ok(running)
        })
    }

    /// The current state, settling first so a due block is never reported as
    /// still running.
    pub fn state(&self, now: NaiveDateTime) -> Result<TimerState> {
        self.settle(now)?;

        let day = self.store.read_day(now.date())?;
        let settings = self.store.read_settings()?;
        let completed_today = u32::try_from(day.sessions().len()).unwrap_or(u32::MAX);

        // The break that follows the session just finished — or, if none has
        // finished yet, the one that would follow the first.
        let next_break = settings.break_after(completed_today.max(1));
        let next_break_kind = if next_break == settings.long_break {
            SessionKind::LongBreak
        } else {
            SessionKind::ShortBreak
        };

        Ok(TimerState {
            active: self.store.read_active()?,
            completed_today,
            tracked_today: day.total(),
            next_break,
            next_break_kind,
        })
    }
}

/// Ends the running block at `now`, or at its planned end if that has passed.
fn stop_within(tx: &Tx<'_>, now: NaiveDateTime) -> Result<Stopped> {
    let Some(active) = tx.read_active()? else {
        return Ok(Stopped::Idle);
    };
    // Stopping late must not inflate the log: a block whose time is already up
    // is worth its planned length, not however long it sat unattended.
    let ended = if active.is_due(now) {
        active.ends_at()
    } else {
        now
    };
    Ok(match retire(tx, &active, ended)? {
        Some(session) => Stopped::Logged(session),
        None => Stopped::TooShort,
    })
}

fn retire(tx: &Tx<'_>, active: &ActiveSession, ended: NaiveDateTime) -> Result<Option<Session>> {
    let logged = active.to_session(ended);
    if let Some(session) = &logged {
        // The start date, not the end date: a session crossing midnight belongs
        // to the day it began, as one line with `end < start`.
        tx.update_day(active.started.date(), |day| {
            day.add_session(session.clone());
        })?;
    }
    tx.set_active(None)?;
    Ok(logged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveTime};

    fn store() -> (tempfile::TempDir, Store) {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Store::new(directory.path());
        (directory, store)
    }

    fn moment(hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 8, 1)
            .expect("valid date")
            .and_hms_opt(hour, minute, 0)
            .expect("valid time")
    }

    fn slug(text: &str) -> Option<ProjectSlug> {
        Some(ProjectSlug::new(text).expect("valid slug"))
    }

    #[test]
    fn starting_writes_the_running_block_with_the_default_length() {
        let (_directory, store) = store();
        let timer = Timer::new(&store);

        let active = timer
            .start(
                moment(9, 0),
                StartRequest::focus(slug("timemd"), "file store"),
            )
            .expect("starts");

        assert_eq!(active.duration, Minutes::new(25));
        assert_eq!(active.kind, SessionKind::Focus);
        assert_eq!(store.read_active().expect("reads"), Some(active));
    }

    #[test]
    fn an_explicit_length_wins_over_the_settings() {
        let (_directory, store) = store();
        let request = StartRequest {
            duration: Some(Minutes::new(50)),
            ..StartRequest::focus(None, "long haul")
        };

        let active = Timer::new(&store)
            .start(moment(9, 0), request)
            .expect("starts");
        assert_eq!(active.duration, Minutes::new(50));
    }

    #[test]
    fn breaks_take_their_own_lengths_from_the_settings() {
        let (_directory, store) = store();
        let timer = Timer::new(&store);

        for (kind, expected) in [
            (SessionKind::ShortBreak, Minutes::new(5)),
            (SessionKind::LongBreak, Minutes::new(15)),
        ] {
            let request = StartRequest {
                kind,
                duration: None,
                project: None,
                note: String::new(),
            };
            let active = timer.start(moment(9, 0), request).expect("starts");
            assert_eq!(active.duration, expected, "{kind:?}");
        }
    }

    #[test]
    fn stopping_early_logs_only_the_time_worked() {
        let (_directory, store) = store();
        let timer = Timer::new(&store);
        timer
            .start(moment(9, 0), StartRequest::focus(slug("timemd"), "partial"))
            .expect("starts");

        let Stopped::Logged(logged) = timer.stop(moment(9, 10)).expect("stops") else {
            panic!("expected a logged session");
        };

        assert_eq!(logged.duration(), Minutes::new(10));
        assert_eq!(store.read_active().expect("reads"), None);
        assert_eq!(
            store.read_day(moment(9, 0).date()).expect("reads").total(),
            Minutes::new(10)
        );
    }

    #[test]
    fn settling_is_a_no_op_before_the_block_is_due() {
        let (_directory, store) = store();
        let timer = Timer::new(&store);
        timer
            .start(moment(9, 0), StartRequest::focus(None, ""))
            .expect("starts");

        assert_eq!(timer.settle(moment(9, 20)).expect("settles"), None);
        assert!(store.read_active().expect("reads").is_some());
    }

    #[test]
    fn settling_logs_a_due_block_and_clears_it() {
        let (_directory, store) = store();
        let timer = Timer::new(&store);
        timer
            .start(moment(9, 0), StartRequest::focus(slug("timemd"), "done"))
            .expect("starts");

        let logged = timer
            .settle(moment(9, 25))
            .expect("settles")
            .expect("logged");

        assert_eq!(logged.duration(), Minutes::new(25));
        assert_eq!(store.read_active().expect("reads"), None);
    }

    /// The property that makes the timer survive the server being asleep.
    #[test]
    fn a_long_overdue_block_is_logged_at_its_planned_length() {
        let (_directory, store) = store();
        let timer = Timer::new(&store);
        timer
            .start(moment(9, 0), StartRequest::focus(None, ""))
            .expect("starts");

        let logged = timer
            .settle(moment(15, 0))
            .expect("settles")
            .expect("logged");

        assert_eq!(logged.duration(), Minutes::new(25));
        assert_eq!(
            logged.end,
            NaiveTime::from_hms_opt(9, 25, 0).expect("valid time")
        );
    }

    #[test]
    fn stopping_an_overdue_block_also_uses_its_planned_length() {
        let (_directory, store) = store();
        let timer = Timer::new(&store);
        timer
            .start(moment(9, 0), StartRequest::focus(None, ""))
            .expect("starts");

        let Stopped::Logged(logged) = timer.stop(moment(15, 0)).expect("stops") else {
            panic!("expected a logged session");
        };
        assert_eq!(logged.duration(), Minutes::new(25));
    }

    #[test]
    fn settling_twice_logs_once() {
        let (_directory, store) = store();
        let timer = Timer::new(&store);
        timer
            .start(moment(9, 0), StartRequest::focus(None, ""))
            .expect("starts");

        assert!(timer.settle(moment(9, 25)).expect("settles").is_some());
        assert!(timer.settle(moment(9, 30)).expect("settles").is_none());
        assert_eq!(
            store
                .read_day(moment(9, 0).date())
                .expect("reads")
                .sessions()
                .len(),
            1
        );
    }

    #[test]
    fn a_zero_length_request_is_refused_before_anything_is_written() {
        let (_directory, store) = store();
        let request = StartRequest {
            duration: Some(Minutes::new(0)),
            ..StartRequest::focus(None, "")
        };

        assert!(Timer::new(&store).start(moment(9, 0), request).is_err());
        assert_eq!(store.read_active().expect("reads"), None);
    }

    #[test]
    fn stopping_after_settling_does_not_log_a_second_time() {
        let (_directory, store) = store();
        let timer = Timer::new(&store);
        timer
            .start(moment(9, 0), StartRequest::focus(None, ""))
            .expect("starts");

        timer.settle(moment(9, 25)).expect("settles");
        assert_eq!(timer.stop(moment(9, 25)).expect("stops"), Stopped::Idle);
        assert_eq!(
            store
                .read_day(moment(9, 0).date())
                .expect("reads")
                .sessions()
                .len(),
            1
        );
    }

    #[test]
    fn breaks_are_never_logged() {
        let (_directory, store) = store();
        let timer = Timer::new(&store);
        let request = StartRequest {
            kind: SessionKind::ShortBreak,
            duration: None,
            project: None,
            note: String::new(),
        };
        timer.start(moment(9, 0), request).expect("starts");

        assert_eq!(timer.settle(moment(9, 5)).expect("settles"), None);
        assert!(
            store
                .read_day(moment(9, 0).date())
                .expect("reads")
                .sessions()
                .is_empty()
        );
    }

    #[test]
    fn starting_while_running_logs_what_was_interrupted() {
        let (_directory, store) = store();
        let timer = Timer::new(&store);
        timer
            .start(moment(9, 0), StartRequest::focus(slug("timemd"), "first"))
            .expect("starts");
        timer
            .start(moment(9, 10), StartRequest::focus(slug("admin"), "second"))
            .expect("starts");

        let day = store.read_day(moment(9, 0).date()).expect("reads");
        assert_eq!(day.sessions().len(), 1);
        assert_eq!(day.sessions()[0].note, "first");
        assert_eq!(day.sessions()[0].duration(), Minutes::new(10));

        let active = store.read_active().expect("reads").expect("running");
        assert_eq!(active.note, "second");
    }

    #[test]
    fn cancelling_discards_without_logging() {
        let (_directory, store) = store();
        let timer = Timer::new(&store);
        timer
            .start(moment(9, 0), StartRequest::focus(None, ""))
            .expect("starts");

        assert!(timer.cancel().expect("cancels"));
        assert!(!timer.cancel().expect("cancels"));
        assert!(
            store
                .read_day(moment(9, 0).date())
                .expect("reads")
                .sessions()
                .is_empty()
        );
    }

    #[test]
    fn a_zero_length_block_is_not_worth_logging() {
        let (_directory, store) = store();
        let timer = Timer::new(&store);
        timer
            .start(moment(9, 0), StartRequest::focus(None, ""))
            .expect("starts");

        assert_eq!(timer.stop(moment(9, 0)).expect("stops"), Stopped::TooShort);
        assert!(
            store
                .read_day(moment(9, 0).date())
                .expect("reads")
                .sessions()
                .is_empty()
        );
    }

    #[test]
    fn a_block_crossing_midnight_lands_on_the_day_it_started() {
        let (_directory, store) = store();
        let timer = Timer::new(&store);
        let started = moment(23, 50);
        timer
            .start(started, StartRequest::focus(slug("timemd"), "late night"))
            .expect("starts");

        timer
            .settle(started + chrono::TimeDelta::minutes(30))
            .expect("settles");

        let day = store.read_day(started.date()).expect("reads");
        assert_eq!(day.sessions().len(), 1);
        assert_eq!(day.sessions()[0].duration(), Minutes::new(25));

        let next = store
            .read_day(started.date().succ_opt().expect("valid date"))
            .expect("reads");
        assert!(next.sessions().is_empty());
    }

    #[test]
    fn state_reports_the_day_and_settles_on_the_way() {
        let (_directory, store) = store();
        let timer = Timer::new(&store);
        timer
            .start(moment(9, 0), StartRequest::focus(slug("timemd"), "done"))
            .expect("starts");

        let state = timer.state(moment(9, 30)).expect("reads");

        assert_eq!(state.active, None, "a due block must not read as running");
        assert_eq!(state.completed_today, 1);
        assert_eq!(state.tracked_today, Minutes::new(25));
        assert_eq!(state.next_break, Minutes::new(5));
        assert_eq!(state.next_break_kind, SessionKind::ShortBreak);
    }

    #[test]
    fn a_long_break_is_suggested_on_the_interval() {
        let (_directory, store) = store();
        let timer = Timer::new(&store);

        for hour in 9..13 {
            timer
                .start(moment(hour, 0), StartRequest::focus(slug("timemd"), ""))
                .expect("starts");
            timer.settle(moment(hour, 25)).expect("settles");
        }

        let state = timer.state(moment(13, 0)).expect("reads");
        assert_eq!(state.completed_today, 4);
        assert_eq!(state.next_break, Minutes::new(15));
        assert_eq!(state.next_break_kind, SessionKind::LongBreak);
    }

    #[test]
    fn an_idle_timer_reports_an_empty_day() {
        let (_directory, store) = store();
        let state = Timer::new(&store).state(moment(9, 0)).expect("reads");

        assert_eq!(state.active, None);
        assert_eq!(state.completed_today, 0);
        assert_eq!(state.tracked_today, Minutes::new(0));
        assert_eq!(state.next_break, Minutes::new(5));
    }
}
