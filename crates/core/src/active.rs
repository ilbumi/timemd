//! `data/state/active.md` — the running timer.
//!
//! The server, not the phone, owns elapsed time: a focus block completes, gets
//! logged and fires its notification even while the tab is suspended. Keeping
//! that state in a readable file is also what lets an agent answer "what are you
//! working on right now" without the web app running.

use chrono::{NaiveDateTime, NaiveTime, TimeDelta};
use serde::{Deserialize, Serialize};

use crate::day::Session;
use crate::document::Document;
use crate::ids::ProjectSlug;
use crate::minutes::Minutes;

/// File contents when nothing is running.
pub const IDLE: &str = "---\n---\n\n# Active session\n\nNothing running.\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    Focus,
    ShortBreak,
    LongBreak,
}

impl SessionKind {
    /// Only focus blocks become logged sessions; breaks are timer state.
    pub fn is_focus(self) -> bool {
        matches!(self, Self::Focus)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActiveSession {
    pub started: NaiveDateTime,
    pub kind: SessionKind,
    pub duration: Minutes,
    pub project: Option<ProjectSlug>,
    pub note: String,
}

impl ActiveSession {
    pub fn new(
        started: NaiveDateTime,
        kind: SessionKind,
        duration: Minutes,
        project: Option<ProjectSlug>,
        note: impl Into<String>,
    ) -> Self {
        Self {
            started,
            kind,
            duration,
            project,
            note: note.into(),
        }
    }

    /// Reads the state file. A file with no `started` key means idle.
    pub fn parse(text: &str) -> Result<Option<Self>, yaml_serde::Error> {
        let document = Document::parse(text)?;
        let Some(started) = document
            .front_key::<String>("started")
            .and_then(|raw| raw.parse::<NaiveDateTime>().ok())
        else {
            return Ok(None);
        };
        Ok(Some(Self {
            started,
            kind: document.front_key("kind").unwrap_or(SessionKind::Focus),
            duration: document.front_key("duration").unwrap_or_default(),
            project: document.front_key("project"),
            note: document.front_key("note").unwrap_or_default(),
        }))
    }

    pub fn render(&self) -> String {
        let mut document = Document::new();
        document.set_front_key(
            "started",
            &self.started.format("%Y-%m-%dT%H:%M:%S").to_string(),
        );
        document.set_front_key("kind", &self.kind);
        document.set_front_key("duration", &self.duration);
        if let Some(project) = &self.project {
            document.set_front_key("project", project);
        }
        if !self.note.is_empty() {
            document.set_front_key("note", &self.note);
        }
        document.set_preamble(vec![
            String::new(),
            "# Active session".to_owned(),
            String::new(),
        ]);
        document.render()
    }

    /// When the timer is due to retire this block.
    pub fn ends_at(&self) -> NaiveDateTime {
        self.started + TimeDelta::minutes(i64::from(self.duration.get()))
    }

    /// Whole minutes elapsed, clamped at zero if the clock moved backwards.
    pub fn elapsed(&self, now: NaiveDateTime) -> Minutes {
        let seconds = (now - self.started).num_seconds().max(0);
        Minutes::new(u32::try_from(seconds / 60).unwrap_or(u32::MAX))
    }

    /// Whole minutes left, zero once due.
    pub fn remaining(&self, now: NaiveDateTime) -> Minutes {
        Minutes::new(self.duration.get().saturating_sub(self.elapsed(now).get()))
    }

    pub fn is_due(&self, now: NaiveDateTime) -> bool {
        now >= self.ends_at()
    }

    /// Converts a finished focus block into the session to log.
    ///
    /// Returns `None` for breaks, and for a focus block that rounded to zero
    /// minutes — logging `09:00-09:00` would be noise, not data.
    pub fn to_session(&self, ended: NaiveDateTime) -> Option<Session> {
        if !self.kind.is_focus() {
            return None;
        }
        let start: NaiveTime = self.started.time();
        let end: NaiveTime = ended.time();
        let session = Session::new(start, end, self.project.clone(), self.note.clone());
        (!session.duration().is_zero()).then_some(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moment(hours: u32, minutes: u32) -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 8, 1)
            .expect("valid date")
            .and_hms_opt(hours, minutes, 0)
            .expect("valid time")
    }

    fn running() -> ActiveSession {
        ActiveSession::new(
            moment(9, 0),
            SessionKind::Focus,
            Minutes::new(25),
            Some(ProjectSlug::new("timemd").expect("valid slug")),
            "file store layer",
        )
    }

    #[test]
    fn an_idle_file_parses_as_nothing_running() {
        assert_eq!(ActiveSession::parse(IDLE).expect("parses"), None);
        assert_eq!(ActiveSession::parse("").expect("parses"), None);
        assert_eq!(ActiveSession::parse("---\nkind: focus\n---\n").expect("parses"), None);
    }

    #[test]
    fn round_trips_through_render_and_parse() {
        let session = running();
        let reparsed = ActiveSession::parse(&session.render()).expect("parses");
        assert_eq!(reparsed, Some(session));
    }

    #[test]
    fn renders_a_readable_file() {
        assert_eq!(
            running().render(),
            "---\nstarted: 2026-08-01T09:00:00\nkind: focus\nduration: 25m\nproject: timemd\nnote: file store layer\n---\n\n# Active session\n"
        );
    }

    #[test]
    fn omits_absent_project_and_note() {
        let session = ActiveSession::new(moment(9, 0), SessionKind::ShortBreak, Minutes::new(5), None, "");
        let rendered = session.render();
        assert!(!rendered.contains("project:"), "{rendered}");
        assert!(!rendered.contains("note:"), "{rendered}");
        assert_eq!(ActiveSession::parse(&rendered).expect("parses"), Some(session));
    }

    #[test]
    fn tracks_elapsed_and_remaining() {
        let session = running();
        assert_eq!(session.elapsed(moment(9, 10)), Minutes::new(10));
        assert_eq!(session.remaining(moment(9, 10)), Minutes::new(15));
        assert_eq!(session.ends_at(), moment(9, 25));
        assert!(!session.is_due(moment(9, 24)));
        assert!(session.is_due(moment(9, 25)));
    }

    #[test]
    fn clamps_when_the_clock_moves_backwards_or_overruns() {
        let session = running();
        assert_eq!(session.elapsed(moment(8, 0)), Minutes::new(0));
        assert_eq!(session.remaining(moment(8, 0)), Minutes::new(25));
        assert_eq!(session.remaining(moment(23, 0)), Minutes::new(0));
    }

    #[test]
    fn a_finished_focus_block_becomes_a_session() {
        let session = running().to_session(moment(9, 25)).expect("logs");
        assert_eq!(session.duration(), Minutes::new(25));
        assert_eq!(session.note, "file store layer");
        assert_eq!(
            session.project.as_ref().map(ProjectSlug::as_str),
            Some("timemd")
        );
    }

    #[test]
    fn breaks_and_zero_length_blocks_are_never_logged() {
        let rest = ActiveSession::new(moment(9, 0), SessionKind::ShortBreak, Minutes::new(5), None, "");
        assert_eq!(rest.to_session(moment(9, 5)), None);
        assert_eq!(running().to_session(moment(9, 0)), None);
    }

    #[test]
    fn only_focus_blocks_are_loggable() {
        assert!(SessionKind::Focus.is_focus());
        assert!(!SessionKind::ShortBreak.is_focus());
        assert!(!SessionKind::LongBreak.is_focus());
    }
}
