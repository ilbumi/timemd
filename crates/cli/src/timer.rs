//! `start`, `stop`, `cancel` and `status` — the running session.

use chrono::NaiveDateTime;
use timemd_core::active::SessionKind;
use timemd_core::{Minutes, ProjectSlug, Result, StartRequest, Stopped, Store, Timer};

use crate::{name_or_dash, suffix};

pub fn start(
    store: &Store,
    project: Option<String>,
    note: String,
    duration: Option<String>,
    now: NaiveDateTime,
) -> Result<String> {
    let request = StartRequest {
        kind: SessionKind::Focus,
        duration: duration.map(|raw| raw.parse::<Minutes>()).transpose()?,
        project: project.map(ProjectSlug::new).transpose()?,
        note,
    };
    let active = Timer::new(store).start(now, request)?;
    Ok(format!(
        "started {} → {}{}",
        active.started.format("%H:%M"),
        name_or_dash(active.project.as_ref()),
        suffix(&active.note),
    ))
}

pub fn stop(store: &Store, now: NaiveDateTime) -> Result<String> {
    Ok(match Timer::new(store).stop(now)? {
        Stopped::Logged(session) => format!(
            "logged {} to {}",
            session.duration(),
            name_or_dash(session.project.as_ref()),
        ),
        Stopped::TooShort => "stopped — under a minute, so nothing was logged".to_owned(),
        Stopped::Idle => "nothing was running".to_owned(),
    })
}

pub fn cancel(store: &Store) -> Result<String> {
    Ok(if Timer::new(store).cancel()? {
        "discarded".to_owned()
    } else {
        "nothing was running".to_owned()
    })
}

pub fn status(store: &Store, now: NaiveDateTime) -> Result<String> {
    let state = Timer::new(store).state(now)?;
    Ok(match state.active {
        Some(active) => format!(
            "{} on {} — {} left{}",
            active.kind,
            name_or_dash(active.project.as_ref()),
            active.remaining(now),
            suffix(&active.note),
        ),
        None => format!(
            "idle — {} tracked today across {} session(s)",
            state.tracked_today, state.completed_today
        ),
    })
}

#[cfg(test)]
mod tests {
    use crate::testing::{moment, start, store};
    use crate::{Command, run};
    use timemd_core::Minutes;

    #[test]
    fn start_then_stop_logs_the_time_worked() {
        let (_directory, store) = store();

        let started = run(
            &store,
            Command::Start {
                project: Some("timemd".to_owned()),
                note: "file store".to_owned(),
                duration: None,
            },
            moment(9, 0),
        )
        .expect("starts");
        assert!(started.contains("timemd"), "{started}");
        assert!(started.contains("file store"), "{started}");

        let stopped = run(&store, Command::Stop, moment(9, 10)).expect("stops");
        assert!(stopped.contains("10m"), "{stopped}");
        assert_eq!(
            store.read_day(moment(9, 0).date()).expect("reads").total(),
            Minutes::new(10)
        );
    }

    #[test]
    fn stopping_immediately_says_it_was_too_short_rather_than_idle() {
        let (_directory, store) = store();
        run(&store, start(Some("timemd"), None), moment(9, 0)).expect("starts");

        let stopped = run(&store, Command::Stop, moment(9, 0)).expect("stops");
        assert!(stopped.contains("under a minute"), "{stopped}");
        assert!(!stopped.contains("nothing was running"), "{stopped}");
    }

    #[test]
    fn stopping_or_cancelling_nothing_says_so() {
        let (_directory, store) = store();
        assert_eq!(
            run(&store, Command::Stop, moment(9, 0)).expect("stops"),
            "nothing was running"
        );
        assert_eq!(
            run(&store, Command::Cancel, moment(9, 0)).expect("cancels"),
            "nothing was running"
        );
    }

    #[test]
    fn cancel_discards_without_logging() {
        let (_directory, store) = store();
        run(&store, start(None, None), moment(9, 0)).expect("starts");

        assert_eq!(
            run(&store, Command::Cancel, moment(9, 10)).expect("cancels"),
            "discarded"
        );
        assert!(
            store
                .read_day(moment(9, 0).date())
                .expect("reads")
                .sessions()
                .is_empty()
        );
    }

    #[test]
    fn status_reports_running_and_idle() {
        let (_directory, store) = store();

        let idle = run(&store, Command::Status, moment(9, 0)).expect("reads");
        assert!(idle.starts_with("idle"), "{idle}");

        run(&store, start(Some("timemd"), Some("50m")), moment(9, 0)).expect("starts");

        let running = run(&store, Command::Status, moment(9, 20)).expect("reads");
        assert!(running.contains("focus on timemd"), "{running}");
        assert!(running.contains("30m left"), "{running}");
    }

    #[test]
    fn an_invalid_slug_or_duration_is_rejected() {
        let (_directory, store) = store();

        assert!(run(&store, start(Some("Not A Slug"), None), moment(9, 0)).is_err());
        assert!(run(&store, start(None, Some("ages")), moment(9, 0)).is_err());
    }
}
